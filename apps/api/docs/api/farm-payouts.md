# Payout desk and cash reconciliation

Staff pay a farmer cash from their passbook balance (G13). They follow the same four-eyes rules as vouchers. Every payout, void and cash movement is a new row: a database trigger refuses `UPDATE`, `DELETE` and `TRUNCATE` on `farm_payouts`, `farm_payout_voids` and `farm_cash_ledger`. Code: `src/modules/farm_payouts/`, migration `farm_payouts`.

Money is integer minor units. The cash ledger's `day` is the Tanzania date (UTC+3 all year).

## `POST /v1/workspace/{workspace}/farm/payouts`

```json
{ "client_ref": "…", "person_id": "…", "amount": 300000, "currency_id": "…",
  "id_type": "nida", "name_confirmed": true }
```

| Field | Rule |
|---|---|
| `client_ref` | required uuid, the retry key of the (offline) desk |
| `person_id` | required, a farmer in a village where the caller is staff (`404` otherwise) |
| `amount` | required integer ≥ 1, at most the passbook balance in that currency |
| `currency_id` | required, an active currency of this workspace |
| `id_type` | `nida`, `voter`, `driving_licence` or `village_letter` |
| `name_confirmed` | must be `true` |

Refusals:
- A farmer is refused with `403`.
- Four eyes (`403`, the same rule as vouchers, one shared check):
  - `you registered this household, so another staff member must pay this farmer`
  - `you verified this household, so another staff member must pay this farmer`
  - `you cannot pay your own household`
- Above the workspace setting `farm.payout_officer_limit` (minor units, default 10,000,000 = TZS 100,000.00), only ops or admin pay. Others get `403 this payout is above the officer limit: ops or admin must pay it`. Exactly at the limit is allowed.
- `422` on `amount`: `The amount must not be greater than the balance of {balance}.` A negative balance (a return after a payout) blocks every payout.

The farmer's row is locked before the balance is read, so parallel payouts never overdraw. One transaction writes the payout, a passbook line `payout` of `−amount` and a cash ledger `payout` row for the payer. The same `client_ref` and body by the same member return the stored payout with `200`; anything else under it gets `409`.

`201 {"data": {id, client_ref, person_id, village_id, amount, id_type, paid_by, paid_at, void}}` (`void` is `null` until voided).

## `POST …/farm/payouts/{payout}/void`

ops or admin; body `{reason}` (1–500 chars). Adds the void row, a passbook line `payout_void` of `+amount` and a cash row `payout_void` giving the amount back to whoever paid it. Returns the payout with `void: {reason, voided_by, voided_at}`. Voiding again is `409`.

## `POST …/farm/cash/movements`

`{kind, staff_user_id, village_id, currency_id, amount}`:
- `float_out`: ops hands the officer cash (+).
- `float_back`: the officer hands cash back (−).
- `count`: what the officer counted, recorded as is (0 allowed).

ops and admin record any kind. An officer records only their own `count`, in their village. `staff_user_id` must be staff of that village (officer there, ops or admin). `201` with the stored row.

## `GET …/farm/cash/reconciliation?day=YYYY-MM-DD&village_id=`

One row per officer and currency: `{staff_user_id, opening, floats, payouts, paid, voided, returned, expected, counted, variance}`.
- `opening`: every earlier movement.
- `payouts`: the number of payouts that day.
- `expected = opening + floats − paid + voided − returned`.
- `counted`: the day's last count; `variance = counted − expected`. Both are `null` when there was no count.

ops and admin see every officer; an officer sees only their own row. A farmer is refused with `403`, and a missing or bad `day` gets `422`.
