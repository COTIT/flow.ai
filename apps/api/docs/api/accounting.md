# Accounting (native book core)

Base: `/v1/workspace/{workspace}/accounting`. One workspace is one book. Every route needs an `accounting.*` permission. It is checked before the body is read and before any replay is disclosed, so a caller without the right always gets 403, never a stored answer. Owners and `workspace_admin` pass every check.

Status of this release (Flow ERP M1): setup, chart of accounts, fiscal calendar and manual journals. Activation (opening conversion), period and year close, revaluation, open items, allocations, banking and exceptions come in later milestones. Nothing here touches existing orders, invoices, stock or cash.

## Common rules

- **Envelopes.** Create: `{"client_ref": UUID, "payload": {...}}`. Action on an existing record: `{"client_ref": UUID, "expected_version": n, "payload": {...}}`. Field errors inside the payload come back as `payload.<field>`.
- **Idempotency-Key** header is required on every write (`422 The Idempotency-Key header is required.` without it).
- **Permanent replay.** `client_ref` plus the operation is the command's identity, stored in `accounting_commands` and never pruned. The same caller sending the same body again gets the first answer back byte for byte, even days later and under another Idempotency-Key. A different body or another caller under the same `client_ref` gets `409`. A failed request stores nothing.
- **Versions.** Every committed change bumps `version` once; a replay doesn't. A stale `expected_version` gets `409` with code `stale_document`.
- **Amounts** are exact decimal strings in major units, at most the currency's decimals (`"12.50"`). They are stored as integer minor units.
- **Dates** are `YYYY-MM-DD`. Ranges are `[start_date, end_date)`: start inclusive, end exclusive.
- **Lists** return `{data, meta}` with `page` and `page_size`. Details and actions return `{data}`. Create is `201`; actions and reads are `200`.

### Reason codes

Coded errors return `{"message": "...", "code": "..."}`:

| Status | code | When |
|---|---|---|
| 409 | `stale_document` | `expected_version` or `review_fingerprint` no longer matches; the draft was posted meanwhile |
| 422 | `accounting_not_configured` | post/reverse (or their reviews) before the book is active with currency, timezone and all roles mapped |
| 422 | `accounting_period_closed` | no open period (in an open fiscal year) covers the date |
| 422 | `unsupported_active_erp_operation` | setup change after activation; edit or generic reverse of a non-manual entry; edit of a posted journal |

## Workspace gate

Writers that change the book take the transaction advisory lock `erp.book|<workspace uuid>`. Posting and reversal take it shared. Setup, fiscal years and periods take it exclusive, as will activation and close later. Several workspaces lock in ascending UUID order.

## Setup

`GET /setup` (`accounting_setup_get`, `accounting.read`) returns `state` (`setup` or `active`), `version`, `functional_currency` `{id, code, decimals}`, `timezone`, `cutover_at`, `activated_at`, `account_mappings` (role → account id), `missing_roles`, `configured` and `allowed_actions`.

`PUT /setup` (`accounting_setup_update`, `accounting.configure`), action envelope:

```json
{"client_ref": "…", "expected_version": 1,
 "payload": {"functional_currency_id": "…", "timezone": "Africa/Dar_es_Salaam",
             "account_mappings": {"cash": "<account id>", "revenue": "<account id>"}}}
```

Every field is optional. Fields you leave out stay as they are, and only the roles you send are remapped. Only allowed while `state` is `setup` (otherwise 422 `unsupported_active_erp_operation`). The currency must be a live currency of this workspace, the timezone an IANA name Postgres knows, and each mapped account active in this book.

Roles (26): receivables, unbilled_receivables, payables, inventory, grni, revenue, sales_returns, cogs, tax_payable, recoverable_tax, customer_deposits, supplier_advances, bank, cash, provider_clearing, inventory_adjustment, pending_purchase_cost_variance, purchase_return_variance, cash_over_short, realized_fx, unrealized_fx, rounding, opening_equity, retained_earnings, financial_discrepancy, financial_loss.

