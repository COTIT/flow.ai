# Correcting a bought farm delivery: design

Status: built 2026-11-09 (API `docs/api/farm-supply.md` "Corrections", Office `/supply/corrections`, Field passbook line, voice review card). Follow-up: the "season closed" part of D2-b is not enforced; the code has no season-close record. Only the sold-out rule applies until a season-close feature exists (coordinator decision 2026-11-09).

## Decided 2026-11-09

| # | Decision |
|---|---|
| D2-a | Ops or admin files a correction. A **different** ops or admin approves it (four eyes). Officers can't do either. |
| D2-b | A share can be corrected until the raw lot is sold out or the season closes, whichever comes first. |
| D2-c | If the farmer was already paid out, their balance goes negative and is recovered from future credits. The payout desk keeps refusing any payout above the balance. |
| D2-d | The farmer is notified by a passbook line, with the reason, in the Field app. SMS comes later and needs a third-party provider. |
| D2-e | Kg only. A wrong farmer is fixed as two deltas (−A, +B) in one approval. No price corrections. |
| D2-f | A cut is refused when the lot has already been dispatched over a supply link. More generally, any cut larger than the raw lot's free stock is refused. |

## The problem

When a farmer delivers coffee, an office user buys it into a supply batch (`farm_supply_create`). The kg shown for each farmer is frozen at that point. If the scale reading or the farmer was wrong, there is no way to fix it today. The voice agent says so plainly. Passbook lines are append-only: they are never edited or deleted.

Analogy: a shop till receipt. You never rub out a printed receipt. You print a correction slip that points at the original, says why, and carries the manager's signature.

## How the pieces link today (from the code)

| Step | Table / code | What it records | Money? |
|---|---|---|---|
| Buy | `farm_supply_batches` | village, crop, the new **raw lot**, agreed `price_per_kg` (minor units), currency, `kg_total` | No |
| Buy | `farm_supply_shares` | one row per farmer harvest: `person_id`, `harvest_report_id`, `kg` | No |
| Buy | `stock::mutate` in `farm_supply/service.rs::create` | `kg_total` goes into the raw lot at the farm's store ("buying is the receipt") | No |
| Sale | `farm_supply/sale.rs::on_sale` | for every supply lot an order takes: `credit_total = price_per_kg × kg sold`, rounded half up once (`credit.rs`), split over the batch's farmers **by their current share kg** (`repo::weights`, largest remainder) | Yes |
| Sale | `farm_sale_attributions` | one row per order × lot sold (or per return change) | total |
| Sale | `farm_passbook_entries` kind `sale_credit` / `sale_reversal` | one signed line per farmer, pointing at the attribution | per farmer |
| Payout | `farm_payouts` + passbook kind `payout` + `farm_cash_ledger` | cash at the desk; ID checked; four-eyes against own household; officer limit; refused if amount > balance | per farmer |
| Void | `farm_payout_voids` + passbook `payout_void` | new rows only | per farmer |

Every table above has a trigger that refuses UPDATE, DELETE (and TRUNCATE for the money tables).

Other readers of the share kg:
- `farm_harvest_committed_kg(report)` sums share kg, so one harvest can't be bought twice.
- `public_farmers` (lot passport page) groups farmers by their shares.
- `lineage` walks packed and transferred lots back to the batch, so sales of packed jars, or sales in a partner workspace over a supply link, still credit the batch's farmers.

**Key consequence.** The money already credited for a batch is `price_per_kg × kg sold`. It does not depend on the share kg. A share correction therefore never changes the total credited so far. It changes:
1. **who** got that money (the split between farmers), and
2. **how many kg** the raw lot holds (if the batch total changes), and so how much can still be sold and credited later.

## Proposed design

### 1. A correction record, never an edit

New append-only table `farm_supply_share_corrections` (same freeze trigger as the shares):

| column | meaning |
|---|---|
| `id`, `workspace_id`, `village_id`, `batch_id` | scope; FKs as composite `(workspace_id, …)` like the shares |
| `share_id` | the original `farm_supply_shares` line being corrected |
| `person_id` | the farmer whose kg changes |
| `kg_delta` | signed, `numeric(12,2)`, `<> 0` |
| `reason` | text, 1–500 chars, required |
| `requested_by`, `requested_at` | who asked |
| `approved_by`, `approved_at` | a second person; CHECK `approved_by <> requested_by` |
| `created_at` | when it took effect (= approval) |

A pending request lives in a separate small table (`farm_supply_correction_requests`, also append-only, with a `decided` row for approve or reject). Only an approval writes the correction.

Effective share kg = original `kg` + sum of its `kg_delta`s. It must stay `>= 0`, checked under a row lock on the batch.

Moving kg from the wrong farmer to the right one is one approval that writes two deltas (−A, +B) with the same reason. The batch total stays the same.

### 2. Readers use effective kg

- `repo::weights` (sale split) and `farm_harvest_committed_kg` read effective kg. Otherwise future sales split wrong, and a cut would free kg that the next buy then commits twice.
- `public_farmers` reads effective kg.
- Batch detail and the farmer's supply history show the original line with its corrections under it. They never show a silently changed number.

