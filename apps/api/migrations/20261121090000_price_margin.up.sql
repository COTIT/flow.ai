-- Margin of item lines priced elsewhere (pricing::engine::margin): lines that are not set
-- children count; cost = Σ round(purchase × q) at `dp`, revenue = Σ `revenues` (each line's
-- excl), amount = revenue − cost, % = amount × 100 / revenue (/ 1 when revenue is 0), as
-- float8 rounded once from the exact quotient. Pairs lines with revenues up to the shorter
-- array. margin_overflow_at: first counted line (0-based) whose cost or running sum leaves
-- rust_decimal's range, the last counted line when the amount or % does; values NULL then.
-- One row, also for no lines.
CREATE FUNCTION price_margin(dp int, qs numeric[], pps numeric[], scs boolean[],
                             revenues numeric[])
    RETURNS TABLE (cost numeric, revenue numeric, margin_amount numeric,
                   margin_percentage float8, margin_overflow_at int)
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
AS $$
WITH l AS (
    SELECT (u.o - 1)::int AS n, u.q * coalesce(u.pp, 0) AS pq,
           round(u.q * coalesce(u.pp, 0), dp) AS cost_line, u.r AS excl
    FROM unnest(qs, pps, scs, revenues) WITH ORDINALITY u(q, pp, sc, r, o)
    WHERE u.o <= least(cardinality(qs), cardinality(revenues)) AND NOT coalesce(u.sc, false)
), w AS (
    SELECT l.*, sum(l.cost_line) OVER (ORDER BY l.n) AS cost_run,
           sum(l.excl) OVER (ORDER BY l.n) AS revenue_run
    FROM l
), t AS (
    SELECT coalesce(sum(w.cost_line), 0) AS cost, coalesce(sum(w.excl), 0) AS revenue,
           coalesce(max(w.n), 0) AS last_counted,
           min(w.n) FILTER (WHERE greatest(abs(w.pq), abs(w.cost_run), abs(w.revenue_run))
                                  > 79228162514264337593543950335) AS bad
    FROM w
), s AS (
    SELECT t.*, t.revenue - t.cost AS amount,
           CASE WHEN t.revenue = 0 THEN 1 ELSE t.revenue END AS divisor,
           79228162514264337593543950335::numeric AS big
    FROM t
), m AS (
    SELECT s.*, coalesce(s.bad,
                         CASE WHEN abs(s.amount) > s.big
                                   OR abs(s.amount) > s.big * abs(s.divisor)
                                   OR abs(s.amount) * 100 > s.big * abs(s.divisor)
                              THEN s.last_counted END) AS over
    FROM s
)
SELECT CASE WHEN m.over IS NULL THEN m.cost END,
       CASE WHEN m.over IS NULL THEN m.revenue END,
       CASE WHEN m.over IS NULL THEN m.amount END,
       -- Scale 40 before dividing: the quotient keeps ≥ 40 decimals, so float8 rounds once.
       CASE WHEN m.over IS NULL THEN (round(m.amount * 100, 40) / m.divisor)::float8 END,
       m.over::int
FROM m
$$;

-- price_lines (20261120090000) with its margin from price_margin; same columns and results.
CREATE OR REPLACE FUNCTION price_lines(dp int, qs numeric[], ps numeric[], vs numeric[],
                                       ds numeric[], pps numeric[], scs boolean[])
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
    SELECT (u.o - 1)::int AS n, u.q, u.p, u.v, u.d,
           u.q * u.p AS qp, round(u.q * u.p, dp) AS gross
    FROM unnest(qs, ps, vs, ds) WITH ORDINALITY u(q, p, v, d, o)
), x AS (
    SELECT i.*, i.gross * i.d AS gd, round(i.gross * i.d * 0.01, dp) AS disc FROM i
), e AS (
    SELECT x.*, x.gross - x.disc AS excl FROM x
), l AS (
    SELECT e.*, e.excl * e.v AS ev, round(e.excl * e.v * 0.01, dp) AS line_vat,
           min(e.n) OVER (PARTITION BY e.v) AS rate_first,
           first_value(e.v) OVER (PARTITION BY e.v ORDER BY e.n) AS rate,
           sum(e.excl) OVER (PARTITION BY e.v) AS rate_base,
           sum(e.excl) OVER (PARTITION BY e.v ORDER BY e.n) AS rate_run,
           sum(e.gross) OVER w AS gross_run, sum(e.disc) OVER w AS disc_run,
           sum(e.excl) OVER w AS excl_run
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
           max(l.n) AS last, array_agg(l.excl ORDER BY l.n) AS excls,
           min(l.n) FILTER (WHERE greatest(abs(l.qp), abs(l.gd), abs(l.ev), abs(l.gross_run),
                                           abs(l.disc_run), abs(l.excl_run), abs(l.rate_run))
                                  > k.big) AS bad_line
    FROM l, k GROUP BY k.big
), s AS (
    SELECT t.*,
           coalesce(t.bad_line,
                    CASE WHEN EXISTS (SELECT 1 FROM rt WHERE abs(rt.br) > k.big
                                                          OR abs(rt.vat_run) > k.big)
                              OR abs(t.excl_total + t.vat_total) > k.big
                         THEN t.last END) AS overflow_at
    FROM t, k
), m AS (
    SELECT s.*, g.* FROM s
    LEFT JOIN LATERAL price_margin(dp, qs, pps, scs, s.excls) g ON s.overflow_at IS NULL
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
       m.cost, m.revenue, m.margin_amount, m.margin_percentage,
       m.overflow_at::int, m.margin_overflow_at
FROM l
JOIN r ON r.rate_first = l.rate_first
CROSS JOIN m
$$;
