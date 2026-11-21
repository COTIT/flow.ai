# Payables (Flow ERP M5-A)

Supplier bills, supplier payments and refunds received, purchase returns and purchase cost
adjustments. Active accounting books only (setup books get 422 `accounting_not_configured`).
Masterplan §11, §13, §15.

All writes use the accounting envelopes: create `{client_ref, payload}`, actions
`{client_ref, expected_version, payload}`, a required `Idempotency-Key` header, permanent replay
by `client_ref`. Every post and reverse has a `/review` twin. The review writes nothing and
returns a `review_fingerprint`. The action recomputes everything under its locks and needs the
same fingerprint, otherwise 409 `stale_document`. Amounts are decimal strings in major units:
document currency for bills, payments and returns, functional currency for GRNI, FX, variance and
journal previews. Functional figures and journals need `accounting.read`.

All money math runs in SQL: `supplier_bill_plan` / `supplier_bill_journal`,
`purchase_return_plan` / `purchase_return_journal`, `receipt_line_capacity`, `grni_relief`,
`money_convert`, `money_allocate`, `money_round_div`, `tax_line`.

## Capacity (§11 BILL MATCHING)

For a booked, valued receipt line:

```
B = booked quantity                U = posted unbilled returns
M = posted bill matches            D = draft bill matches (reservations)
matchable_unbilled = B − U − M − D   (never below 0)
```

- A bill match and an unbilled return both spend `matchable_unbilled`. Both take the receipt
  line's row lock first, so two requests can't spend the same quantity.
- A billed return names the posted match it returns (`source_bill_allocation_id`). It never
  lowers M, so returned goods can't be billed again.
- Every return together stays within B.
- GRNI still carried per receipt line = booked net value − GRNI relieved by posted matches and
  unbilled returns (functional, and PO-currency nominal). A relief of `q` takes
  `round(rem × q / rem_q)`; the last relief takes the whole remainder.
- Lines booked before M5-A carry no PO-currency share (`doc_value`). They can't be billed or
  returned: 422 `unsupported_active_erp_operation`.

## Supplier bills `/supplier-bills`

Permission group `supplierbill` (list, get, create, update, match, post, reverse).

| Route | |
|---|---|
| `GET /supplier-bills` | newest first; `supplier_id`, `page`, `page_size` |
| `POST /supplier-bills` | numbered draft |
| `GET/PATCH /supplier-bills/{id}` | read; PATCH replaces a draft's fields and lines |
| `POST /supplier-bills/{id}/match` | replaces the draft's goods reservations |
| `POST /supplier-bills/{id}/post` (+ `/review`) | `{review_fingerprint}` |
| `POST /supplier-bills/{id}/reverse` (+ `/review`) | `{accounting_date, reason, review_fingerprint}` |

The create and update payload holds:

- `supplier_id`, `currency_id`, `supplier_invoice_number`
- `invoice_date`, `due_date?`, `accounting_date`
- `evidence_ref?`, `note?`, `corrects_bill_id?`
- `lines` (1–100)

Each line names exactly one of:

- `product_id`: goods. Matched to receipt lines of the same product, supplier and currency.
- `landed_cost_id`: clears one landed component booked on a receipt. The component's currency
  must be the bill's, `quantity` is 1, and each component is reserved by one live bill.
  The read line carries `landed: {purchase_order_id, purchase_order_number, receipt_id,
  receipt_number, evidence_ref, amount}` (null on other kinds).
- `expense_account_id`: an active expense account that isn't a mapped control account.

It also takes `quantity`, `unit_price` (per base unit, ≤8 places), an optional `tax_profile_id`
and `description`. The line's extension is computed in SQL.

**Duplicate invoice number.** A live (draft or posted) bill with the same supplier, currency and
invoice number (trimmed, case-insensitive) is refused: 422 on `payload.supplier_invoice_number`.
A reversed bill's number is reused only by a bill naming it in `corrects_bill_id`.

**Match.** The payload is `{allocations: [{bill_line_id, receipt_line_id, quantity}]}`, each
receipt line once per bill. An empty list releases every reservation. Any of these is a 422:

- more than `matchable_unbilled`
- another product, supplier or currency
- a bill line matched beyond its quantity

**Post** needs every goods line matched in full. The rate is the bill currency → functional rate
in force on `accounting_date` (no rate is a 422). Tax per line comes from `tax_line`, with the
invoice date as tax point. Per match `i`:

```
A   = AP functional = convert(Σ extension + Σ tax)
T   = recoverable tax functional
E   = expense lines functional (extension + nonrecoverable tax)
s_i = money_allocate(A − T − ΣE) over matches by nominal
g_i = GRNI relieved
fx_i = convert(GRNI nominal at bill rate) − g_i
v_i  = s_i − g_i − fx_i
```

