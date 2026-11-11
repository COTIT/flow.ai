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

## Below minimum (`stock.below_minimum`)

A stock-tracked product is below its minimum when its sellable `available` (the `out_of_stock` rule: level − reserved, held and expired lots left out, all warehouses) is less than its minimum. Minimum = the sum of the product's row `minimal_stock_level` when that is above 0, else the workspace `product.low_stock_threshold` (default 5). `shortfall` = minimum − available. The summary carries `stock.below_minimum {count, items[≤5]}` (largest shortfall first). The scheduled `stock` email keeps its own rule.

## Flow noticed `GET /attention/priorities` (`attention_priorities`, any member)

```json
{ "data": { "generated_at": "…", "restricted": ["invoices.overdue"],
  "groups": [{ "kind": "stock.below_minimum", "count": 3, "open_count": 3,
    "acknowledged_count": 1, "snoozed_count": 0, "snoozed": false,
    "headline_ref": "<product id>", "impact": { "open_orders": 8 },
    "actions": ["review", "acknowledge", "snooze", "adjust"],
    "items": [{ "ref_id": "…", "label": "Honey", "available": "2.00", "minimum": "5.00",
      "shortfall": "3.00", "open_orders": 8, "money": null, "since": null,
      "acknowledged": true }] }] } }
```

- Kinds and gates: `stock.below_minimum` (`stock.list`), `orders.open` (draft and open; `order.list`), `invoices.overdue` (`invoice.list`), `till.open_shifts` (`cashregister.list`), `lots.quarantined` (any member), `webhooks.failed` (`workspace_admin`). A kind without permission is named in `restricted`; it never shows as zero. Empty groups are left out.
- One statement per kind returns the items and the counts (window functions), so the headline and the list can't disagree.
- Stock impact: item `open_orders` = draft or open orders with a line on the product; group `impact.open_orders` = distinct such orders over the not-snoozed listed products. Both null without `order.list`. Other kinds: null.
- Items: at most 5, the caller's snoozed ones left out. Stock: most open orders, then largest shortfall, then name. Others as the summary.
- Group order: groups with something not snoozed first, then `impact.open_orders` desc, then the kind order above.
- `actions`: `review`, `acknowledge`, `snooze`; stock adds `adjust` with `stock.mutate` or `stock.set_stock_levels`.

## Marks `PUT /attention/marks` (`attention_marks_set`) and `DELETE /attention/marks/{mark}` (`attention_marks_delete`)

Body `{kind, ref_id, state, snooze?}`. `state` is `acknowledged` or `snoozed`; `snooze` (`until_tomorrow` = next local midnight in `general.timezone`, UTC fallback, or `seven_days`) is required with `snoozed` and refused with `acknowledged`. The server computes the end. One mark per user, kind and record; marking again replaces it. 200 returns `{id, kind, ref_id, state, snoozed_until, created_at}`.

- Marks are the caller's own: other members never see them; another member's mark id is 404 on delete.
- 403 without the kind's list permission; 404 when `ref_id` is not a live record of that kind in this workspace; 422 for an unknown kind or state, a bad id, or a wrong `snooze`.
- Acknowledge only flags the item. It stays listed until its condition clears (stock back at minimum, order confirmed, …), and nothing else changes.
- Hourly cleanup (`cleanup::prune`, `attention_marks`) drops snoozes over a day past their end, acknowledgements over 30 days old and marks whose record is gone. No history is kept.
