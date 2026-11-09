# Attention

`GET /v1/workspace/{workspace}/attention`: what an office operator should deal with today, in one read-only call. Bearer token; any active member. Outsider, unknown or malformed workspace: 404. Inactive member: 403. No token: 401.

Each section gives counts plus at most 5 items, oldest first. A section is `null` when the caller lacks the permission of its list route. Money is integer minor units with `currency` (upper-case code) and `decimals`.

| Section | Permission | Counts | Rule reused |
|---|---|---|---|
| `orders` | `order.list` | `draft`, `open`, `awaiting_payment` | Order actions: draft can be confirmed, open picked or completed |
| `invoices` | `invoice.list` | `draft`, `unpaid`, `overdue`, `open[]` | Invoice actions: draft can be sent; unpaid = `pay` allowed (`invoices::actions::PAYABLE`, not credit) |
| `stock` | `stock.list` | `negative`, `out_of_stock` | Negative-stock list (store warehouses, level below 0); storefront `sold_out` per product |
| `till_shifts` | `cashregister.list` | `open` | Shift without `closed_at` (as `GET /till/shifts`) |
| `lots` | any member | `quarantined`, `held_orders` | Lot status; holds on lots off sale (`GET /lots/{lot}/holds`) |
| `webhooks` | `workspace_admin` | `failed` | Dead-letter deliveries (kept 30 days) |

Definitions:

- `overdue`: unpaid and `due_date` before local midnight today in settings `general.timezone`. An unset or unknown zone counts as UTC.
- `out_of_stock`: a live product with `has_stock` whose available stock, Σ (level − reserved) over its live stock rows, is at most 0. Rows in a lot that is quarantined, recalled or past `expires_on` are left out. One empty lot next to a full one is not out of stock; a tracked product never stocked is.
- `held_orders`: distinct orders holding stock on a lot that is not released.
- `invoices.open`: Σ (total − paid) of unpaid invoices, one entry per currency, summed in SQL.

## 200

```json
{
  "data": {
    "generated_at": "2026-11-08T09:00:00Z",
    "orders": {
      "draft": 1, "open": 2, "awaiting_payment": 0,
      "items": [{ "id": "…", "number": "ORD-0001", "status": "draft",
                  "created_at": "…", "total": 1210, "currency": "EUR", "decimals": 2 }]
    },
    "invoices": {
      "draft": 1, "unpaid": 2, "overdue": 1,
      "open": [{ "currency": "EUR", "decimals": 2, "amount": 2210 }],
      "items": [{ "id": "…", "number": "INV-1001", "status": "pending", "customer_name": "Client BV",
                  "due_date": "…", "overdue": true, "open": 1210, "currency": "EUR", "decimals": 2 }]
    },
    "stock": {
      "negative": 1, "out_of_stock": 4,
      "items": [{ "product_id": "…", "product_name": "Tea", "available": "-2.00" }]
    },
    "till_shifts": {
      "open": 1,
      "items": [{ "id": "…", "register_id": "…", "register_name": "Front desk",
                  "opened_by": "Cas", "opened_at": "…" }]
    },
    "lots": {
      "quarantined": 1, "held_orders": 1,
      "items": [{ "id": "…", "code": "OIL-Q", "product_id": "…", "product_name": "Oil", "held_orders": 1 }]
    },
    "webhooks": {
      "failed": 1,
      "items": [{ "id": "…", "endpoint_id": "…", "event_type": "order.created", "attempts": 8,
                  "last_error": "connection refused", "updated_at": "…" }]
    }
  }
}
```

Item order: orders by `created_at`; invoices by `due_date` (none last); stock lowest `available` first; shifts by `opened_at`; lots by last change; webhooks by `updated_at`.

Performance: two statements per section (count, items), no per-row queries. Partial indexes (migration `attention_indexes`) cover open orders, open invoices, open shifts, lots off sale and failed deliveries. `out_of_stock` aggregates every tracked product of the workspace per call.
