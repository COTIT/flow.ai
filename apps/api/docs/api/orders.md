# Orders

Office orders and web shop checkouts. Code: `src/modules/orders/`, stock holds in `src/modules/stock/reserve.rs`.

Money is integer minor units of the order's currency (`decimals` on the order). Quantities are decimal text with 2 places, the stock scale.

## Statuses

| Status | Stock | Reached by |
|---|---|---|
| `draft` | nothing held | `POST /orders` |
| `awaiting_payment` | held until `expires_at` | web checkout (phase 2a step 5) |
| `open` | held | `POST …/confirm` from draft; payment confirmed |
| `completed` | taken off the shelf (ledger `decrease`, related `order`) | `POST …/complete` |
| `cancelled` | hold given back | `POST …/cancel` from draft, awaiting_payment or open |
| `partially_returned`, `returned` | returned quantity put back (unless `restock: false`) | `POST …/returns` on a completed order |

Every other move is a 409. A paid order can't be cancelled until it is refunded: 409 `{"message": "refund_required"}`.

Holds come from the warehouse-level stock row of the order's warehouse (default: the main warehouse) and are all or nothing: a short line is a 422 on `lines.N.quantity` (`Not enough stock: 2 available.`) and holds nothing. Only lines of products with `has_stock` hold stock (`tracked`).

## Routes (`/v1/workspace/{workspace}`)

| Method | Path | Permission | Success |
|---|---|---|---|
| GET | `/orders?status=&page=&page_size=` | `order.list` | 200 page of orders with lines, newest first |
| POST | `/orders` | `order.create` | 201 `{data:{order_id, number}}` |
| GET | `/orders/{order}` | `order.view` | 200 `{data: order}` |
| PATCH | `/orders/{order}` | `order.update` | 200 `{data: order}`; `lines`/`warehouse_id` on drafts only (else 409) |
| DELETE | `/orders/{order}` | `order.delete` | 200 `{message:"ok"}`; drafts only |
| POST | `/orders/{order}/confirm` | `order.update` | 200; draft → open |
| POST | `/orders/{order}/pick` | `order.update` | 200; `{lines:[{line_id, quantity}]}` sets picked totals (≤ line quantity) |
| POST | `/orders/{order}/complete` | `order.update` | 200; every stock line picked in full, else 409 |
| POST | `/orders/{order}/cancel` | `order.update` | 200 |
| POST | `/orders/{order}/returns` | `order.update` | 200; `{lines:[{line_id, quantity}], restock}`: quantity returned now, more than 0 and at most taken − returned; `restock` (default true) puts it back on the order's warehouse |
| POST | `/orders/{order}/invoice` | `order.update` | 200; one draft invoice of every line (open, completed or returned orders; 409 when already invoiced; 422 `relation_id` until the order has a relation). The invoice's lines move no stock: the order did. |

Every write takes an optional `Idempotency-Key` (1–255 visible characters): a retry of the same request replays the first answer, another request under the key is a 422, another member's key is never theirs.

## Create body

```json
{ "relation_id": null, "currency_id": null, "warehouse_id": null, "reference": "PO-7", "note": null,
  "lines": [{ "product_id": "…", "product_variant_id": null, "quantity": "2",
              "unit_price": 1250, "discount_percentage": "10", "title": "Honey" }] }
```

- `lines`: 1 to 50. `quantity` 0.01 to 100000, at most 2 decimals.
- Prices default to the catalog: the variant's price when one is sent, else the product's, in the order's currency (a product priced in another currency is a 422). VAT is the product's VAT group (0 without one). `unit_price` (minor units, excl VAT), `discount_percentage` and `title` override.
- Totals come from the pricing engine: line amounts rounded to the currency, VAT once per rate. `summary.vats` lists `{rate, base, vat}` per rate.
- Currency defaults to the main currency; warehouse to the main warehouse (422 `warehouse_id` when none is set).
- Numbers come from the `order` numbering settings (`ORD-0001` by default).

## Events

`order.created` on create (see `webhooks.md`): ids, number, status, source and totals only.
