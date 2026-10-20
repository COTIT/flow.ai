# Relations and contacts API

Port of vimdesk-api `src/Domain/CRM/Relation` and `CRM/Contact`: the customers, suppliers and leads of a workspace, and the people at each one.

Shared rules (guard order, 404s, patch updates, list `meta`, money printing) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}`. The ids in paths and bodies are uuids, and `id` in every response is that uuid, as in Laravel.

## Routes

| Route | Permission | Answer |
|---|---|---|
| `GET /relations` | `relation.list` | `{data: [RelationIndexResource], meta}` |
| `POST /relations` and `POST /relations/` | `relation.create` | `201 {"data": {"relation_id"}}` |
| `GET /relations/{relation}` | member | `{data: RelationResource}` |
| `POST /relations/{relation}` | `relation.update` | `{"message": "ok"}` |
| `DELETE /relations/{relation}` | `relation.delete` | `{"message": "ok"}` |
| `GET /relations/{relation}/financial-summary` | member | `{data: {...}}` |
| `POST /relations/{relation}/contacts` | `contact.create` | `201 {"data": {"contact_id"}}` |
| `POST /relations/{relation}/contacts/{contact}` | `contact.update` | `{"message": "ok"}` |
| `DELETE /relations/{relation}/contacts/{contact}` | `contact.delete` | `{"message": "ok"}` |
| `GET /searches?domain=relation` | `workspace.global-search` | same shape as `GET /relations` |

- **Trailing slash.** vimdesk-web and the POS create with a trailing slash (`/relations/`), so both forms route to create.
- **Member-only reads.** Reading one relation and its financial summary needs workspace membership only. Laravel had no check there, and the order, invoice and POS forms load relations for members who hold other permissions.
- **Contacts belong to their relation.** A contact id used under another relation is 404, and so is an unknown or deleted relation.

## Create `POST /relations`

The body is a JSON object, and unknown keys are ignored (the web form sends `address: {}`, `default_validity_period` and others).

| Field | Rule |
|---|---|
| `type` | required, `business` or `personal` (any case, stored lowercase): "The selected type: X is invalid." |
| `relation_type_id` | uuid of a live `relation` custom type in this workspace; else 422 "The relation type id is not valid". Absent or null: the workspace's oldest relation type, which signup seeds as Supplier. No relation type at all: 422 "The relation type id field is required." |
| `source_type_id` | uuid of a live custom type in this workspace |
| `alias`, `company_name` | text ≤ 255 |
| `coc_number` | text ≤ 200 |
| `vat_number` | text; the two-letter prefix plus that country's pattern (ibericode/vat 2.1.2 shapes, no VIES call): "The Vat number must be a valid number issued by any Member State." |
| `email` | email ≤ 255, stored lowercase |
| `website` | `http(s)://` URL with a dotted host ≤ 255 |
| `phone_number` | text ≤ 55, normalized (below) |
| `default_language` | `en` or `nl` |
| `addresses` | array of address objects, at most one `main_address` |
| `financial_account` | `{account_name, account_number (IBAN), bic_swift (BIC), credit_limit ≥ 0}` |
| `allow_invoicing_on_account`, `disable_order_workflow_emails` | boolean (Laravel set: `true/false/1/0/"1"/"0"`) |
| `meta` | object or null |
| `created_from_pos` | boolean; when true, a company name, an alias, or contact first and last name is required |
| `contact` | object; becomes the relation's default contact (contact rules below, names optional) |
| `custom_sections`, `custom_section_forms` | null or a list without `section_id` (custom fields aren't ported, so every section is unknown: "this custom section does not exist for the selected domain") |
| `default_payment_term`, `default_vat_rate` | validated (`integer`, `numeric 0..100`) and dropped, as in Laravel |

- **Ignored on create.** Laravel's create rules had no `terms_and_periods`, `supplier_data`, `import_source` or `import_id`, so these keys are ignored; update accepts them.
- **One transaction.** The relation, its number and the inline contact are written together.

### Address objects

| Field | Rule |
|---|---|
| `address`, `city` | required text ≤ 255 |
| `house_number` | required, numeric (string or number), stored as text |
| `zipcode` | required text ≤ 20 |
| `country` | required text ≤ 255, stored uppercase |
| `house_number_addition` | text ≤ 10 |
| `address2`, `state`, `type`, `attn`, `phone_number` | text ≤ 255 |
| `label` | text ≤ 55 |
| `main_address` | boolean |

Address errors use bare keys (`zipcode`, `city`), and only the first bad item reports. That is what Laravel sent, and the web address form reads those keys. A non-array `addresses` is "The addresses is not valid"; two main addresses are "There should be one main address".

### Numbering

- **Format.** `relation_nr` is `RE-` plus the number padded to 4 digits: the first relation is `RE-0001`. Each row stores the prefix/digits/suffix it was numbered with (`relation_appendix`).
- **No reuse.** The number is the workspace's highest number plus one, deleted relations included.
- **No duplicates.** A per-workspace advisory lock makes parallel creates take turns, and a unique index backs it up. Laravel had neither and could hand out the same number twice.

### Phone numbers

Relation and contact phone numbers are normalized with the country of the main address (else the first address):

- `06 12345678` with NL becomes `+31612345678`
- `00…` becomes `+…`
- only NL, BE and DE national numbers get a country code; others keep their digits

## Read `GET /relations/{relation}`

Query: `slim=1` drops `supplier_discount_groups`, `relation_discount_groups` and `unpaid_invoices`. `include_unpaid_invoices=0` drops `unpaid_invoices`.

```json
{ "data": {
  "id": "…", "alias": "…", "company_name": "…", "coc_number": null, "vat_number": null,
  "type": "business", "email": "…", "email_validation": null, "website": null,
  "phone_number": "+31…", "relation_nr": "RE-0001", "import_id": null, "import_source": null,
  "allow_invoicing_on_account": false, "disable_order_workflow_emails": false,
  "terms_and_periods": { "invoice_payment_term": null, "credit_invoice_payment_term": null,
                         "collective_invoice_payment_term": null, "estimate_validity_period": null },
  "used_terms_and_periods": { "invoice_payment_term": null, "credit_invoice_payment_term": 30,
                              "collective_invoice_payment_term": 30, "estimate_validity_period": 30 },
  "default_vat_rate": "21", "default_language": null, "has_reverse_charge": false,
  "addresses": [ { "address": "…", "house_number": "11", "zipcode": "…", "city": "…", "country": "NL",
                   "house_number_addition": null, "address2": null, "state": null, "label": null,
                   "main_address": true, "type": "other", "formatted": "…", "search_format": "1339nm 11",
                   "attn": null, "phone_number": null, "reverse_charge": false,
                   "shows_reverse_charge_notice": false,
                   "vat_rates": [ { "vat_amount": 21.0, "name": "Hoog", "default_group": true } ] } ],
  "financial_account": null, "supplier_data": { … },
  "created_at": "…", "last_updated_at": "…", "vat_groups": [ VatGroupResource ], "meta": null,
  "relationships": {
    "relation_type": CustomTypeResource, "source_type": null, "contacts": [ ContactResource ],
    "supplier_discount_groups": [], "relation_discount_groups": [],
    "custom_sections": [], "custom_section_forms": [],
    "unpaid_invoices": { "count": 0, "allowed_for_collective": 0, "allowed_on_account": false },
    "counts": { "files": 0 }
  }
} }
```

**Conditional keys.** These follow Laravel `when()`: a key is missing, never null, when its condition is false.
- `email_validation` is present (null) only when the relation has an email.
- `supplier_data` is present only when stored.
- `default_vat_rate` is present only when it is non-zero.

**Computed fields:**
- `formatted`: the rinvex/countries 9.1.0 postal template of the address country (`src/modules/relations/countries.rs`, generated from the package's data files). A country without a template uses Laravel's default template; an unknown code uses Laravel's non-region layout.
- **Reverse charge.** Reverse-charge rules compare the address country with the workspace profile `address.country`:
  - `reverse_charge`: a foreign address and a VAT number
  - `shows_reverse_charge_notice`: also a company, with both countries in the EU
  - `has_reverse_charge`: the same test on the default address, for companies only
- **`vat_rates`.** The workspace VAT groups of that country, default first then highest rate. When the country has no 0 % group, a `{name, vat_amount: 0, default_group: false}` row is added, named "BTW-verlegd" for an `nl` workspace and "VAT reverse charge" otherwise. `vat_amount` is a JSON number here (Laravel float cast).
- **`default_vat_rate`.**
  - A company whose default address is outside the workspace country gets `"0"`; with no workspace country set, every addressed company counts as foreign, as in Laravel.
  - Otherwise it is the default VAT group of the default address country, printed by `format_decimals` (2 dp).
- **`used_terms_and_periods`.** Blanks are filled with Laravel's empty-settings defaults: invoice none; credit, collective invoice and estimate 14 days (personal) or 30 days (company).
- **Stubbed relationships.** Discount groups, custom sections and unpaid invoice counts are empty or zero until those modules are ported.

### ContactResource

`{id, full_name, prefix?, attn?, salutation?, initials, job_title, first_name, middle_name, last_name, email, email_validation?, phones: [{type, number}], default_contact, default_for, relation_id, created_at, last_updated_at, meta, relationships: []}`

- **`full_name`.** The first name (else salutation), then ` middle `, then the last name. The middle name is lowercase.
- **Optional keys.** `prefix`, `attn` and `salutation` are sent only when set. `email_validation` is sent (null) only with an email.
- **`phone_number`.** It is stored but never returned, as in Laravel.

## List `GET /relations`

**Paging.** `limit` (default 30; 200 or more gives 50; junk or ≤ 0 gives 30) and `page`. Rows come newest first, leads included. Unknown query params are ignored.

**Filters:**

| Param | Effect |
|---|---|
| `suppliers=1` | relation type `properties.is_supplier = true` and a company name; adds `open_orders: 0` |
| `include_advice=1` | adds `has_advice: false, advice_count: 0`; together with `suppliers`, `relationships` is only `{tags: []}` (Laravel loaded nothing) |
| `last_name` | a contact with exactly this last name, any case (≤ 255) |
| `zipcode` | an address with this zipcode, ignoring case, spaces and dashes (≤ 10) |
| `company_name` | exact company name, any case (≤ 255) |

**Search `q`** (≤ 255 chars; blank or `*` = no filter):
- **Splitting.** Commas outside quotes split it into phrases, and quotes are stripped. `null`/`undefined` tokens left by template strings are dropped, and a `q` emptied that way answers an empty page.
- **Matched fields.** Any phrase may match alias, company name, email, phone, website or relation type name (case-insensitive substring), a contact's email, or a contact whose first, middle or last names contain every word of the phrase.
- **Id-like input** (uuid, digits, codes like `RE-0012`) also matches exactly on id, import id, email, phone and relation number.
- **Phone-like input** (7+ digits) matches the stored `+31…`, `0031…`, `0…` spellings on relation and contact phones.
- **Escaping.** `%` and `_` match literally.

Row shape (RelationIndexResource): `id, alias, company_name, type, email, email_validation?, website, phone_number, relation_nr, default_language, addresses (stored, VAT keys null), created_at, last_updated_at, allow_invoicing_on_account, disable_order_workflow_emails, supplier_data (null when every top-level value is empty), activities_count: 0, has_activities: false, has_note_activity: false, tags_count: 0, relationships: {relation_type, source_type, contacts, tags: []}, open_orders, open_order_value: "0.00"`.

`/searches?domain=relation&q=&is_supplier=` uses the same search and row shape, with the registry's `page`/`page_size` paging.

## Update `POST /relations/{relation}`

- **Required.** `type` and `relation_type_id` are required (Laravel's update rules).
- **Patch.** Every other key is a patch: an absent key keeps its value, and null clears a nullable field.
- **Never-null fields.** `allow_invoicing_on_account` and `disable_order_workflow_emails` can't be null (422 "required").
- **Replacing fields.** `addresses` replaces the whole list, and `email` is lowercased. The number never changes.
- **VAT number.** It is checked only when the stored main (else first) address is in ibericode's EU list. Laravel checked against the stored address too, not the one in the same request.
- **`terms_and_periods`.** Four optional whole numbers 1..365, with errors at `terms_and_periods.<key>`. Laravel's rule named a validator that doesn't exist and answered 500.
- **`supplier_data`.** An object stored as sent, after these checks (errors at `supplier_data.<key>`):
  - `payment_term`: one of the 8 Laravel terms
  - `order_type`: `email` or `ftp`
  - `lead_time`: integer
  - `min_order_weight` and `min_order_value`: numbers
  - `fixed_shipping_cost`: `enabled` boolean; `amount_base` required when enabled and > 0; `vat` ≥ 0; `line_name` and `reference` ≤ 255
- **FTP password (write-only).**
  - Responses always show `supplier_data.order_ftp.password: null`.
  - On update, a blank or null password keeps the stored one (the web form posts back what it loaded), and a new value replaces it.
  - Once set, the API can replace a password but not clear it.
- **Ignored keys.** `import_source` and `import_id` (≤ 255) are accepted. `source_type_id`, `owner_id` and the singular `address` key are ignored, as in Laravel.

## Delete `DELETE /relations/{relation}`

Soft delete. Its contacts stay, and its number stays taken. Laravel refused (422) to delete a relation with invoices, orders, estimates or work orders. genesy has none of those tables yet, so delete always succeeds until the order/invoice batch adds the check.

## Contacts

- **Create.**
  - `first_name` and `last_name` are required (≤ 55, first letter uppercased).
  - `middle_name` (lowercased), `prefix`, `initials` and `salutation` are ≤ 55, and `job_title` is ≤ 100.
  - `email` is ≤ 255 and stored lowercase. `phone_number` is ≤ 55.
  - `phones` is a list of `{type: home|mobile|phone|phone2|work, number ≤ 30}`, with errors at `phones.N.type`.
  - `default_contact` is a boolean, `default_for` is one of the five `default_for_*` values, and `meta` is an object.
- **Update.** Every key is a patch. Laravel set `email` and `phone_number` to null on every update that left them out; genesy keeps them.
- **Default contact.** A relation has at most one live default contact (unique index). Creating or updating a contact as default clears the flag on the others in the same transaction. Laravel did this on update only, so a relation could end up with several defaults.
- **Delete.** Soft delete.

## Financial summary `GET /relations/{relation}/financial-summary`

The same keys as Laravel. Every amount is 0, because genesy has no invoices yet; the invoice batch fills in the real sums.

- `lite=1`: `relation_id, projected_ltv: null, total_spend, outstanding_amount, outstanding_count, total_customer_value, outstanding_balance, credit_limit, available_credit, account_manager_name: null, customer_since: null, last_payment_date: null, loyalty_points`.
- **Full keys.** `relation_id, first_send_date, last_send_date, months_since_last, orders_count, years_active, total_spend, avg_order_value, purchases_per_year, expected_lifespan_years: 5, projected_ltv, remaining_ltv, outstanding_amount, outstanding_count, total_customer_value, outstanding_balance, account_manager_name, customer_since ("Y-m-d H:i:s", the creation time), last_payment_date, credit_limit, available_credit`.
- **Credit.** `credit_limit` comes from `financial_account.credit_limit`, and `available_credit` is that limit (no outstanding balance yet).
- **`relation_id`.** It is the relation uuid (Laravel sent its integer id).

## Not ported

- **No client calls these routes:** merge, import-link, set-owner, search-or-create-by-email, history, label download, export, loyalty (7 routes), portal login/products, `get-address` (PDOK postcode lookup), email-suggestion routes, contact get/list, the leads routes. The `is_lead` column is kept, and leads appear in the list.
- **Not ported subsystems:**
  - Meilisearch (search is SQL)
  - audit history and workflows (no webhooks or AFAS sync for relations written here)
  - email validation checks (`email_validation` is always null)
  - custom fields, tags, files, activities (empty or zero values)
  - discount groups, purchase-order counts, loyalty

## Laravel differences

**Fixes:**
- Relation numbers can't be duplicated.
- A wrong `relation_type_id` is a 422; Laravel quietly used the default type.
- A deleted relation type no longer counts as valid.
- Updating a contact keeps the fields it doesn't send.
- One default contact per relation.
- A contact can't be changed or deleted through another relation.
- `terms_and_periods` values are validated instead of answering 500.
- The zipcode filter matches `1339NM` and `1339 NM` both.
- The FTP password is never returned.
- Inline contacts keep `default_for`, `initials` and `job_title`; Laravel dropped them.

**Other changes:**
- A missing email is stored as null; Laravel stored `""`.
- `addresses` is always an array, never null.
- Names are capitalized on contact update too.
- Search runs SQL only:
  - For plain words, the results are a superset of Laravel's SQL fallback.
  - It also matches contacts for id-like input.
  - Results are ordered newest first, not by Meilisearch relevance.
  - There is no typo tolerance or accent folding.
- `supplier_data` is checked loosely: enums, numbers and the fixed shipping cost rule. Errors use `supplier_data.` keys instead of bare ones.
- Validation messages are English for every workspace; Laravel used Dutch for `nl` workspaces.
