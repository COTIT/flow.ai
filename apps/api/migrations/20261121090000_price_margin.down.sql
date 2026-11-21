-- Back to the 20261120090000 body (margin inline), then drop price_margin.
CREATE OR REPLACE FUNCTION price_lines(dp int, qs numeric[], ps numeric[], vs numeric[], ds numeric[],
                            pps numeric[], scs boolean[])
    RETURNS TABLE (n int, gross numeric, discount numeric, excl numeric, line_vat numeric,
                   rate numeric, rate_first int, rate_base numeric, rate_vat numeric,
                   gross_total numeric, discount_total numeric, excl_total numeric,
                   vat_total numeric, total numeric, cost numeric, revenue numeric,
                   margin_amount numeric, margin_percentage float8,
                   overflow_at int, margin_overflow_at int)
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
AS $$
WITH k AS (SELECT 79228162514264337593543950335::numeric AS big),
i AS (
    SELECT (u.o - 1)::int AS n, u.q, u.p, u.v, u.d, coalesce(u.pp, 0) AS pp,
           coalesce(u.sc, false) AS sc, u.q * u.p AS qp, round(u.q * u.p, dp) AS gross
    FROM unnest(qs, ps, vs, ds, pps, scs) WITH ORDINALITY u(q, p, v, d, pp, sc, o)
), x AS (
    SELECT i.*, i.gross * i.d AS gd, round(i.gross * i.d * 0.01, dp) AS disc FROM i
), e AS (
    SELECT x.*, x.gross - x.disc AS excl FROM x
), l AS (
    SELECT e.*, e.excl * e.v AS ev, round(e.excl * e.v * 0.01, dp) AS line_vat,
           e.pp * e.q AS pq, round(e.pp * e.q, dp) AS cost_line,
           min(e.n) OVER (PARTITION BY e.v) AS rate_first,
           first_value(e.v) OVER (PARTITION BY e.v ORDER BY e.n) AS rate,
           sum(e.excl) OVER (PARTITION BY e.v) AS rate_base,
           sum(e.excl) OVER (PARTITION BY e.v ORDER BY e.n) AS rate_run,
           sum(e.gross) OVER w AS gross_run, sum(e.disc) OVER w AS disc_run,
           sum(e.excl) OVER w AS excl_run,
           sum(CASE WHEN e.sc THEN 0 ELSE round(e.pp * e.q, dp) END) OVER w AS cost_run,
           sum(CASE WHEN e.sc THEN 0 ELSE e.excl END) OVER w AS revenue_run
    FROM e WINDOW w AS (ORDER BY e.n)
), r AS (
    SELECT l.rate_first, l.rate_base * l.rate AS br,
           round(l.rate_base * l.rate * 0.01, dp) AS rate_vat
    FROM l WHERE l.n = l.rate_first
), rt AS (
    SELECT r.*, sum(r.rate_vat) OVER (ORDER BY r.rate_first) AS vat_run FROM r
), t AS (
    SELECT sum(l.gross) AS gross_total, sum(l.disc) AS discount_total,
           sum(l.excl) AS excl_total, (SELECT sum(rate_vat) FROM r) AS vat_total,
           max(l.n) AS last,
           min(l.n) FILTER (WHERE greatest(abs(l.qp), abs(l.gd), abs(l.ev), abs(l.gross_run),
                                           abs(l.disc_run), abs(l.excl_run), abs(l.rate_run))
                                  > k.big) AS bad_line,
           coalesce(sum(l.cost_line) FILTER (WHERE NOT l.sc), 0) AS cost,
           coalesce(sum(l.excl) FILTER (WHERE NOT l.sc), 0) AS revenue,
           coalesce(max(l.n) FILTER (WHERE NOT l.sc), 0) AS last_counted,
           min(l.n) FILTER (WHERE NOT l.sc AND greatest(abs(l.pq), abs(l.cost_run),
                                                         abs(l.revenue_run)) > k.big) AS bad_cost
    FROM l, k GROUP BY k.big
), s AS (
    SELECT t.*, t.revenue - t.cost AS amount,
           CASE WHEN t.revenue = 0 THEN 1 ELSE t.revenue END AS divisor,
           coalesce(t.bad_line,
                    CASE WHEN EXISTS (SELECT 1 FROM rt WHERE abs(rt.br) > k.big
                                                          OR abs(rt.vat_run) > k.big)
                              OR abs(t.excl_total + t.vat_total) > k.big
                         THEN t.last END) AS overflow_at
    FROM t, k
), m AS (
    SELECT s.*,
           CASE WHEN s.overflow_at IS NULL THEN
               coalesce(s.bad_cost,
                        CASE WHEN abs(s.amount) > k.big
                                  OR abs(s.amount) > k.big * abs(s.divisor)
                                  OR abs(s.amount) * 100 > k.big * abs(s.divisor)
                             THEN s.last_counted END)
           END AS margin_overflow_at
    FROM s, k
)
SELECT l.n,
       CASE WHEN m.overflow_at IS NULL THEN l.gross END,
       CASE WHEN m.overflow_at IS NULL THEN l.disc END,
       CASE WHEN m.overflow_at IS NULL THEN l.excl END,
       CASE WHEN m.overflow_at IS NULL THEN l.line_vat END,
       l.rate, l.rate_first,
       CASE WHEN m.overflow_at IS NULL THEN l.rate_base END,
       CASE WHEN m.overflow_at IS NULL THEN r.rate_vat END,
       CASE WHEN m.overflow_at IS NULL THEN m.gross_total END,
       CASE WHEN m.overflow_at IS NULL THEN m.discount_total END,
       CASE WHEN m.overflow_at IS NULL THEN m.excl_total END,
       CASE WHEN m.overflow_at IS NULL THEN m.vat_total END,
       CASE WHEN m.overflow_at IS NULL THEN m.excl_total + m.vat_total END,
       CASE WHEN ok.margin THEN m.cost END,
       CASE WHEN ok.margin THEN m.revenue END,
       CASE WHEN ok.margin THEN m.amount END,
       -- Scale 40 before dividing: the quotient keeps ≥ 40 decimals, so float8 rounds once.
       CASE WHEN ok.margin THEN (round(m.amount * 100, 40) / m.divisor)::float8 END,
       m.overflow_at::int, m.margin_overflow_at::int
FROM l
JOIN r ON r.rate_first = l.rate_first
CROSS JOIN m
CROSS JOIN LATERAL (SELECT m.overflow_at IS NULL AND m.margin_overflow_at IS NULL AS margin) ok
$$;

DROP FUNCTION price_margin(int, numeric[], numeric[], boolean[], numeric[]);
