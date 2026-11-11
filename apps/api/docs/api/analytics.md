# Sales analytics and invoice export

Paths under `/v1/workspace/{workspace}`. Bearer token; a non-member gets 404, a member without the permission 403. Money is integer minor units (`decimals` per currency), never summed across currencies. Every row belongs to this workspace.

## Periods

- `from` is inclusive and `to` exclusive. Each takes an RFC 3339 time or a `YYYY-MM-DD` date. A date means local midnight in the workspace `general.timezone`, or UTC when that is unset or is a name Postgres doesn't know. "October" is `from=2026-10-01&to=2026-11-01`.
- `to` must be after `from`, at most 366 days later. A missing or malformed bound, or a bad span, is a 422 on that field.
- Answers echo the resolved instants and `time_zone`, the zone the dates were read in.
- A sale follows the same rule as `GET /sales-summary` (`orders.md`): an order now `open`, `completed`, `partially_returned` or `returned`, counted at `confirmed_at` (else `paid_at`, `completed_at`, `created_at`). `source` (`office`, `storefront`, `till`, blank = all) narrows it.

## Top customers `GET /sales/top-customers?from&to&source&limit&product_id&variant_id` (`order.list` + `relation.list`)

```json
{ "data": { "from": "…", "to": "…", "time_zone": "UTC", "source": null,
  "currencies": [{ "currency": "EUR", "decimals": 2,
    "rows": [{ "relation": { "id": "…", "name": "Beta Shop" }, "orders": 1, "gross_total": 2000, "net_total": 1800, "vat_total": 200 }],
    "without_customer": { "orders": 1, "gross_total": 300, "net_total": 270, "vat_total": 30 } }] } }
```

- `rows`: the top `limit` customers per currency (default 5, 1 to 20; else a 422), by the totals incl VAT of their orders, then name, then id.
- `without_customer`: orders with no relation (till walk-ins), never ranked. Zeros when there are none.
- `relation.list` is required as well because the answer names customers.
- `product_id` ("who buys the most honey") ranks by that product's lines only: totals are `line_excl + line_vat`, `net_total` = `line_excl`, `vat_total` = `line_vat`, `orders` the orders holding it; other products on the same orders are left out. `without_customer` follows the same filter. `variant_id` keeps one size (alone or with `product_id`). Not a UUID: 422 on that field. An id from another account finds nothing (`currencies: []`).
- Every total comes three ways: `gross_total` (incl VAT), `net_total` (the orders' `subtotal`, excl VAT) and `vat_total` (the orders' stored `vat`; the pricing engine writes total = subtotal + VAT), so gross = net + VAT exactly. The same holds in best sellers and compare (compare reads VAT as gross − net). All before refunds.
- Line VAT is rounded per line; order VAT once per rate. With a product filter, `vat_total` can differ from that product's share of order VAT by a cent per line.

## Best sellers `GET /sales/top-products?from&to&source&limit&by&product_id&variant_id` (`order.list`)

```json
{ "data": { "from": "…", "to": "…", "time_zone": "UTC", "source": null, "by": "product",
  "currencies": [{ "currency": "EUR", "decimals": 2, "rows": [
    { "product": { "id": "…", "name": "Honey" }, "variant": null, "title": "Honey",
      "units": "5.00", "lines": 2, "orders": 2, "gross_total": 2500,
      "net_total": 2250, "vat_total": 250 }] }] } }
```

- Ranked per currency by line totals incl VAT (`line_excl + line_vat`), then product id. `units` is the summed quantity (2 decimals), `lines` the order lines, `orders` the distinct orders.
- `by=variant` ranks each variant apart: `variant` is `{id, sku}`, or null for lines without one. `title` is the line title as sold, so it names the size. Any other `by` is a 422.
- `product_id` keeps one product's lines (all its variants), so "how much honey did we sell in October" is one row. `variant_id` keeps one size's lines. Not a UUID: 422 on `product_id`. An id from another account finds nothing (`currencies: []`). `net_total` = `line_excl`, `vat_total` = `line_vat`.

## Compare periods `GET /sales/compare?from&to&compare_from&compare_to&source` (`order.list`)

