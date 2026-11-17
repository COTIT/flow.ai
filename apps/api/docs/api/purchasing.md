# Purchasing

Flow ERP M3 (masterplan §11, §7, §10): purchase orders, partial receipts, booking received goods into valued stock, landed costs, and closing what will never arrive. Base `/v1/workspace/{workspace}`, tag `purchasing`, migration `purchasing`.

A receipt accepts goods: nothing is in stock or the books yet. Booking adds the stock and, in an active book, its value: DR inventory / CR GRNI once per receipt line. Bills, supplier payments, returns, corrections and late-receipt review come with M5.

## Money in the database

Every amount is computed in SQL with `money_round_div` (exact, half away from zero): line extensions, order totals, cumulative receipt values, conversion (`money_convert(minor, rate, from_dp, to_dp, inverse)`), landed-cost splits (`money_allocate(total, ids, bases)`: last id takes the rest), line values, booking totals, pool previews and the supplier open value. The API passes the results through.

## Envelopes and replay

Every write takes the accounting envelopes (`docs/api/accounting.md`) and a required `Idempotency-Key` header:

- C (create): `{client_ref: uuid, payload}` → 201.
- A (action): `{client_ref: uuid, expected_version: integer ≥ 1, payload}` → 200.

The same `client_ref` and body replays the committed answer for good; another body under it is 409. A version that moved is 409 `stale_document`; so is a review fingerprint that no longer matches.

## Numbers

- Quantities: decimal strings, at most 2 places, base units unless stated.
- Prices and rates: at most 8 places.
- Amounts: exact decimal strings in major units of their currency (`"12.50"`). Order amounts are in the order currency; booked values are in the book's functional currency.

## Permissions

| Operation | Route | Permission |
|---|---|---|
| `purchase_orders_list` | `GET /purchase-orders?state=&supplier_id=&page=&page_size=` | `purchaseorder.list` |
| `purchase_orders_create` | `POST /purchase-orders` | `purchaseorder.create` |
| `purchase_orders_get` | `GET /purchase-orders/{order}` | `purchaseorder.get` |
| `purchase_orders_update` | `PATCH /purchase-orders/{order}` | `purchaseorder.update` |
| `purchase_orders_confirm_review` | `POST /purchase-orders/{order}/confirm/review` | `purchaseorder.confirm` |
| `purchase_orders_confirm` | `POST /purchase-orders/{order}/confirm` | `purchaseorder.confirm` |
| `purchase_orders_close_lines` | `POST /purchase-orders/{order}/close-lines` | `purchaseorder.close` |
| `purchase_orders_cancel` | `POST /purchase-orders/{order}/cancel` | `purchaseorder.cancel` |
| `purchase_orders_events` | `GET /purchase-orders/{order}/events` | `purchaseorder.get` |
| `purchase_receipts_list` | `GET /purchase-orders/{order}/receipts?page=&page_size=` | `purchaseorder.get` |
| `purchase_receipts_create` | `POST /purchase-orders/{order}/receipts` | `purchaseorder.receive` |
| `purchase_receipts_get` | `GET /purchase-orders/{order}/receipts/{receipt}` | `purchaseorder.get` |
| `purchase_receipts_book_review` | `POST /purchase-orders/{order}/receipts/{receipt}/book/review` | `purchaseorder.book` |
| `purchase_receipts_book` | `POST /purchase-orders/{order}/receipts/{receipt}/book` | `purchaseorder.book` |

`journal_entry_id` on receipt lines and the `pool` preview in a book review need `accounting.read` as well; without it they are null.

## Orders

### Create / update payload

```json
{ "supplier_id": "uuid", "currency_id": "uuid", "purchase_date": "2026-11-15",
  "expected_date": "2026-11-20", "reference": "PO-ref", "note": "rush",
  "default_warehouse_id": "uuid",
  "lines": [{ "product_id": "uuid", "quantity": "2", "purchase_unit_id": "uuid",
              "unit_price": "30.00", "warehouse_id": "uuid" }] }
```