Every workspace has one setup row from creation on. There is no activation route yet: activation posts the opening balances and arrives with the opening conversion.

## Accounts

- `GET /accounts` (`accounting_accounts_list`, `accounting.read`), sorted by code.
- `POST /accounts` (`accounting_accounts_create`, `accounting.configure`): payload `{code, name, type, parent_id?}`. `code` is 1-32 letters, digits, `.` or `-` and unique per book. `type` is asset, liability, equity, income or expense. A parent must be in the same book and of the same type.
- `PATCH /accounts/{account}` (`accounting_accounts_update`, `accounting.configure`): payload `{name?, active?}`. An account mapped to a role can't be deactivated.

Resources carry `roles` (the roles mapped to the account).

## Fiscal years and periods

- `GET/POST /fiscal-years` (`accounting_years_list` / `accounting_years_create`): payload `{start_date, end_date}`. At most 24 months long, never overlapping another year.
- `GET/POST /periods` (`accounting_periods_list` / `accounting_periods_create`): payload `{fiscal_year_id, start_date, end_date}`. Must lie inside an open fiscal year and never overlap another period.

Both are enforced again by database triggers. Close arrives in M5.

## Journals

`GET /journals` (newest date first) and `GET /journals/{journal}` (`accounting.read`) return `id, number, kind, state, accounting_date, currency, functional_currency, reason, evidence_ref, reversal_of, reversed_by, period_id, posted_by, posted_at, version, totals, lines, allowed_actions`. Each line has `line_no, account_id, account_code, relation_id, source_ref, debit, credit, functional_debit, functional_credit, rate, rate_inverse, rate_source, rate_date`.

`kind` is one of manual, source, opening, reversal, fx_revaluation, year_end_close. This release writes only `manual` and `reversal`.

### Manual drafts

`POST /journals` (`accounting_journals_create`, `accounting.journal_create`):

```json
{"client_ref": "…", "payload": {
  "accounting_date": "2027-01-15", "currency_id": "…", "reason": "Accrue rent",
  "evidence_ref": "INV-77",
  "lines": [{"account_id": "…", "debit": "12.50"},
            {"account_id": "…", "credit": "12.50", "relation_id": "…", "source_ref": "x"}]}}
```

- 2 to 100 lines. Each line has exactly one of `debit` / `credit` above zero, no negatives, and no more decimals than the currency has.
- Accounts and relations must belong to this book. Accounts must be active.
- **Control accounts are refused:** a manual line can't hit an account mapped to receivables, unbilled_receivables, payables, inventory, grni, customer_deposits, supplier_advances, cash, bank, provider_clearing, financial_discrepancy, pending_purchase_cost_variance or opening_equity. These balances move only through their own workflows. Tax and retained earnings stay allowed.
- `bank_account_id` is refused until bank accounts exist.
- `reason` is up to 1,000 characters; `evidence_ref` and `source_ref` up to 255.

`PATCH /journals/{journal}` (`accounting_journals_update`, `accounting.journal_create`) replaces every field and line of a manual draft (action envelope). A posted journal gets 422 `unsupported_active_erp_operation`. Drafts have no number and no functional amounts.

### Post

1. `POST /journals/{journal}/post/review` (`accounting_journals_post_review`, `accounting.post`), no body. Writes nothing. It returns `version`, `period_id`, the lines with functional amounts and rate snapshot, `totals` and `review_fingerprint`.
2. `POST /journals/{journal}/post` (`accounting_journals_post`, `accounting.post`), action envelope with payload `{review_fingerprint}`.

Posting recomputes everything under locks and refuses when the fingerprint differs (409 `stale_document`). It needs an active, configured book, balanced entry-currency totals (422 on `lines`), and an open period covering the date. Functional amounts:

- When the journal currency is the functional currency, they are equal (rate 1, `identity`).
- Otherwise they use the latest workspace rate on or before the date: direct first, else the inverse of the reverse pair, never a third currency (422 when none exists).
- Each side's total is converted once and spread over that side's lines in line order. The last line takes the exact remainder, so functional debits always equal functional credits.

