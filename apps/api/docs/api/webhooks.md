# Webhooks

Events sent to URLs a workspace registers. Code: `src/modules/webhooks/`.

## Events

| Type | When | `data` |
|---|---|---|
| `stock.changed` | a stock level changes (mutations, set levels, bin moves, invoice stock moves). Requirement-only changes send nothing. | `stock_id, product_id, warehouse_id, reason` (increase, decrease, set_level), `quantity_change, quantity_after` (decimal strings) |
| `product.changed` | a product or one of its variants is created, updated or deleted | `product_id, action` (created, updated, deleted; variant writes are `updated`) |

Payloads are thin: re-read through `/storefront` for the details. Planned, not sent yet: `order.created`, `lot.created`, `harvest.reported`, `passport.published`, `sale.attributed`.

## Delivery

1. **Outbox.** The change's own transaction writes a `webhook_events` row when an active endpoint of the workspace subscribes to that type, plus one `webhook_fanout` job per transaction. A rollback sends nothing.
2. **Fan-out.** The job writes one `webhook_deliveries` row per event and subscribed active endpoint (never twice) and queues a `webhook_deliver` job for each.
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

**Outcomes.** `2xx` delivered. Any other status (a `3xx` included), a network error, or a failure before sending (no server key) is retried by the job rail: 12 attempts, 10 s doubling to 1 h, about 3.5 h in all. After the last one the delivery is `failed`, the dead letter, kept in `webhook_deliveries`. An inward IP target, or a secret that can't be opened, fails at once. Outbound requests never use an HTTP proxy (it would resolve names past the guard). An inactive or deleted endpoint gets nothing. Order between events is not guaranteed: use `quantity_after`/re-read, not arrival order.

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

### `PATCH …/webhook-endpoints/{endpoint}`

Any of `url`, `events`, `description` (null clears), `active`. Returns the endpoint. `422` on invalid input.

### `DELETE …/webhook-endpoints/{endpoint}`

`204`. Deletes its delivery history too.
