# Invoices API (part 1: drafts)

Draft invoices and their item lines. Ported from vimdesk-api `src/Domain/Finance/Invoice` (Store/UpdateInvoiceRequest, Create/Update/Delete/DuplicateInvoice, InvoiceResource, InvoiceIndexResource) and `src/Domain/Shared/Item` (BuildRequestItems, HandleItems, ItemResource). Sending, numbering, PDF, payments and credit notes come in later steps.

Shared rules (guard order, 404s, list `meta`) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}`.

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
- A draft has no number yet and prints `--draft--`.
- Without the sidecar: `503 {"message": "PDF rendering isn't configured."}`; without storage: `503 {"message": "File storage isn't configured."}`.
- **Not ported yet:** the workspace logo on the built-in layout and the payment link (F8). Product images aren't printed.

## Totals

Lines price with `pricing::engine` at the currency's decimals: each line's amounts round on their own, VAT once per rate on the summed lines, total = excl + VAT (`pricing.md`). Every write recomputes them from the stored lines in its own transaction, under the invoice's row lock:

- `invoices.subtotal` / `total`: minor units (excl / incl), as Laravel's columns.
- `invoices.summary`: exact decimals and the per-rate breakdown `vats: [{rate, base, vat}]`.

- The margin is computed too: a line whose margin leaves the number range (`purchase_price × quantity` past ~10²⁸) is a 422 `The item totals are out of range.` on save, so the detail can always price what is stored.
- The totals keep the currency decimals they were computed with (`summary.dp`): editing the currency later doesn't change a stored invoice's detail or list row until it is saved again.

Laravel created lines without recomputing, so its stored totals of a fresh draft stayed 0 until a later save.

## Detail `GET /invoices/{id}`

Laravel InvoiceResource:
- **Top level:** `id`, `invoice_nr` (null; `"N/A"` once cancelled), `status`, `reference`, `payment_hash` (null), `payment_method`, `note`, `send_date`, `due_date`, `is_credit`, `previous_invoice_id` (null), `mollie_refunds` (`[]`), `addressed_contact`, `billing_address` (address with `formatted`), `import_id` / `import_source` (null), `customer_ref`, `created_at`, `last_updated_at`, `meta`.
- **`balance`:** `{open, paid: 0, total}`. Payments arrive in F7.
- **`summary`:** as `calculate-items` prints it, `discounts` holding one null per discounted line.
- **`marge_calculation_summary`:** as `calculate-marge` prints it.
- **`items`:** see below.
- **`relationships`:** `relation` (RelationIndexResource), `currency`, `payments: []`, `custom_sections: []`, `child_invoices: []`. `pdf_file`, `estimate`, `parent_invoice`, `order`, `work_order`, `collective_invoice` and `cash_register` are null.

**Lines** (ItemResource) print numbers with `formatDecimals`:
- `quantity`, `vat`, `subtotal`, `total`, `total_incl`, `discount_percentage`, `discount_amount`, `discount_amount_incl`, `purchase_price`, `purchase_price_total`, `purchase_price_incl`, `marge`, `marge_total`, `marge_percentage` (null without a purchase price) and `price_incl_vat` at 2 places.
- `price`, `price_incl`, `total_each_price_incl` and `vat_amount` at 4 places.
- `purchase_price` falls back to the product's purchase price when the line's is 0.
- These per-line figures are exact until printed (Laravel's floats were). Summed per-line VAT can differ from the document VAT by half a cent per line.
- `relationships.product` is there only for product lines, with the fields vimdesk-web reads: `id`, `name`, `product_nr`, `reference`, `reference_vendor`, `sale_unit`, `should_mutate_stock`, `purchase_price`, `price_incl`, `original_price_incl`, `total_available_stock` (Laravel embedded the whole list resource per line). `relationships.mutate_from_warehouse` is the compact warehouse or null.
- Stock, credit and set fields print their Laravel defaults (`"0.00"`, `false`, `[]`).

## List `GET /invoices`

Newest first; `page` / `page_size` (50, max 100), or `limit` (1..100) for a plain `{data:[…]}`.

| Param | Filter |
|---|---|
| `relation_id` | uuid |
| `status` | exact status |
| `in_status` | comma-separated statuses (each must be valid) |
| `is_credit` | boolean |
| `on_account` | with `in_status`: those, or on-account invoices (`meta.on_account`) not paid, credited or cancelled |
| `q` | ≤ 100 chars; matches the reference or the relation's name or alias (ILIKE). Laravel searched Meilisearch; the search batch replaces this |

Rows (InvoiceIndexResource):
- `id`, `invoice_nr`, `status`, `balance {paid, open}`, `reference`, `payment_hash`, `payment_method`, `note`, `is_on_account`, `send_date`, `due_date`, `is_credit`, `meta`.
- `total_amount`: the stored total, two places kept.
- `summary`: from the stored totals; no line is read.
- `has_activities`, `has_note_activity`, `has_collective_invoice`: false until those modules are ported.
- `relationships.{relation, currency}`.

## Delete guards on other modules

- `DELETE /products/{id}` is a 422 `{"message": "Product can not be deleted because it is used in items."}` while a line uses the product.
- `DELETE /relations/{id}` is a 422 on `relation_id`, `This relation cannot be deleted because it has invoices, orders, estimates, or work orders.`, while it has a live invoice. Cancelled invoices count.
- Both deletes lock the row `FOR UPDATE`. An invoice write key-share locks the relation and products it uses, so a delete that starts during that write waits for it and then sees the new rows.

## Concurrency

- Each write takes its locks in a fixed order: invoice row (`FOR NO KEY UPDATE`), relation, products, variants, warehouses (`FOR KEY SHARE`, sorted). Parallel saves of one draft queue on its row and lose no line.
- Lines are written with one statement per kind (insert, update, delete via `UNNEST`), whatever the line count.
- Totals are stored before commit.