- 1 to 100 lines. `reference` ≤ 255, `note` ≤ 1000 characters. `expected_date` not before `purchase_date`.
- Products: live, stock-tracked (`has_stock`, type `physical` or `physical_second_chance`).
- `purchase_unit_id`: an active purchase unit of that product. `quantity` is then in that unit; the line freezes its ratio. `base_quantity = quantity × ratio` must fit 2 places, else 422.
- `extension = quantity × unit_price`, rounded half away from zero to the currency's decimals. It is the line's authoritative amount.
- `PATCH` replaces every field and line; drafts only (422 otherwise). Orders get their number at create.

### Confirm

`POST .../confirm/review` returns `{purchase_order_id, version, supplier_snapshot, currency, lines[{line_id, product_id, purchase_unit_id, ratio, base_quantity, unit_price, extension}], totals, review_fingerprint}`. `POST .../confirm` with `{review_fingerprint}` freezes the supplier (name, alias, addresses, supplier data without the FTP password) and the lines. No stock, no journal.

### Close and cancel

- `close-lines` `{reason, lines:[{line_id, quantity}]}`: confirmed orders; each quantity ≤ the line's open quantity. Closures are kept as history. Received goods stay bookable.
- `cancel` `{reason}`: drafts or confirmed orders nothing was received on. A confirmed order closes every open remainder. Once goods were received, close the remainder instead (422).

### Resource

`{id, workspace_id, number, version, state (draft|confirmed|cancelled), receipt_status (not_received|partial|received), supplier{id,name}, supplier_snapshot, currency{id,code,decimals}, purchase_date, expected_date, reference, note, default_warehouse_id, totals{ordered,received,open}, lines[…], created_by, created_at, updated_at, confirmed_by, confirmed_at, cancelled_at, cancel_reason, allowed_actions}`.

Line: `{id, line_no, product{id,name,unit}, purchase_unit{id,code,ratio}|null, input_quantity, ratio, base_quantity, unit_price, extension, warehouse_id, received_quantity, booked_quantity, closed_quantity, open_quantity}`. Always `booked ≤ received` and `received + closed ≤ base_quantity` (database CHECKs); `open = base − received − closed`.

`allowed_actions`: draft `update, confirm, cancel`; confirmed `receive, close` while something is open, `cancel` while nothing was received.

### Events

`GET .../events`: `[{at, kind, actor, ref_id, detail}]`, newest first, at most 500. Kinds: `created`, `confirmed`, `received` (ref = receipt), `booked` (ref = receipt), `closed` (ref = order line, detail = quantity and reason), `cancelled`. Built from the order's rows; no separate log.

## Receipts

### Receive

```json
{ "effective_receipt_date": "2026-11-15", "note": "pallet 1",
  "lines": [{ "po_line_id": "uuid", "base_quantity": "2", "warehouse_id": "uuid",
              "bin_id": "uuid", "lot_id": "uuid" }] }
```

- Confirmed orders only. Each order line at most once per receipt; quantity ≤ open (422 `payload.lines.N.base_quantity`). Concurrent receipts lock the order lines, so they never overreceive together.
- A line of another order: 404. The bin must be in the warehouse, the lot a lot of the product. The date may not be after today in the book's timezone.
- In an active book each product/warehouse pool is locked and the line becomes **pending**. If the pool was already valued on a later date, the line is **late**: kept as evidence, no guard, booked through the late-receipt review (M5).

### Book

Review first: `POST .../book/review` with `{receipt_line_ids, landed_costs}`, then `POST .../book` with the same plus `review_fingerprint` and the receipt's `expected_version`.

```json
{ "receipt_line_ids": ["uuid"],
  "landed_costs": [{ "client_component_ref": "uuid", "amount": "7.00", "currency_id": "uuid",
                     "evidence_ref": "DHL-1",
                     "explicit_line_allocations": [{ "receipt_line_id": "uuid", "amount": "7.00" }] }] }
```

