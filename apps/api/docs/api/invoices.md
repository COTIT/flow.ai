# Invoices API

Invoices, their item lines, sending, numbering and credit notes (payments: `invoice-payments.md`). Ported from vimdesk-api `src/Domain/Finance/Invoice` (Store/UpdateInvoiceRequest, Create/Update/Delete/DuplicateInvoice, SendInvoice, ResentInvoice, SendInvoiceMail, GenerateInvoiceNumber, CreditInvoice, InvoiceResource, InvoiceIndexResource, InvoiceFromHashResource) and `src/Domain/Shared/Item` (BuildRequestItems, HandleItems, ItemResource). Payments come in F7, the Mollie payment link in F8.

Shared rules (guard order, 404s, list `meta`) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}`, except the public link.

## Routes

| Route | Permission | Answer |
|---|---|---|
| `GET /invoices` | `invoice.list` | paginated rows, or `{data:[…]}` with `limit` |
| `GET /invoices/{id}` | `invoice.view` | `{data: invoice}` |
| `POST /invoices` | `invoice.create` | `201 {"data": {"invoice_id": "<uuid>"}}` |
| `POST /invoices/{id}` | `invoice.update` | `{"message": "ok"}` |
| `DELETE /invoices/{id}` | `invoice.delete` | `{"message": "ok"}` (cancels) |
| `POST /invoices/{id}/duplicate` | `invoice.create` | `201 {"data": {"invoice_id": "<uuid>"}}` |
| `GET /invoices/{id}/preview` | `invoice.view` | `{data: FileResource}` of the rendered PDF |
| `POST /invoices/{id}/send` | `invoice.send` + `workspace.send-message` | `{"message":"ok","delivery":{…}}` |
| `POST /invoices/{id}/resend` | `invoice.send` + `workspace.send-message` | `{"message":"ok","delivery":{…}}` |
| `POST /invoices/{id}/credit` | `invoice.credit` + `invoice.create` | `201 {"data": {"invoice_id": "<credit note>"}}` |
| `GET /invoices/send-tasks/active` | `invoice.list` | `{data:[…]}` (at most 100 pending/unknown receipts) |
| `GET /v1/invoices/fetch-from-hashed/{token}` | none (public, rate limited) | `{data: public invoice}` |

- A body may be up to 2 MiB, with up to 1000 item rows, as on the item-total routes. Its body deadline is twice the normal one.
- An invoice holds at most 1000 lines. A write that would leave more is a 422 on `items`: `An invoice holds at most 1000 items.`
- An unknown, malformed or other workspace's id is a 404. Laravel answered 422 for a malformed id.

## Create `POST /invoices`

| Field | Rule |
|---|---|
| `relation_id` | required; a live relation of this workspace (`The relation id is an invalid id for the relation domain.`) |
| `currency_id` | optional; a live currency, else the workspace's main currency (`No default currency has been found. Please create one` when there is none) |
| `addressed_contact_id` | optional; a live contact of that relation. Stored as a snapshot (`uuid`, `initials`, `prefix`, `first_name`, `middle_name`, `last_name`, `job_title`, `email`, `default_for`, `phones`) |
| `billing_address` | optional address object (the relation address rules; field errors use bare keys such as `address`). Left out: a snapshot of the relation's `billing` address, else its main one, else its first |
| `reference` | string ≤ 255 |
| `note` | string ≤ 1000 |
| `payment_method` | `bank_transfer` (default) or `mollie`, any case |
| `is_credit` | boolean |
| `meta` | object |
| `items` | list of item rows, see below; may be left out, null or `[]` |

- The invoice starts as `draft`. It has no dates: `send_date` and `due_date` are set when it is sent.
- `meta`, `billing_address` and `customer_ref` are refused with `The X is too large.` past 1000 JSON nodes or 16 KiB of keys and strings. The bound is applied while parsing, so a 2 MiB body never grows into a big tree; the database also caps `meta` and `customer_ref` at 16 KiB of JSON text.
- Strings are trimmed and a blank one is null (Laravel TrimStrings + ConvertEmptyStringsToNull).
- `custom_sections` are not stored: custom fields aren't ported.

## Item rows

Each row of `items`:

| Field | Rule |
|---|---|
| `id` | uuid of a line of **this** invoice (update) |
| `deleted` | boolean; `true` removes the line with that `id` |
| `title` | required string ≤ 255 |
| `quantity` | required number |
| `price` | number; required unless `product_id` is sent. A product line without a price stores 0 (Laravel did no lookup) |
| `vat` | required number ≥ 0 |
| `productDiscount` | number 0..100: the line's discount percentage |
| `purchase_price` | number; stored as sent, else 0 |
| `description` | string ≤ 5000; on an update row, left out keeps it and null clears it |
| `position` | integer; else the row's index in `items` |
| `product_id` | a live product of this workspace (new lines only; the product of a line never changes) |
| `product_variant_id` | a live variant of the line's product; any other is dropped (Laravel dropped unknown ones). A new product line takes the product's default variant |
| `mutate_from_warehouse_id` | a live warehouse; on an update row, left out keeps it and null clears it. A new product line takes the product's default warehouse while it is live |

Numbers follow the shared decimal rules: at most 8 decimals, ±10¹⁶. `items.*.discount_percentage` is not read (Laravel dropped it too).

**Diff (update):**
- A row with the `id` of one of the invoice's lines updates it.
- A row with `deleted: true` removes it.
- A row without `id` is a new line.
- Lines the body doesn't mention stay as they are (Laravel did no full-set diff).
- An `id` that isn't a line of this invoice is a 422 `The selected items.N.id is invalid.`; deleting such an id is a no-op. Laravel updated or deleted another invoice's line.
- Set children (`multi_set_product_id`) and rows whose set parent was deleted are skipped. genesy has no product sets yet.
- Several rows with one `id`: the last one wins (Laravel applied them in order).
- A sent line equal to the stored one is not rewritten (no new row version per save).

## Update `POST /invoices/{id}`

Only a `draft` can change; anything else is a 422 `{"message": "This Invoice does not have the correct status to be updated."}`.

| Field | Rule |
|---|---|
| `relation_id` | required, as on create |
| `reference`, `note`, `meta`, `billing_address`, `addressed_contact_id` | as on create; a key left out keeps the stored value, null clears it |
| `due_date` | date, today or later (`The due date must be a date after yesterday.`) |
| `customer_ref` | object `{ordered_by, po_number, reference}`, each a string ≤ 255 (`po_no` is read as `po_number`) |
| `items` | item rows, see above |

`currency_id`, `payment_method` and `is_credit` are not updatable (Laravel).

Another `relation_id` without a new `addressed_contact_id` / `billing_address` clears the contact and takes the new relation's default billing address, so the old relation's contact never stays on the invoice (Laravel kept both).

## Delete `DELETE /invoices/{id}`

Cancels a draft: status `cancelled`, the row and its lines stay (Laravel never deleted). A cancelled invoice prints `invoice_nr: "N/A"`, can't change and still counts for the relation delete guard.

## Duplicate `POST /invoices/{id}/duplicate`

A new draft from any live invoice, whatever its status (the source is share locked while it is copied):
- **Copied:** relation, currency, credit flag, note and meta, plus every line (title, description, quantity, price, VAT, discount, purchase price, product, variant, warehouse), with positions renumbered from 0.
- **Not copied:** reference, billing address, addressed contact, payment method, dates and customer reference (Laravel). Laravel also reset the purchase prices to 0; genesy keeps them, so the margin stays right.

## Preview `GET /invoices/{id}/preview`

Renders the invoice as a PDF, stores it as a file (`files.md`) and returns its `FileResource`; `file_path` is a signed download URL valid for 10 minutes.

| Query | Rule |
|---|---|
| `pdf_template_id` | optional uuid (`The pdf template id must be a valid UUID.`) |

- With the id of a live `invoice` template of this workspace, that template is used (`pdf-templates.md`, "Rendering") and the file is named `Invoice_{number_}{action_}{unix time}.pdf`.
- Otherwise, as in Laravel, the built-in layout is used (`templates/pdf/invoice.html`, ported from the Blade view): file `Invoice_pdf_{number}.pdf`. Another workspace's or another domain's template id falls back to it as well.
- A draft has no number yet and prints `--draft--`; a sent invoice prints its number.
- Without the sidecar: `503 {"message": "PDF rendering isn't configured."}`; without storage: `503 {"message": "File storage isn't configured."}`.
- Render limits, sidecar failures (503) and refused template HTML (422) are as in `pdf-templates.md`, "Rendering".
- `{invoice.payment_link}` prints the public page once the invoice is sent.
- **Not ported yet:** the workspace logo on the built-in layout. Product images aren't printed.

## Totals

Lines price with `pricing::engine` at the currency's decimals: each line's amounts round on their own, VAT once per rate on the summed lines, total = excl + VAT (`pricing.md`). Every write recomputes them from the stored lines in its own transaction, under the invoice's row lock:

- `invoices.subtotal` / `total`: minor units (excl / incl), as Laravel's columns.
- `invoices.summary`: exact decimals and the per-rate breakdown `vats: [{rate, base, vat}]`.

- The margin is computed too: a line whose margin leaves the number range (`purchase_price × quantity` past ~10²⁸) is a 422 `The item totals are out of range.` on save, so the detail can always price what is stored.
- The totals keep the currency decimals they were computed with (`summary.dp`): editing the currency later doesn't change a stored invoice's detail or list row until it is saved again.

Laravel created lines without recomputing, so its stored totals of a fresh draft stayed 0 until a later save.

## Detail `GET /invoices/{id}`

Laravel InvoiceResource:
- **Top level:** `id`, `invoice_nr` (null until sent; `"N/A"` once cancelled), `status`, `reference`, `payment_hash` (the public link token once sent, else null), `payment_method`, `note`, `send_date`, `due_date`, `is_credit`, `previous_invoice_id` (a credit note's source), `mollie_refunds` (`[]`), `addressed_contact`, `billing_address` (address with `formatted`), `import_id` / `import_source` (null), `customer_ref`, `created_at`, `last_updated_at`, `meta`.
- **`allowed_actions`:** what the API accepts on this invoice now, in this order (enum `InvoiceAction`). Each action route runs the same rule under the invoice lock, so a listed action passes its state rule and an unlisted one gets that route's 422/400. Status rules only: not permissions, not a body's own validation.

  | Action | Listed when |
  |---|---|
  | `edit`, `send`, `cancel` | `draft` |
  | `resend` | numbered |
  | `duplicate` | always |
  | `credit` | not a credit note, numbered, and `paid`/`partially_paid`; with an on-account relation any status but `credited` |
  | `pay` | not a credit note, `pending` or `partially_paid` |
  | `mark_as_credited` | a `pending` credit note |
- **`money`:** every amount in integer minor units, computed in SQL from the stored totals: `{currency, decimals, subtotal, vat, total, paid, open}`. `vat` = total − subtotal, `open` = total − paid (negative when overpaid). `currency` is the currency's code (`asset_type`), `decimals` the places the amounts are in (frozen once sent). Clients format these and do no money arithmetic. Also on list rows.
- **`balance`** (deprecated, use `money`): `{open, paid, total}`: `open` = total − paid (negative when overpaid, as Laravel), `paid` = the paid, unvoided payments as Laravel's decimal text (`"60.00000000"`, or `0` without any), `total` from the lines.
- **`summary`:** as `calculate-items` prints it, `discounts` holding one null per discounted line.
- **`marge_calculation_summary`:** as `calculate-marge` prints it.
- **`items`:** see below.
- **`relationships`:** `relation` (RelationIndexResource), `currency`, `pdf_file` (the last sent PDF's FileResource; null before a send, or when that file was deleted), `payments` (unvoided, `invoice-payments.md`), `order` (`{id, number}` of the order it was made from, `POST /orders/{order}/invoice`; null otherwise), `custom_sections: []`, `child_invoices: []`. `estimate`, `parent_invoice`, `work_order`, `collective_invoice` and `cash_register` are null.

**Lines** carry, in minor units at `decimals`, priced in SQL on exact `numeric` the way `pricing::engine` prices a line (no floats; every rounding half away from zero, as the engine's `MidpointAwayFromZero`: 0.125 → 0.13, −0.125 → −0.13):
- `unit_price_minor`: price incl. VAT;
- `subtotal_minor`: excl, gross and discount each rounded; the lines' sum is `money.subtotal`;
- `total_minor`: `subtotal_minor` + the line's own VAT. Their sum can differ from `money.total` by half a cent per line, because document VAT is computed per rate.

`price_incl`, `price_incl_vat` and `total_incl` are deprecated.

**Lines** (ItemResource) print numbers with `formatDecimals`:
- `quantity`, `vat`, `subtotal`, `total`, `total_incl`, `discount_percentage`, `discount_amount`, `discount_amount_incl`, `purchase_price`, `purchase_price_total`, `purchase_price_incl`, `marge`, `marge_total`, `marge_percentage` (null without a purchase price) and `price_incl_vat` at 2 places.
- `price`, `price_incl`, `total_each_price_incl` and `vat_amount` at 4 places.
- `purchase_price` falls back to the product's purchase price when the line's is 0.
- These per-line figures are exact until printed (Laravel's floats were). Summed per-line VAT can differ from the document VAT by half a cent per line.
- `relationships.product` is there only for product lines, with the fields vimdesk-web reads: `id`, `name`, `product_nr`, `reference`, `reference_vendor`, `sale_unit`, `should_mutate_stock`, `purchase_price`, `price_incl`, `original_price_incl`, `total_available_stock` (Laravel embedded the whole list resource per line). `relationships.mutate_from_warehouse` is the compact warehouse or null.
- `credited_amount` (what credit notes took back), `amount_left_to_credit` (quantity − credited) and `mutated_from_stock` (what left stock on send) are stored. Other stock and set fields print their Laravel defaults (`"0.00"`, `false`, `[]`).

## List `GET /invoices`

Newest first; `page` / `page_size` (50, max 100), or `limit` (1..100) for a plain `{data:[…]}`.

| Param | Filter |
|---|---|
| `relation_id` | uuid |
| `status` | exact status |
| `in_status` | comma-separated statuses (each must be valid). Unpaid = `pending,awaiting_payment,awaiting_pos_payment,partially_paid` |
| `sent_before` | `send_date` before this (exclusive); unsent invoices drop out. RFC 3339 time, or `YYYY-MM-DD` = local midnight starting that day in the workspace `general.timezone` (UTC when unset or unknown). Unpaid older than 30 days: unpaid `in_status` + `sent_before=<today − 30 days>` |
| `due_before` | `due_date` before this (exclusive), same forms; invoices without a due date drop out. Overdue = unpaid `in_status` + `due_before=<today>` |
| `is_credit` | boolean |
| `on_account` | with `in_status`: those, or on-account invoices (`meta.on_account`) not paid, credited or cancelled |
| `q` | ≤ 100 chars; matches the reference or the relation's name or alias (ILIKE). Laravel searched Meilisearch; the search batch replaces this |

A malformed `sent_before` / `due_before` is 422 `The sent before must be an RFC 3339 time or a YYYY-MM-DD date.` (`due before` likewise).

Rows (InvoiceIndexResource):
- `id`, `invoice_nr`, `status`, `balance {paid, open}` (from the stored paid total, no payment is read), `reference`, `payment_hash` (always null in a list: the token prints on the detail only), `payment_method`, `note`, `is_on_account`, `send_date`, `due_date`, `is_credit`, `meta`.
- `total_amount`: the stored total, two places kept.
- `summary`: from the stored totals; no line is read.
- `has_activities`, `has_note_activity`, `has_collective_invoice`: false until those modules are ported.
- `customer_name` (Genesy addition): the relation's company name, else its alias, else its first contact's full name; null when none is set. Read from the embedded relation, no extra query.
- `relationships.{relation, currency}`.

## Delete guards on other modules

- `DELETE /products/{id}` is a 422 `{"message": "Product can not be deleted because it is used in items."}` while a line uses the product.
- `DELETE /relations/{id}` is a 422 on `relation_id`, `This relation cannot be deleted because it has invoices, orders, estimates, or work orders.`, while it has a live invoice. Cancelled invoices count.
- Both deletes lock the row `FOR UPDATE`. An invoice write key-share locks the relation and products it uses, so a delete that starts during that write waits for it and then sees the new rows.

## Send `POST /invoices/{id}/send`

The body is the `POST /send-message` body (`messages.md`, up to 1 MiB): `addressed_contact_id` (a contact of the invoice's relation, else 422 `The selected addressed contact id is invalid.`), `address`, `send_method`, `subject`, `body`, `to` / `cc` / `bcc`, `attachments`, `email_profile_id`, plus `pdf_template_id`.

**Checked before anything is stored** (a refusal leaves the draft as it was):
- the PDF sidecar and the file store are configured (503 otherwise, as on preview). Not for a manual send (`send_method: manual`): without either, it numbers the invoice and sets its status as below and renders no PDF (`pdf_file` stays null; preview still answers 503). With both, a manual send renders the PDF as before;
- `pdf_template_id`, when sent, is an invoice template of this workspace (422 on `pdf_template_id`: `The selected pdf template id is invalid.`; Laravel's preview fell back to the built-in layout, a send doesn't);
- for an email: the attachments, the profile and a mail server with a from address (the `/send-message` 422s), and a recipient: without `to` the addressed contact needs an email (422 on `to`: `The to field is required when the contact has no email address.`).

Only a `draft` is sent: anything else is `422 {"message": "This Invoice does not have the correct status to be send."}`. One transaction then:

1. stores the addressed contact snapshot and `address` as the billing address;
2. prices the draft again when its currency's decimals changed since it was saved (the currency row is share locked, so a decimals edit waits);
3. moves stock (below);
4. sets `send_date` (the moment of numbering) and `due_date` when they are empty;
5. sets the status: `pending`; with a total of 0, `paid` (invoice) or `credited` (credit note), as Laravel's balance update did;
6. creates the payment token (once per invoice);
7. takes the next number of the series, stores it with the appendix, and records delivery intent with its specific job.

After commit, in a task of its own (a request timeout or a client that leaves doesn't stop it; the caller still gets its outcome):

8. renders the PDF with `pdf_template_id` when that is an invoice template of this workspace, else the built-in layout, stores it as a file (`Invoice_pdf_{number}.pdf`) and sets it as the invoice's `pdf_file`;
9. mails it unless `send_method` is `manual` (below).

A failure in 8 or 9 answers with that error and preserves the invoice number. Proven pre-transport failures permit a later resend. Ambiguous transport or interrupted rendering remains unknown and blocks another send until verified. A second send of an already-numbered draft keeps its 422; overlapping eligible resends return 409 before replacing recipient/PDF snapshots.

### Due date

The first of these that is a whole number of days from 1 to 36500 (Laravel's falsy 0 falls through):

1. The relation's `terms_and_periods.invoice_payment_term` (credit note: `credit_invoice_payment_term`).
2. Settings `invoice.default_payment_term_company` for a business relation, else `invoice.default_payment_term` (credit note: the `credit` section).
3. Settings `invoice.default_payment_term`.
4. 14.

The date is today (UTC) plus that many days, at midnight UTC.

### Numbering

- Invoices and credit notes share one series per workspace (Laravel's MAX+1 counted both). A credit note takes the `credit` section's prefix, digits and suffix; an invoice the `invoice` section's.
- The series starts at settings `invoice.start_number`. Raising it later moves the series forward to that number; lowering it never moves it back. Laravel read the start number only for the first document.
- The number is taken last in the transaction from a counter row (`document_counters`), so its lock is held only until commit. A rolled-back send frees its number: no gaps.
- The number is unique per workspace (index backstop). A clash means the counter is behind: it is logged as an error and answered `503 {"message": "The invoice could not be numbered."}` until it is repaired: `UPDATE document_counters SET last_number = (SELECT max(invoice_number) FROM invoices WHERE workspace_id = $1) WHERE workspace_id = $1 AND series = 'invoice'`.
- The database refuses (SQL error 23001), whoever asks: deleting a numbered invoice, changing its number, appendix, credit flag or workspace, setting it back to `draft`, soft-deleting it, and `TRUNCATE` while any invoice is numbered. This also blocks a hard delete of a workspace with numbered invoices (bookkeeping retention).

### Stock

As Laravel SendInvoice::mutateStock, for every line whose product is `physical` or `physical_second_chance` with `has_stock`, and whose quantity hasn't fully left stock yet:

- **Warehouse:** the line's `mutate_from_warehouse_id`, else the product's default warehouse, else the workspace's main warehouse (live ones only). None: `422 {"message": "No warehouse available"}`.
- **Change:** −(quantity − already moved). A credit note's negative lines put stock back.
- **Floor:** a store may go negative. Any other warehouse refuses a shortage with `422 {"message": "Insufficient stock for product {reference_vendor - name} in {warehouse}. Available stock: {level}. Required: {quantity}"}`, and nothing of the send is stored (no number used).
- Each line's `mutated_from_stock` becomes its quantity.
- The movement has `related_domain: "invoice"` and `related_id` set; Laravel put the number in `reason`, which doesn't exist yet when stock moves.
- **Not for orders:** an invoice made from an order (`POST /orders/{order}/invoice`), and a credit note of such an invoice, move no stock on send. The order took it on complete and puts it back through `POST /orders/{order}/returns`; as Laravel, which skipped invoices of orders.
- **Not ported:** Laravel skipped invoices of orders (orders arrive in F12) and its Picqer sync (dropped).

### The mail

- `to`: the body's `to` addresses; none: the addressed contact's email. Laravel mailed only the first `to`.
- `subject` and `body`: `{invoice.*}` variables (the PDF template variables, `pdf-templates.md`) are filled in one pass; values are HTML-escaped in the body, except `{invoice.customer_references}`. Unknown `{…}` stay as typed (Laravel). Filled in, each may be at most 2 MiB (422 on `body`: `The body is too long once its variables are filled in.`).
- Attachments: the body's `attachments` plus the invoice PDF.
- A "View and pay invoice" button links to the public page while the invoice is open (not `paid`, `cancelled` or `credited`). Credit notes get no button (Laravel added one to every mail).
- The send log has `message_type` `send_invoice` (resend: `resend_invoice`) and names the invoice (`entity_type`, `entity_id`) for its mail history.

## Resend `POST /invoices/{id}/resend`

Same body and up-front checks. Needs a number: `422 {"message": "This Invoice has not been send yet."}`. Stores the new addressed contact and billing address, then mails as above. The stored PDF goes again; a new one is rendered only with a `pdf_template_id` or when the stored one is gone (Laravel). An invoice sent without a credential key gets its payment token here.

## Credit `POST /invoices/{id}/credit`

Makes a **draft credit note** that takes back what is left of the invoice's lines (Laravel CreditInvoice with `fromInvoice`).

| Field | Rule |
|---|---|
| `credit_all` | boolean; `true` credits every line's remaining quantity and ignores `items` |
| `items` | list (≤ 1000) of `{id, credit_amount, return_amount, disable_discount}` |
| `items.*.id` | required uuid of a line of this invoice (`item does not belong to given domain`) |
| `items.*.credit_amount` | number ≤ the line's remaining quantity (`more items are being credited while the maximum amount is: {left}`) |
| `items.*.return_amount` | number from 0 to what the credited quantity cost incl. VAT after discount (`The items.N.return amount may not be greater than {cap}.`) |
| `items.*.disable_discount` | boolean; accepted and ignored (Laravel ignored it here too) |

- **Allowed:** a sent (numbered) invoice that is `paid` or `partially_paid`; with a relation that allows invoicing on account, any status but `credited`. Laravel also credited on-account drafts (of the till, F15); an unsent invoice has moved no stock and has no number to credit. Else `422 {"message": "This Invoice has the status {status} and does not have the correct status to be credited."}`. A credit note is never credited: `422 {"message": "A credit note can not be credited."}`.
- **Lines:** a selected line with `credit_amount` > 0 becomes a line of −`credit_amount` at the source's price and discount. With a `return_amount` (the refund incl. VAT for that quantity), the unit price is `return_amount / (1 + VAT) / credit_amount` (8 decimals) and the discount is 0. The credit note's total can then differ from the return amount by a cent per VAT rate (VAT is computed on the rounded line, as on every invoice).
- The return-amount cap is the source line's incl. VAT amount for that quantity priced the way the invoice was (gross, discount and VAT each rounded at the invoice's decimals). A line selected without `credit_amount`, or with 0 or less, is skipped. Title, description, VAT, purchase price, product, variant and warehouse are copied; `parent_item_id` points at the source line.
- Nothing to credit: `422 {"message": "There are no items to credit."}` (Laravel printed its missing translation key).
- **Header:** relation, currency, reference and customer reference of the source; note = settings `credit.default_note`; `previous_invoice_id` = the source; totals as on every write.
- Each source line's `credited_amount` grows by what was taken. A `draft` or `pending` source with nothing left to credit becomes `cancelled`.
- The credit note is edited like any draft and sent like an invoice: it takes the next number of the shared series with the credit appendix, and its stock comes back.
- **Fixes on Laravel:** the return-amount cap (Laravel's never fired); the remaining quantity is checked under the source's row lock, so two credits at once can't take the same quantity.

## Public link `GET /v1/invoices/fetch-from-hashed/{token}`

For the SPA's `/clients/pay-for-invoice/{token}` page. No login; behind the public rate limit.

- The token is 32 random bytes (64 hex characters), made when an invoice is first sent. The database keeps its sha256 for the lookup and a sealed copy (credential key, path `invoices.{id}.payment_token`) so the detail, the PDF and later mails print the same link. Laravel's `payment_hash` was the encrypted ids. Without a credential key an invoice is sent without a link.
- Any other token (malformed, unknown, a draft's, or one of a deleted workspace) is a 404.
- `balance` is `{open, paid, total}`: `open` is total − paid but never below 0 (Laravel), and 0 for a `paid`, `cancelled` or `credited` invoice (a cash-rounded paid invoice shows its few cents in the detail only).
- `company_details` is the workspace profile with its bank account (for a bank transfer) but without `account.credit_limit`.
- **Answer** (InvoiceFromHashResource): `invoice_nr`, `is_credit`, `status`, `reference`, `balance {open, paid: 0, total}`, `payment_hash`, `payment_method`, `company_details` (the workspace profile), `due_date`, `items`, `summary`, `relationships.currency` (`name`, `asset_type`, `symbol_prefix`, `symbol_suffix`, `decimals`). `relationships.order_fields` is null.
- **Lines** print `id`, `title`, `description`, `quantity`, `price`, `price_incl`, `price_incl_vat`, `vat`, `discount` (= `discount_percentage`), `subtotal`, `total`, `total_incl`, `discount_fixed`, `discount_percentage`, `discount_amount`, `discount_amount_incl`, `credited_amount`, `amount_left_to_credit`. Laravel also printed purchase prices, stock and product codes to the customer; they are left out.

## Concurrency

- Each draft write takes its locks in a fixed order: invoice row (`FOR NO KEY UPDATE`), relation, products, variants, warehouses (`FOR KEY SHARE`, sorted). Parallel saves of one draft queue on its row and lose no line.
- **Send:** invoice row → its currency (`FOR SHARE`) → stock slots (sorted by product, warehouse, bin) → the counter row, last. The PDF and mail run after commit, holding no lock. A currency decimals edit locks only the currency row, so it waits for a send and never closes a cycle.
- **Credit:** the source invoice row → the new credit note (an insert). Stock and the counter are taken when the credit note is sent, on the send order above.
- Every path takes at most one invoice row and takes it first, so sends and credits of one invoice queue instead of deadlocking.
- Lines are written with one statement per kind (insert, update, delete via `UNNEST`), whatever the line count.
- Totals are stored before commit.

## Durable delivery receipts

`POST /invoices/{invoice}/send` and `/resend` keep `message: "ok"` and add `delivery`: `{invoice_delivery_id, invoice_id, status, send_log_id, pdf_file_id}`. Status is `queued`, `accepted`, `blocked`, `manual`, `failed` or `unknown`. Accepted means the mail server accepted transport, without confirming inbox delivery. `GET /invoices/send-tasks/active` returns at most 100 pending or uncertain delivery references for the workspace.

Numbering/recipient snapshots, delivery intent and its specific `invoice_delivery` job commit together. The request waits for one attempt. A restart can run queued intent; a claimed attempt is never automatically replayed. The request and worker share a once-only claim before rendering. Log insertion and the SMTP fence commit before transport. A five-minute sweep resolves known accepted/blocked send-log evidence or records interrupted attempts as unknown. Failures preserve the invoice number.

Send/resend returns 409 while that invoice has queued, running, sending or unknown delivery work. This protects its recipient and PDF snapshots from a late worker. Unknown work requires operator/send-log verification before another send; there is no automatic resend promise.

Pending business mail input contains the validated message and file references, never SMTP credentials or file bytes. It is bounded to 3 MiB and cleared on terminal/unknown outcomes. PostgreSQL backups can retain prior pending values. Resolved bounded delivery audit expires after 90 days; unknown receipts remain. Generated PDF ids are reserved with the delivery before rendering, so interrupted file writes retain a durable reference.
