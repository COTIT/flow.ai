# Stocktakes

Flow ERP M6 (masterplan D10, §10, §15, §19 Q). A stocktake counts one warehouse. Starting it pauses that warehouse until the count is posted or cancelled. Base `/v1/workspace/{workspace}`, tag `stocktakes`, migration `stocktakes`.

## Lifecycle

`draft` → `counting` → `posted` or `cancelled`. A draft can also be cancelled.

- **Start** takes the book gate, then the warehouse `FOR UPDATE`, so stock writes already running there finish first. Start is refused while goods received here wait for booking in an active book (422 `receipt_booking_required`) and while another count runs here (409 `warehouse_paused`). It then freezes every live stock row of the warehouse (each bin and lot) as a line with its level as `expected_quantity`, and commits. No transaction stays open while people count.
- **Counts** store `counted_quantity` per line. Lines not sent keep their count. Send them as often as needed while counting.
- **Post** needs every line counted (a missing count is not zero). Each changed line books `counted − expected` once through the stock layer: a movement related to `("stocktake", id)`, plus value and journal in an active book. In the same transaction the warehouse resumes. A count below what orders hold on that row is refused (422).
- **Cancel** keeps the lines and counts as history, moves no stock and resumes the warehouse.

After post and cancel, holds sitting on lots that picking skips (recalled, quarantined, expired) are reallocated for this warehouse with the usual rule: an order's hold moves whole to sellable rows, or stays whole. A hold that stays is listed on `GET /lots/{lot}/holds`, and the order can't complete until it is released or covered.

## While a warehouse is counted

| Refused (409 `warehouse_paused`) | Still allowed |
|---|---|
| Every level change here: stock mutate/set levels, transfers in or out, lot stock mutations, order complete (take), till sale, invoice send, packing, farm supply, receipt booking | Reads |
| New order holds here (confirm) | Releasing holds (order cancel, expiry) |
| New purchase receipts into this warehouse | Lot status changes (recall, quarantine): the hold on such a lot stays until the count ends; the order can't complete |
| Deleting the warehouse | Min/max settings; other warehouses |

The pause does not expire on its own.

## Value in an active book

- Fewer than expected: removed at the current average cost, DR `inventory_adjustment` / CR `inventory`.
- More than expected, and the product has stock with cost here: added at the current average cost. Sending `positive_adjustment_value` is refused (422 `stocktake_value_not_allowed`).
- More than expected and no cost here (empty pool): `positive_adjustment_value` is required (422 `stocktake_value_required`). It is the total value of the found stock in major units of the functional currency (`"0"` allowed when sent). DR `inventory` / CR `inventory_adjustment`.
- `accounting_date` dates the value entries and journals. A date before the pool's last valued date is refused (422 `backdated_valuation`).

In a setup book no value is booked. `positive_adjustment_value` is ignored and not stored.

## Review and post

`POST /stocktakes/{id}/post/review` with `{accounting_date}` runs the real post in a transaction and rolls it back. It returns each changed line's `delta` and `value` (null in setup books) and a `review_fingerprint`. Nothing is written: no journal number, command row or outbox event survives a review. Post sends the fingerprint back. If anything changed meanwhile, post answers 409 `stale_document`.

## Envelopes

Writes take the accounting envelopes (`docs/api/accounting.md`) and a required `Idempotency-Key` header: C `{client_ref, payload}` → 201, A `{client_ref, expected_version, payload}` → 200. The same `client_ref` and body replay the committed answer for good.

## Routes

| Operation | Route | Permission | Payload |
|---|---|---|---|
| `stocktakes_list` | `GET /stocktakes?page=&page_size=` | `stocktake.list` | headers only, newest first |
| `stocktakes_create` | `POST /stocktakes` | `stocktake.create` | C `{warehouse_id, reason}` |
| `stocktakes_get` | `GET /stocktakes/{id}` | `stocktake.get` | with `lines` |
| `stocktakes_start` | `POST /stocktakes/{id}/start` | `stocktake.start` | A `{}` |
| `stocktakes_count` | `PUT /stocktakes/{id}/counts` | `stocktake.count` | A `{lines: [{stock_identity, counted_quantity, positive_adjustment_value?}]}`, 1–1000 lines |
| `stocktakes_post_review` | `POST /stocktakes/{id}/post/review` | `stocktake.post` | `{accounting_date}` (no envelope) |
| `stocktakes_post` | `POST /stocktakes/{id}/post` | `stocktake.post` | A `{accounting_date, review_fingerprint}` |
| `stocktakes_cancel` | `POST /stocktakes/{id}/cancel` | `stocktake.cancel` | A `{reason}` |

Quantities are decimal strings with at most 2 places, ≥ 0. `stock_identity` is a line's stock row id. Another workspace's stocktake is 404.

## Line

```json
{ "stock_identity": "uuid", "product_id": "uuid", "bin_id": null, "lot_id": "uuid",
  "expected_quantity": "5", "counted_quantity": "4", "positive_adjustment_value": null }
```

## Limits (v1)

- A stock row created after start (stock found in a bin or lot that had no row) is not in the count. Cancel and start again to include it.
- A hold that can't be reallocated after the count stays whole on its lot (see above). No retry job or extra attention row is added.