- Each selected line books its full quantity once: stock goes to its warehouse/bin/lot slot. A line already booked: 409 `stale_document`. A late line: 422 `backdated_valuation`.
- Active book, per line: value = the order line's extension share (cumulative rounding, the last unit takes the exact rest) converted at the rate in force on the effective receipt date (direct, else inverse, never 1 by default; 422 `payload.currency_id` without one), plus its landed share. Journal DR inventory / CR GRNI dated the receipt date; effect identity `purchase_receipt_line/booked`.
- Landed cost: converted on the same date and spread over the selected lines by net value in receipt-line id order, the last line taking the exact rest. Lines without net value need `explicit_line_allocations` (component currency, adding up to `amount`). A `client_component_ref` capitalises once per order: reuse on any receipt is 422 `payload.landed_costs.N.client_component_ref`. The credit side is GRNI; the component keeps its own rows so a carrier's bill can match it later.
- Setup book: stock only. No value, journal, rate or guard; `landed_costs` must be empty (422 `unsupported_active_erp_operation`).

Review answer: `{receipt_id, receipt_version, purchase_order_version, accounting_date, functional_currency, lines[{receipt_line_id, base_quantity, net_value, landed_value, value, rate}], landed_costs[{client_component_ref, amount, currency, functional_amount, rate, allocations}], total, journal{debit_role, credit_role}, pool[{product_id, warehouse_id, quantity_before, value_before, quantity_after, value_after}], review_fingerprint}`. Null values in a setup book.

### Resource

`{id, purchase_order_id, number, version, effective_receipt_date, note, recorded_at, created_by, functional_currency{id,code,decimals}|null (null in a setup book; labels every functional amount), booking_status (unbooked|partial|booked), lines[{id, po_line_id, product_id, warehouse_id, bin_id, lot_id, base_quantity, state (pending|late|booked), booked_at, accounting_date, net_value, landed_value, value, rate{rate,inverse,source,date}, journal_entry_id}], landed_costs[{id, client_component_ref, amount, currency, functional_amount, rate, evidence_ref, allocations[{receipt_line_id, functional_amount}]}], allowed_actions}`.

## Pending receipts block the pool

While an active book holds a pending line for a product in a warehouse:

- Every other stock change of that pool is refused with 422 `receipt_booking_required` and nothing written: sales (orders, invoices, till), transfers out **and in**, packing, manual changes, lot stock changes. Bin moves and reservations still work; a reservation can't be fulfilled until booking.
- Sale availability counts 0 for that warehouse's stock: web shop catalog and product page, till catalog and quote, `available_lte`, attention out-of-stock and below-minimum.
- Booking goes in eligible order per pool: effective date, then record time, then line id. Booking a later line first: 422 `receipt_booking_required`.
- The SQL function `purchase_receipt_pending(workspace, warehouse, product|null)` answers it for every reader; stocktake start (M6) and close checks (M5) call it with product null.

## Supplier list

`GET /relations?suppliers=1` rows carry `open_orders` (confirmed orders with anything open) and `open_order_value` (open extension share, Laravel number format). The value counts only orders in the workspace default currency; amounts in other currencies don't add up without a rate, so those orders count in `open_orders` only.

## Errors

| Status | When |
|---|---|
| 401 | No or bad token |
| 403 | Missing permission |
| 404 | Other workspace, unknown order/receipt/line, or a line of another order or receipt |
| 409 | `stale_document` (version, review fingerprint, line already booked); reused `client_ref` with another body |
| 422 | Field errors; `receipt_booking_required`; `backdated_valuation`; `accounting_not_configured` (functional currency or `inventory`/`grni` mapping missing); `accounting_period_closed`; `unsupported_active_erp_operation` (stock from before activation, landed cost in a setup book) |