The journal gets the next number of the workspace's journal series, its period and a `manual_journal/posted` effect. A deferred database trigger refuses any posted entry that doesn't balance. Posted entries, their lines and effects can't be updated or deleted.

### Reverse (manual journals only)

1. `POST /journals/{journal}/reverse/review` (`accounting_journals_reverse_review`, `accounting.reverse`), body `{accounting_date, reason}`.
2. `POST /journals/{journal}/reverse` (`accounting_journals_reverse`, `accounting.reverse`), action envelope with payload `{accounting_date, reason, review_fingerprint}`. `expected_version` is the original's version.

The original must be a posted `manual` journal. Any other kind gets 422 `unsupported_active_erp_operation`: source entries are corrected through their own workflow. The date must not be before the original's and must fall in an open period. The reversal is a new posted `reversal` entry with every line's sides swapped and the same functional amounts and rate snapshot. It records a `manual_journal/reversed` effect, so a second reversal gets 409. The original never changes; its `reversed_by` points at the reversal. No bank or provider refund is started.

## Reports

Read-only, `accounting.read`, `GET` under `/accounting/reports`. Only posted journals count; drafts never do. Amounts are the stored functional values (the book's functional currency), summed and formatted in SQL as exact strings in major units ("1250.50"). No rate is applied again: a later currency rate never changes a posted figure. Ranges are `from` inclusive, `to` exclusive; `as_of` includes that day. Dates are `YYYY-MM-DD` and required; there's no default to today.

| Route | operation_id | Query |
|---|---|---|
| `/reports/trial-balance` | `accounting_trial_balance` | `from`, `to` |
| `/reports/general-ledger` | `accounting_general_ledger` | `account_id`, `from`, `to`, `page`, `page_size` (default 50, max 100) |
| `/reports/profit-and-loss` | `accounting_profit_and_loss` | `from`, `to` |
| `/reports/balance-sheet` | `accounting_balance_sheet` | `as_of` |

Each answers `{data: …}` with `functional_currency` and its dates.

- **Trial balance**: one row per account with a posted line before `to`, ordered by code (`account` = id, code, name, type). Columns: `opening_debit`/`opening_credit` (balance before `from`, shown on its own side), `debit`/`credit` (activity in the range), `closing_debit`/`closing_credit`. `totals` sums each column; each debit total equals its credit total. Year-end close entries count.
- **General ledger**: one account. `opening_balance`, `debit`, `credit`, `closing_balance` (balances are debit-positive, so a credit balance is negative), and `lines` = one page `{data, meta}` of posted lines oldest first (date, journal number, line number): `entry_id`, `number`, `kind`, `accounting_date`, `reason`, `line_no`, `relation_id`, `source_ref`, `debit`, `credit`, `balance`. `balance` is the running balance after the line; it carries on across pages. An account of another workspace, or none, gets 404.
- **Profit and loss**: `income` (credit-positive) and `expenses` (debit-positive) per account, `total_income`, `total_expenses`, `net_profit`. Year-end close transfers (`kind = year_end_close`) are left out.
- **Balance sheet**: `assets` (debit-positive), `liabilities` and `equity` (credit-positive) per account, their totals, `current_earnings` and `total_liabilities_and_equity` (= liabilities + equity + current earnings = `total_assets`). `current_earnings` is income − expense through `as_of` not yet moved to equity by a year-end close, across every year not yet closed; there's no split between this year and earlier years until year-end close exists. A close moves it into retained earnings, so it's counted once.

422 field errors: missing or malformed date, `to` not after `from`, missing or malformed `account_id`. A book without a functional currency gets 422 `accounting_not_configured`.

Not yet: party ledger, receivables and payables aging (they need open items and allocations, M4/M5), inventory valuation and close checks.

## Permissions

`accounting.read`, `accounting.configure`, `accounting.journal_create`, `accounting.post`, `accounting.reverse` are used now. `accounting.reconcile`, `accounting.close`, `accounting.revalue` and `accounting.activate` are catalogued for later milestones.