### 3. Stock follows the net kg change

If the corrections in one approval change the batch total (net `kg_delta` ≠ 0), one stock movement goes on the raw lot at the farm's store, with `related_domain = 'farm_supply_correction'` and `related_id = correction id`. The `Floor` check refuses a cut larger than the raw lot's free stock: kg already sold, packed into jars, or dispatched to a partner workspace can't be taken back.

### 4. Past credits are re-split, as new passbook lines

In the same transaction, for every sale/return attribution of the batch:
1. Recompute the per-farmer split of that attribution's `total` with the **corrected** weights (same `credit::split`).
2. Compare it with what each farmer actually got for that attribution (sum of their passbook lines pointing at it, plus earlier correction lines for it).
3. Post the difference per farmer as a new passbook line of a new kind `supply_correction`, pointing at the correction (new nullable `correction_id` column + FK; the `kind` CHECK gains the value). Zero differences write nothing.

For each attribution the differences sum to exactly 0, so the batch's total credit stays the same. Each farmer's balance moves by what they were over- or under-credited.

Performance: one query for the batch's attributions and one grouped sum of passbook lines by attribution and person. No per-attribution round trip. A batch with very many attributions can be capped (e.g. 5,000) with a clear 422, to be raised when real data needs it.

### 5. Already paid out

If a farmer was paid out from credit that the correction now takes away, their passbook balance goes **negative** (they owe the co-op). The payout desk already refuses any payout above the balance, so nothing more is paid until new credits bring the balance back above zero. The correction review screen says it plainly: "Juma Mussa will owe TZS 12,000.00 back; new payouts are blocked until future sales cover it."

No payout row or cash ledger row is touched. Cash already handed over stays recorded as handed over.

### 6. Who, how, and what is announced

- Request: ops or admin farm role (default). Officers can't.
- Approve: a different ops or admin (four-eyes; DB CHECK + service check). The approver sees the before/after kg per farmer, the stock change, and each farmer's balance before and after.
- On approval: one transaction writes the correction(s), the stock movement, the passbook lines, the farm audit row (`farm_write_audit`), and a webhook event `supply.corrected` (thin payload: correction id, batch id, farmer count, net kg).
- Voice: request only, behind a review card (kind `supply_correction`: batch label, farmer, kg before → after, reason, balance effect). Approval stays in the Office UI, never by voice, like payouts (`secure_ui`).

### 7. Privacy boundary

Every query filters by `workspace_id`. A correction can only reference shares, batches and lots of the caller's farm workspace. Tests prove another workspace gets 404 on request, approve and read. Partner workspaces that bought over a supply link see nothing new. Their sales keep crediting the farm workspace's batch through `lineage`, and future credits use the corrected split.

## What this design deliberately does not do

- **No price corrections.** A wrong price per kg changes the money total of every past sale of the batch. That is a separate, larger decision; proposed answer: no.
- **No edit or delete** of shares, batches, passbook lines, payouts or attributions, ever.
- **No automatic cash recovery.** A negative balance is recovered only from future credits, unless the user picks otherwise (D2-c).
- **No notification provider.** SMS/push would need a third-party provider (user decision).

## Decisions for the user

| # | Question | Default proposed | Alternatives |
|---|---|---|---|
| D2-a | Who may request, who may approve | ops/admin request; a **different** ops/admin approves; officers can't | admin only approves; one-step for small kg |
| D2-b | Time window | any time while the raw lot still has the kg a cut needs | only before the lot is sold out; only within the season; only N days after buying |
| D2-c | Farmer already paid out | allow a negative balance, recovered from future credits; payouts blocked meanwhile | require cash back at the desk (new `refund` passbook kind + cash ledger row); block the correction |
| D2-d | Farmer notification | new passbook line visible in the Field app; Office shows it on the farmer page | SMS / push (needs a provider) |
| D2-e | Scope | kg corrections, including moving kg between farmers of the same batch | kg only, no moves; also price (proposed: no) |
| D2-f | Lot already packed or dispatched over a supply link | refuse any cut larger than the raw lot's free stock | allow and record a stock loss |

## Build outline (once decided)

1. Migration: correction + request tables (freeze triggers), passbook `kind` + `correction_id`, `farm_harvest_committed_kg` reading effective kg.
2. Tests first (`tests/api/farm_supply_corrections.rs`):
   - happy reallocation (balances move, sum 0);
   - net cut with free stock; cut beyond free stock (422);
   - effective kg below 0 (422);
   - same person approves (403); officer requests (403);
   - farmer already paid goes negative and the next payout is refused;
   - future sale splits by corrected kg; committed kg drops after a cut;
   - append-only triggers;
   - another workspace refused (404).
3. Service in `farm_supply` (one transaction), handlers, docs `docs/api/farm_supply.md`, CHANGELOG.
4. Voice: catalog op for the request + review facts; approval stays in the UI.
