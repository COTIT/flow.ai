# Webhooks

Events sent to URLs a workspace registers. Code: `src/modules/webhooks/`.

## Events

The spec lists the names as enum `WebhookEvent`.

| Type | When | `data` |
|---|---|---|
| `stock.changed` | a stock level or what orders hold of it changes (mutations, set levels, bin moves, invoice and order stock moves, holds). Requirement-only changes send nothing. | `stock_id, product_id, warehouse_id, lot_id` (null for no-lot stock), `reason` (increase, decrease, set_level, reserved, released), `quantity_change, quantity_after` (decimal strings; holds send change `0.00`: re-read availability) |
| `product.changed` | a product or one of its variants is created, updated or deleted | `product_id, action` (created, updated, deleted; variant writes are `updated`) |
| `order.created` | an order is created (office draft or web checkout) | `order_id, number, status, source, currency, subtotal, vat, total` (minor units). Never customer details. |
| `lot.created` | a lot is created | `lot_id, product_id, code, public_code` |
| `lot.status_changed` | a lot's status changes, by PATCH or because a lot it was packed from went off sale (one event per lot) | `lot_id, product_id, code, public_code, from, to` |
| `supply.corrected` | an approved kg correction re-split a supply batch's credits; sent in the farm workspace only | `correction_id, batch_id, farmers` (count whose money moved), `net_kg` (decimal string). Each farmer's amount is in their passbook. |
| `supply.source_status_changed` | a lot this workspace received (or is receiving) over a supply link changed status at the sender; sent in the receiving workspace only | `transfer_id, to_lot_id` (null until received), `from_lot_code, status` |
| `passport.published` | a lot's passport is published (each new version) | `lot_id, product_id, public_code, version` |
| `sale.attributed` | farmers of a supply lot were credited for a sale (`kind: sale`) or debited for a return (`kind: return`); sent in the farm workspace only | `attribution_id, kind, sold_workspace_id, order_number, sold_lot_id, batch_id, jars` (decimal string), `price_per_kg, currency_id, total` (minor units), `farmers` (count). Each farmer's amount is in their passbook. |

Payloads are thin: re-read through `/storefront` for the details. Planned, not sent yet: `harvest.reported`.

## Delivery

1. **Outbox.** The change's own transaction writes a `webhook_events` row when an active endpoint of the workspace subscribes to that type, plus one `webhook_fanout` job per transaction. A rollback sends nothing.
2. **Fan-out.** The job writes one `webhook_deliveries` row per event and subscribed active endpoint (never twice) and queues a `webhook_deliver` job for each. A `webhook_sweep` scheduled every minute re-queues a fan-out for any workspace whose events are still undispatched after 5 minutes (up to 100 workspaces a tick), so a lost fan-out job never strands an event.
3. **Send.** `POST <url>` with the JSON body below, a 10 s timeout, through the outbound guard: private, loopback, link-local and metadata addresses are refused on every attempt (names are re-resolved), and redirects are never followed.

```json
{ "id": "<event id>", "type": "stock.changed", "created_at": "…", "data": { … } }
```

Headers (the [Standard Webhooks](https://www.standardwebhooks.com) format, so stock verifier libraries work):

| Header | Value |
|---|---|
| `webhook-id` | event id; the same on every retry, so drop duplicates on it |
| `webhook-timestamp` | Unix seconds of this attempt |
| `webhook-signature` | `v1,<base64 HMAC-SHA256(key, "{webhook-id}.{webhook-timestamp}.{body}")>`; `key` is the base64-decoded part of the secret after `whsec_` |

Verify with a constant-time compare and reject old timestamps (5 minutes is usual).

**Outcomes.** `2xx` delivered. Any other status (a `3xx` included), a network error, or a failure before sending (no server key) is retried by the job rail: 12 attempts, 10 s doubling to 1 h, about 3.5 h in all. After the last one the delivery is `failed`, the dead letter, kept in `webhook_deliveries`. An inward IP target, or a secret that can't be opened, fails at once. Outbound requests never use an HTTP proxy (it would resolve names past the guard). An inactive or deleted endpoint gets nothing. **Delivery is at least once:** a worker that dies after the POST but before recording it sends again, with the same `webhook-id`. Deduplicate on that header. Order between events is not guaranteed: use `quantity_after` or a re-read, not arrival order.

**Retention.** The hourly prune deletes deliveries untouched for 30 days (finished ones, and any a crashed job left pending), then dispatched events with no delivery left.

## Endpoints (workspace owner or `workspace_admin`)

Other members `403`, checked before the body is read; outsiders and other workspaces `404`. At most 10 endpoints per workspace (`422` on `url` beyond that). Needs `CREDENTIALS_KEY` on the server (the secret is sealed with it); without it create is `503`.

### `POST /v1/workspace/{workspace}/webhook-endpoints`

```json
{ "url": "https://shop.example.com/hooks", "events": ["stock.changed"], "description": "Web shop" }
```

| Field | Rule |
|---|---|
| `url` | required, `https://`, dotted host, no user info, ≤ 2000 chars |
| `events` | required, 1–50 known types (duplicates dropped) |
| `description` | optional, ≤ 500 chars |

`201 {"data": {id, url, events, description, active, created_at, updated_at, secret}}`. Store `secret` (`whsec_…`) now: it is never shown again.

### `GET …/webhook-endpoints?page&page_size`, `GET …/webhook-endpoints/{endpoint}`

The endpoint without its secret; lists newest first.

### `GET …/webhook-endpoints/{endpoint}/deliveries?status&page&page_size`

Delivery history: why a webhook failed. One row per event sent to this endpoint, newest first: `{id, event_id, event_type, status, attempts, last_status, last_error, created_at, updated_at, delivered_at}`. `status` is `pending` (still retrying), `delivered` or `failed` (gave up after the last attempt). `last_status` is the HTTP status of the last answer, null when none came. `last_error` has URL query strings cut, because they may carry tokens. No secret and no endpoint URL field. `?status=` filters to one state; any other value is `422`. An endpoint of another workspace, or a malformed id, is `404`. Rows go after 30 days with the prune.

### `PATCH …/webhook-endpoints/{endpoint}`

Any of `url`, `events`, `description` (null clears), `active`. Returns the endpoint. `422` on invalid input.

### `DELETE …/webhook-endpoints/{endpoint}`

`204`. Deletes its delivery history too.
