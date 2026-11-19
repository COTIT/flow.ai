-- One verified sales tax path in active books (Flow ERP M6-A, masterplan §13 TAX): each sale
-- line takes tax from the one verified sale profile covering it, per line and per component
-- (`tax_line`), snapshotted per component; credits reverse each component.

-- Per input line `n` (0-based) its components. Covering = verified, not superseded, 'sale',
-- effective on `on_date`, category in scope (empty scope = all). No covering profile or more
-- than one raises 22023 `tax_profile_missing:<n>` / `tax_profile_ambiguous:<n>`.
CREATE FUNCTION sale_tax_lines(ws uuid, on_date date, relation uuid, products uuid[],
                               amounts bigint[])
    RETURNS TABLE (n integer, profile_id uuid, line_no smallint, code text, rate numeric,
                   base bigint, tax bigint, exempt boolean)
    LANGUAGE plpgsql STABLE AS $$
#variable_conflict use_column
DECLARE
    i    integer;
    cat  uuid;
    hits uuid[];
BEGIN
    FOR i IN 1 .. coalesce(cardinality(amounts), 0) LOOP
        cat := (SELECT p.category_id FROM products p
                WHERE p.workspace_id = ws AND p.id = products[i]);
        hits := ARRAY(SELECT t.id FROM tax_profiles t
                      WHERE t.workspace_id = ws AND t.state = 'verified'
                        AND t.superseded_by_id IS NULL AND 'sale' = ANY (t.transaction_types)
                        AND on_date >= t.effective_from
                        AND (t.effective_to IS NULL OR on_date < t.effective_to)
                        AND (cardinality(t.product_category_ids) = 0
                             OR cat = ANY (t.product_category_ids)));
        IF cardinality(hits) = 0 THEN
            RAISE EXCEPTION 'tax_profile_missing:%', i - 1 USING ERRCODE = '22023';
        ELSIF cardinality(hits) > 1 THEN
            RAISE EXCEPTION 'tax_profile_ambiguous:%', i - 1 USING ERRCODE = '22023';
        END IF;
        RETURN QUERY
        SELECT i - 1, hits[1], x.line_no, x.code, x.rate, x.base, x.tax, x.exempt
        FROM tax_line(ws, hits[1], on_date, 'sale', amounts[i], false, cat, relation) x;
    END LOOP;
END $$;

-- Tax components of an order line as committed (active books); replaced with the line.
CREATE TABLE order_line_taxes (
    workspace_id  uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    order_line_id uuid          NOT NULL REFERENCES order_lines (id) ON DELETE CASCADE,
    line_no       smallint      NOT NULL,
    profile_id    uuid          NOT NULL,
    code          text          NOT NULL,
    rate          numeric(11,8) NOT NULL,
    base          bigint        NOT NULL,
    tax           bigint        NOT NULL,
    exempt        boolean       NOT NULL,
    tax_date      date          NOT NULL,
    PRIMARY KEY (order_line_id, line_no),
    CONSTRAINT order_line_taxes_profile_fk FOREIGN KEY (workspace_id, profile_id)
        REFERENCES tax_profiles (workspace_id, id)
);

-- Tax components of a standalone invoice item (active books); never re-priced once issued.
CREATE TABLE item_taxes (
    workspace_id uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    item_id      uuid          NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    line_no      smallint      NOT NULL,
    profile_id   uuid          NOT NULL,
    code         text          NOT NULL,
    rate         numeric(11,8) NOT NULL,
    base         bigint        NOT NULL,
    tax          bigint        NOT NULL,
    exempt       boolean       NOT NULL,
    tax_date     date          NOT NULL,
    PRIMARY KEY (item_id, line_no),
    CONSTRAINT item_taxes_profile_fk FOREIGN KEY (workspace_id, profile_id)
        REFERENCES tax_profiles (workspace_id, id)
);

-- Components a credit reverses, cumulative share of the order line's components.
CREATE TABLE sales_return_line_taxes (
    workspace_id   uuid     NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    return_line_id uuid     NOT NULL REFERENCES sales_return_lines (id) ON DELETE CASCADE,
    line_no        smallint NOT NULL,
    code           text     NOT NULL,
    tax            bigint   NOT NULL CHECK (tax >= 0),
    PRIMARY KEY (return_line_id, line_no)
);

-- Per line of return `ret` and tax component of its order line: the component's cumulative
-- share by quantity returned on that order line (posted returns, then earlier lines of this
-- return), less the share before; the return reaching the line quantity takes the exact rest.
-- Call before `ret` turns posted.
CREATE FUNCTION component_shares(ws uuid, ret uuid)
    RETURNS TABLE (return_line_id uuid, line_no smallint, code text, tax bigint)
    LANGUAGE sql STABLE AS $$
    WITH x AS (
        SELECT x.id, x.quantity AS q, ol.id AS ol, ol.quantity AS whole,
               coalesce((SELECT sum(y.quantity) FROM sales_return_lines y
                         JOIN sales_returns r ON r.id = y.return_id
                         JOIN sales_recognition_lines k ON k.id = y.recognition_line_id
                         WHERE y.workspace_id = ws AND k.order_line_id = ol.id
                           AND (r.state = 'posted' OR (y.return_id = ret AND y.id < x.id))),
                        0) AS before
        FROM sales_return_lines x
        JOIN sales_recognition_lines l ON l.workspace_id = x.workspace_id
                                       AND l.id = x.recognition_line_id
        JOIN order_lines ol ON ol.workspace_id = x.workspace_id AND ol.id = l.order_line_id
        WHERE x.workspace_id = ws AND x.return_id = ret)
    SELECT x.id, t.line_no, t.code,
           (money_round_div(t.tax * (x.before + x.q), x.whole)
            - money_round_div(t.tax * x.before, x.whole))::bigint
    FROM x JOIN order_line_taxes t ON t.workspace_id = ws AND t.order_line_id = x.ol
$$;