Journal (`supplier_bill/<id>/posted`):

- DR `grni` Σg
- DR `recoverable_tax` T
- DR each expense account
- DR or CR `realized_fx` Σfx
- DR or CR `pending_purchase_cost_variance` Σv
- CR `payables` A (supplier on the line)

Post also opens a `payable` open item (bill currency nominal, functional A, due date). A bill
never moves stock. Nonrecoverable tax on goods or landed lines would change booked stock cost, so
it is refused: 422 `unsupported_active_erp_operation`. Bill it on an expense line.

Example (§19 F): receipt EUR 100 @1.10 books GRNI 110. A bill @1.20 gives AP 120, relieves GRNI
110 and books FX 10. Billing 4 then 6 relieves 44 then the exact 66.

**Reverse** works only on a posted bill with no live allocation on its AP item, no posted return
on its matches and no posted cost adjustment (otherwise 422 `dependent_effects_exist`). The date
is in an open period, not before the bill. Reverse:

- appends the mirror journal (`reversed`)
- closes the AP item with a `supplier_credit` twin applied to it (fx 0)
- releases the matches

## Supplier payments `/supplier-payments`

Permission group `supplierpayment` (list, get, create, allocate, reverse).

| Route | |
|---|---|
| `GET /supplier-payments` | newest first |
| `POST /supplier-payments` (+ `/review`) | payload below + `review_fingerprint` |
| `GET /supplier-payments/{id}` | `unapplied`, `allocations` (fx with `accounting.read`) |
| `POST /supplier-payments/{id}/allocate` (+ `/review`) | `{target_open_item_id, amount, review_fingerprint}` |
| `POST /supplier-payments/{id}/reverse` (+ `/review`) | `{accounting_date, reason, review_fingerprint}` |