```json
{ "data": { "period": { "from": "…", "to": "…" }, "compare_to_period": { "from": "…", "to": "…" },
  "time_zone": "UTC", "source": null,
  "currencies": [{ "currency": "EUR", "decimals": 2,
    "current": { "orders": 4, "gross_total": 3800, "net_total": 3420, "vat_total": 380, "refunded_total": 0 },
    "base": { "orders": 1, "gross_total": 1000, "net_total": 900, "vat_total": 100, "refunded_total": 0 },
    "gross_difference": 2800, "orders_difference": 3, "gross_change_bp": 28000,
    "net_difference": 2520, "net_change_bp": 28000 }] } }
```

- One row per currency sold in either period; a period with no sales in that currency shows zeros.
- `gross_difference` and `orders_difference` are current minus base. `gross_change_bp` is the signed change of `gross_total` in basis points (10000 = +100 %), rounded half away from zero. It is left out when the base gross is 0. `net_difference` and `net_change_bp` do the same on net totals; `net_change_bp` is left out when the base net is 0.
- Both are computed in SQL. Each period follows the period rules (422 on `to` or `compare_to`). The periods may overlap, and an order in both counts in both.

## Daily sales `GET /sales/daily?from&to&source` (`order.list`, `orders_sales_daily`)

`from` and `to` are `YYYY-MM-DD` local days in `general.timezone` (UTC fallback), `to` exclusive, at most 92 days apart (422 on `to`). Same sold rule as the sales summary. One statement (`generate_series` per currency sold in range):

```json
{ "data": { "time_zone": "Africa/Dar_es_Salaam", "from": "2026-10-01", "to": "2026-10-04",
  "currencies": [{ "currency": "EUR", "decimals": 2, "days": [
    { "date": "2026-10-01", "gross": 763, "net": 700, "orders": 2 }, … ] }] } }
```

Every day appears for every currency (zero-filled); currencies never mix; nothing sold gives `currencies: []`. Money in minor units, gross = net + VAT.

## Invoice export `GET /invoices-export?from&to` (`invoice.list`)

```json
{ "data": { "file_id": "…", "name": "invoices-2026-10-01-to-2026-10-31.csv",
  "url": "https://…", "expires_at": "…", "invoices": 3, "lines": 3,
  "currencies": [{ "currency": "EUR", "decimals": 2, "excl": 2000, "vat": 420, "total": 2420 }] } }
```

- Selects numbered invoices, so drafts are out, dated (`send_date`) in the period. Credit notes (negative amounts) and cancelled invoices are included, so the number sequence has no gaps; the `status` column says which is which.
- CSV columns: `invoice_number, invoice_date, due_date, type (invoice|credit_note), status, customer, customer_vat_number, currency, line, quantity, unit_price_excl, vat_rate, discount_pct, line_excl, line_vat, line_total, invoice_excl, invoice_vat, invoice_total`. There is one row per line, ordered by number and line position. Dates are local dates in the workspace zone.
- Money cells are exact decimal text from minor units. Line amounts are priced like the invoice line money (`invoices.md`); invoice totals are the stored ones.
- UTF-8 with a BOM and CRLF line ends. Text cells that start with `= + - @`, a tab or a CR get a leading `'`, so a spreadsheet never runs them as formulas. Cells with a comma, quote or line break are quoted.
- The CSV is stored as a workspace file (`description` "Invoice export …", `meta.kind` `invoice_export`) and expires after 24 hours. The file sweep then removes the object and the row (`files.expires_at`, server-set only). `url` downloads it until `expires_at` (10 minutes); `GET /files/{id}` gives a fresh link while the file lives.
- `currencies`: the exported invoices' totals per currency, each invoice counted once.
- No invoices in range: 200 with `invoices: 0` and `file_id`, `url` and `expires_at` null; nothing is stored.
- At most 10000 lines per export, else a 422 on `to` asking for a shorter range. A file store is required (503 `File storage isn't configured.` without one).
- The file holds customer VAT numbers: only `invoice.list` holders get it, and its contents are never logged.
