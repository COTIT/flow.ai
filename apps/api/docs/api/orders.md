# Orders

Office orders and web shop checkouts. Code: `src/modules/orders/`, stock holds in `src/modules/stock/reserve.rs`.

Money is integer minor units of the order's currency (`decimals` on the order). Quantities are decimal text with 2 places, the stock scale.

## Statuses

| Status | Stock | Reached by |
|---|---|---|
| `draft` | nothing held | `POST /orders` |
| `awaiting_payment` | held until `expires_at` (30 minutes) | `POST /storefront/orders` |
| `open` | held | `POST …/confirm` from draft; payment confirmed |
| `completed` | taken off the shelf (ledger `decrease`, related `order`) | `POST …/complete` |
| `cancelled` | hold given back | `POST …/cancel` from draft, awaiting_payment or open |
| `partially_returned`, `returned` | returned quantity put back (unless `restock: false`) | `POST …/returns` on a completed order |

Every other move is a 409. A paid order can't be cancelled until it is fully refunded: 409 `{"message": "refund_required"}`. An invoice made from a paid order books what was paid minus refunded as an `invoice_payments` row (method `provider`); a draft invoice keeps its status until it is sent.

Holds come from the warehouse-level stock row of the order's warehouse (default: the main warehouse) and are all or nothing: a short line is a 422 on `lines.N.quantity` (`Not enough stock: 2 available.`) and holds nothing. Only lines of products with `has_stock` hold stock (`tracked`).

### `allowed_actions`

Every order resource (get, list, each action's answer) lists the actions the API would accept now, in this order: `confirm`, `pick`, `complete`, `cancel`, `return`, `refund`, `invoice` (enum `OrderAction`). Each action route runs the same check under the order lock, so a listed action passes its state rules and an unlisted one is refused with the 409/422 that check gives. It covers status and payment rules only, not the caller's permissions, and not a request body's own validation (e.g. a pick quantity above the line).

| Action | Allowed when |
|---|---|
| `confirm` | `draft` |
| `pick` | `open` |
| `complete` | `open` and every stock line picked in full |
| `cancel` | `draft`, `awaiting_payment` or `open`, and nothing net paid (paid − refunded) |
| `return` | `completed` or `partially_returned` |
| `refund` | the first paid payment has an amount left after refunds not failed or canceled |
| `invoice` | `open`, `completed`, `partially_returned` or `returned`; no invoice yet; a relation set (else 422 on `relation_id`) |

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
| POST | `/orders/{order}/refunds` | `order.delete` | 201 `{data:{refund_id, amount, status}}`; `{amount}` in minor units, 1 to paid − refunds not failed (422 above); 409 when nothing paid is left to refund (`Nothing paid on this order is left to refund.`). See `payments.md`. |
| GET | `/orders/{order}/events?page=&page_size=` | `order.view` | 200 page of the order's trail, newest first (see "Trail") |
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

## Trail (`order_events`, migration `order_events`)

Every action writes one row in its own transaction: `{id, kind, at, actor, meta}`.

| `kind` | Written by | `actor` |
|---|---|---|
| `created` | office create, web checkout, till sale (`at` = the sale's `sold_at`) | `user`, `app_key` (the shop key, never the customer), `till_user` |
| `confirmed` | office confirm | `user` |
| `paid` | the provider confirmed a web payment | null (system) |
| `picked` | each office pick | `user` |
| `completed` | office complete; till sale (`at` = `sold_at`) | `user`, `till_user` |
| `cancelled` | office cancel | `user` |
| `expired` | the expiry job cancelled an unpaid web order | null |
| `returned` | office returns | `user` |
| `refunded` | office refund (`meta.amount`), or the refund the system owes for a late payment without stock (`meta: {amount, auto: true}`) | `user`, null |
| `invoiced` | office invoice | `user` |

- `actor`: `{type: "user" | "till_user" | "app_key", id, name}` or null. A user's `name` is null once they are no longer a member of this workspace (the id stays).
- No backfill: orders acted on before the migration have no rows for those actions, and their `confirmed_at`/`picked_at` stay null. Show what is there.
- Orders gain `confirmed_at` (the first time it became `open` or a till sale `completed`) and `picked_at` (the first time every stock line was picked in full).
- Another workspace's order is 404; `order.view` is needed (403).

## Web shop (`/v1/storefront`, app key with `checkout:write`)

| Method | Path | Success |
|---|---|---|
| POST | `/storefront/orders` | 201 `{data: order}`: `awaiting_payment`, stock held for 30 minutes. Optional Idempotency-Key, per app key: another key's use of it is a 422. |
| GET | `/storefront/orders/{order}` | 200 `{data: order}` |
| POST | `/storefront/orders/{order}/payment` | 200 `{data:{payment_id, status, checkout_url}}` |

The shop's order view holds `id, number, status, currency, decimals, subtotal, vat, total, expires_at, paid_at` only: never the customer. Office orders and other workspaces' orders are 404.

Checkout body: `{lines: [{product_id, product_variant_id?, quantity}], customer: {email, name, phone?, shipping_address?, billing_address?}}`.
- Prices come from the catalog only: a sent `unit_price`, `discount_percentage` or `title` is a 422. Products must be active and not archived.
- 1 to 50 lines, quantity at most 1000 per line; addresses are objects of at most 4 KiB.
- The main currency and the main warehouse; not enough stock is a 422 on `lines.N.quantity`.
- At most `CHECKOUT_HOLD_MAX` (default 500) web orders per workspace may await payment: 429 above it. The per-key private rate limit applies too.

Payment body: `{redirect_url}`: https, or http to localhost/127.0.0.1 only; at most 2048 characters.
- One live intent per order: repeat calls return it. A new intent is committed as `creating` first; its id is the provider's idempotency key, so a retry gets the same payment.
- 503 `no payment provider configured` without `PAYMENTS_PROVIDER`; 503 while the provider is down (retry); 409 when the order isn't awaiting payment or has expired.

The `order_expire` job (every minute) cancels web orders past `expires_at` and gives their stock back.