The create payload holds `supplier_id`, `currency_id`, `direction` (`payment` or
`refund_received`), `amount`, `date` (the book's business day only), `location`, an optional
`reference`, a required `evidence_ref` and an optional `note`.

`location` is the shared money location. Only a `cash_place` (drawer with an open shift, or
vault) or a `bank_account` (with `reference`, `evidence_ref` and `confirmation_method`) is
accepted, and it must hold the payment currency. The functional amount is converted in SQL at
the rate in force on the date.

- **payment**: DR `payables` / CR cash or the bank's GL account. Opens a `supplier_credit`
  (debit) item. A foreign unapplied payment stays a monetary item in its currency (D23).
- **refund_received**: DR cash or bank / CR `payables`. Opens a `payable` (credit) item.
- **Cash place**: also writes a `cash_ledger` `payment` (−) or `receipt` (+) row.

**Allocate** applies a payment to a `payable` of the same supplier and currency, or a refund to a
`supplier_credit`. Each side relieves its proportional functional carrying (the settlement
trigger's formula). The difference is realized FX: a loss is DR `realized_fx` / CR `payables`,
a gain the other way, with effect `supplier_payment_allocation/<uuid>/allocated`.

Example: AP USD 100 carried at 120.00, paid USD 100 at 1.25 (125.00), allocated in full: FX
loss 5.00.

**Reverse** corrects an erroneous record. It needs an unapplied payment (otherwise 422
`dependent_effects_exist`) dated on the business day. It appends the mirror journal, a cash
counter-row for cash places, and closes the item with its twin. It is no proof money came back:
a refund received is its own record.

## Purchase returns `/purchase-returns`

Permission group `purchasereturn` (list, get, create, post, reverse).

| Route | |
|---|---|
| `GET /purchase-returns` | newest first |
| `POST /purchase-returns` | numbered draft |
| `GET /purchase-returns/{id}` | read |
| `POST /purchase-returns/{id}/post` (+ `/review`) | `{review_fingerprint}` |
| `POST /purchase-returns/{id}/reverse` (+ `/review`) | `{accounting_date, reverse_financial, reverse_physical, evidence_ref?, reason, review_fingerprint}` |

The create payload holds `receipt_id`, `purpose`, `reason`, `accounting_date`,
`supplier_credit_evidence_ref?`, `apply_credit?` and `lines`.

- `purpose` is `vendor_return`, or `receipt_correction` (unbilled lines only).
- Each line is `{receipt_line_id, quantity, source_bill_allocation_id?, warehouse_id?, bin_id?,
  lot_id?}`. Place defaults come from the receipt line.
- One line per receipt line, and one per product and warehouse.
- Capacity is checked at create and again at post.

**Post** takes the goods out at the current average C (`Valued::SupplierReturn`; the
pending-receipt guard and costing chronology apply). FX at return is 0.

- **Unbilled line**: DR `grni` g / CR `inventory` C / `purchase_return_variance` C − g.
- **Billed line**: the supplier credit at the original bill basis (the match's share of the
  line's nominal and recoverable tax, and of its AP functional). Journal: DR `payables` credit +
  tax / CR `recoverable_tax` / CR `inventory` C / `purchase_return_variance` the difference.
  Opens one `supplier_credit` item. FX realizes when the credit is applied.
- **`apply_credit`**: applies the credit at post to the original bill's open AP. All billed lines
  must be from one bill.
- Line facts are frozen at post: issue value, GRNI, credit, tax, variance (debit positive), fx.

Example (§19 D): Q1/V15 and entitlement 20. Inventory −15, explicit variance 5, final Q0/V0.

**Reverse** needs `reverse_financial` and `reverse_physical` both true. One component alone is
422 `unsupported_active_erp_operation`. Reverse:

- refuses when the credit is applied or refunded elsewhere (`dependent_effects_exist`)
- brings the goods back at their original C
- undoes its own credit application
- appends the mirror journal
- closes the credit with its twin
- restores the capacity

## Purchase cost adjustments `/purchase-cost-adjustments`

Permissions: `accounting.read` (list, get), `accounting.journal_create` (create),
`accounting.post` (post), `accounting.reverse` (reverse).

The create payload holds `bill_id` (posted), `reason`, `evidence_ref` and
`allocations: [{receipt_line_id, inventory_amount, cogs_amount}]`. Amounts are signed,
functional, and each receipt line must be one of the bill's matches or landed allocations.

**Post** is dated the business day. The split must have the sign of the bill's remaining pending
variance and be no larger than it. The remaining pending variance is Σ posted match variance
minus posted adjustments, shown as `bill_pending_variance`.

- The inventory part is a value-only pool entry (`cost_adjustment`, quantity 0). An empty pool is
  refused: put that share on COGS.
- Journal: DR or CR `inventory` X, DR or CR `cogs` Y, CR or DR `pending_purchase_cost_variance`
  X + Y.

**Reverse** works only while every touched pool has no later valued event. Otherwise it is 422
`dependent_effects_exist`: post a new current-period adjustment instead.

## Office pickers (M7-A)

Read-only routes for the Office screens. No writes, no locks, one query each; another workspace
gets 404. Each quantity or residual is exactly what the matching write accepts at that moment.

| Route | Permission | Returns |
|---|---|---|
| `GET /supplier-bills/{id}/match-candidates` | `supplierbill.match` | `{bill_id, candidates: [{bill_line_id, receipt_line_id, receipt_id, receipt_number, purchase_order_id, purchase_order_number, booked_at, matchable}]}` |
| `GET /supplier-bills/landed-candidates?currency_id=&bill_id=` | `supplierbill.create` | `{candidates: [{landed_cost_id, purchase_order_id, purchase_order_number, receipt_id, receipt_number, evidence_ref, amount}]}` |
| `GET /purchase-returns/candidates?receipt_id=` | `purchasereturn.create` | receipt, supplier, currency and per booked valued line `{receipt_line_id, product_id, product_name, warehouse_id, warehouse_name, booked, unbilled_returnable, billed: [{bill_allocation_id, bill_id, bill_number, supplier_invoice_number, returnable}]}` |
| `GET /supplier-payments/{id}/targets` | `supplierpayment.allocate` | paged `{open_item_id, classification, source_kind, source_id, source_number, supplier_invoice_number, issue_date, due_date, amount, residual}` |

- Landed candidates: booked landed components in `currency_id` (required, 422 without) that no
  draft or posted bill holds; `bill_id` keeps that draft's own components listed (edit). Newest
  first, at most 500. `amount` is the booked amount in the component currency.
- Match candidates: goods lines only, receipt lines of the bill's supplier, currency and product
  with `matchable` (B − U − M − D, this bill's own reservations not counted, since a match
  replaces them) above 0. First 500.
- Return candidates: `unbilled_returnable = min(matchable, B − returned)`; per posted match
  `returnable = min(match quantity − posted returns on it, B − returned)`. A missing
  `receipt_id` is 422.
- Payment targets: a payment's `payable` items, a refund received's `supplier_credit` items, same
  supplier and currency, residual above 0, oldest due first. Amounts in the payment currency.
- `GET /purchase-cost-adjustments` filters by `bill_id` and `supplier_id`.
- Bill lines carry `product_name`; bill matches `receipt_id` and `purchase_order_id`; return
  lines `product_id`, `product_name` and `warehouse_name`.

## Not in M5-A

- revaluation of open foreign items
- bank statement matching, reports, year close
- applying a kept supplier credit later (only a refund received settles it)
- nonrecoverable tax on goods lines
- reversing one component of a return alone
- late-receipt booking review
