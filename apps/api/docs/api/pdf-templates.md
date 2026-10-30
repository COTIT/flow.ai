# PDF templates API

Per-workspace layouts for printed documents. Ported from vimdesk-api `src/Domain/WorkspaceSetting/PdfTemplate` (Store/Update/ListPdfTemplateRequest, PdfTemplateResource, PdfTemplateFields, the preview controller). PDFs are rendered by the Gotenberg sidecar (see "Rendering") and stored as files (`files.md`).

Shared rules (guard order, 404s, list `meta`) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}/settings/pdf-templates`.

## Routes

| Route | Permission | Answer |
|---|---|---|
| `GET /` | `pdftemplate.list` and `pdftemplate.view` | paginated templates, newest first |
| `GET /available-fields/{domain}` | `pdftemplate.view` | `{data: {fields, variables}}` |
| `GET /{id}` | `pdftemplate.view` | `{data: template}` |
| `POST /` | `pdftemplate.create` | `200 {"data": {"pdf_template_id": "<uuid>"}}` (Laravel answered 200) |
| `POST /{id}` | `pdftemplate.update` | `{"message": "ok"}` |
| `DELETE /{id}` | `pdftemplate.delete` | `{"message": "ok"}` (soft delete) |
| `GET /{id}/preview` | `pdftemplate.view` | `{data: FileResource}` of the rendered PDF |

- Bodies up to 512 KiB.
- An unknown, malformed, deleted or other workspace's id is a 404.

## List filters

`page`, `page_size`, plus:

| Query | Matches |
|---|---|
| `domain` | that domain |
| `q` | name, case-insensitive substring |
| `language`, `relation_type`, `relation_custom_type_id` | templates whose condition equals the value **or is unset** |

## Body (create and update)

| Field | Rule |
|---|---|
| `name` | required string ≤ 255 |
| `domain` | create only, required: `invoice`, `estimate`, `order`, `purchase_order`, `collective_invoice`, `work_order`, `cash_register`, `relation`. Ignored on update |
| `domain_action` | create only; string ≤ 255. Ignored on update |
| `columns` | required non-empty list of `{label?, alignment, fields}` |
| `columns.*.label` | string ≤ 55 |
| `columns.*.alignment` | required: `left` or `right` |
| `columns.*.fields` | required non-empty list; each an item field key of the template's domain (see available fields) |
| `header_left`, `header_right`, `header_top`, `footer` | HTML string ≤ 65535 characters |
| `padding_top`, `padding_bottom`, `padding_x` | required number 0..40 (millimetres), stored rounded to a whole number |
| `enable_summary` | boolean, default true |
| `default` | boolean, default false. Nothing keeps it to one template per domain (as in Laravel) |
| `background_file_id`, `logo_file_id` | a stored image of this workspace, at most 2 MB (`The selected logo file id is invalid.`, `The logo file id must be an image.`, `The logo file id must not be greater than 2048 kilobytes.`). Deleting that file later leaves the template without it |
| `conditions` | object: `language` (≤ 3), `relation_type` (`business` or `personal`), `relation_custom_type_id` (a live relation custom type) |
| `extra_fields` | object of booleans `display_procedures`, `display_signature`, `display_hours_with_tariff`, `display_hours`, `display_planned_dates`, `display_description`, `display_invoice_specifications`, plus `pdf_template_id_for_invoice_specifications` (uuid). Other keys are dropped |
| `meta` | object, ≤ 1000 JSON nodes / 16 KiB |

- Update takes the same required fields as create (Laravel's update rules). The other fields are patches: left out keeps the stored value, null clears it.
- Strings are trimmed and a blank one is null.

## Template resource

```json
{
  "id": "…", "name": "Default invoice", "domain": "invoice", "domain_action": null,
  "columns": [{ "label": "Qty", "alignment": "right", "fields": ["quantity"] }],
  "padding_top": 10, "padding_bottom": 13, "padding_x": 8,
  "enable_summary": 1,
  "header_left": "<p>…</p>", "header_right": null, "header_top": null, "footer": null,
  "default": true,
  "conditions": { "language": "nl", "relation_type": "business", "relation_custom_type_id": null },
  "extra_fields": { "display_description": true },
  "created_at": "…", "last_updated_at": "…", "meta": null,
  "relationships": { "backgroundFile": null, "logoFile": { …FileResource } }
}
```

`enable_summary` is `1`/`0`: Laravel left the column uncast.

## Available fields

`GET /available-fields/{domain}` returns Laravel's `PdfTemplateFields` output for that domain, generated from the PHP source by `tests/fixtures/pdf_template_fields_oracle.php` and served byte for byte (key order kept). `fields` maps each item field key to `{label, type}`; `variables` lists the `{dotted.key}` placeholders the texts can use. An unknown domain gives `{"data": {"fields": [], "variables": []}}`.

## Placeholders

`header_*` and `footer` may hold `{dotted.key}` placeholders such as `{invoice.reference}` or `{invoice.relation.company_name}`.

- The value is HTML-escaped before it goes in. Laravel inserted it raw, so a relation named `<b>` broke the layout.
- Values are inserted in one pass and never expanded again: a note holding `{invoice.reference}` prints those characters. Laravel ran a second pass over the result.
- An unknown placeholder prints `-`.
- `{invoice.customer_references}` is the one HTML value: the invoice's customer references, one escaped `label: value` per line.

## Preview `GET /{id}/preview`

Renders the workspace's newest invoice with this template, stores the PDF as a file named `test-preview-pdf-template-{unix time}.pdf` and returns its `FileResource`.

- No invoice yet: `422 {"message": "Create an invoice to preview this template."}`.
- Templates of other domains also preview against an invoice: only invoices are ported.
- Without the sidecar: `503 {"message": "PDF rendering isn't configured."}`; without storage: `503 {"message": "File storage isn't configured."}`.

## Rendering

The API builds the HTML with askama (`templates/pdf/`) and posts it to Gotenberg 8 (`POST /forms/chromium/convert/html`): A4, the template's paddings as page margins, printed backgrounds on when there is a background image.

- The header and footer texts are part of the page body; the background image goes in Gotenberg's page header as a full-page image.
- Logo and background are inlined as `data:` URIs; the CSS is inlined too. Each page pins `Content-Security-Policy: default-src 'none'; img-src data:; font-src data:; style-src 'unsafe-inline'`, so template HTML can't load a URL or a `file:` path.
- The sidecar runs with JavaScript off, a 30 s limit, on an internal network with no published port, with every capability dropped. Chromium may load only `data:` URIs and the render's own `index.html`. HTML that loads or navigates to anything else (a `<meta http-equiv="refresh">`, say) is blocked: the page comes out blank, or the sidecar refuses it (`422 {"message": "This document can't be rendered: its template loads something it may not."}`) or fails (503). The API gives each render 35 s.
- One render per workspace at a time (shared with uploads) and at most 2 per API instance; a request waits up to 10 s for each, then gets a 503 with `Retry-After`.
- A sidecar that fails or is down: `503 {"message": "PDF rendering failed. Try again later."}`. A PDF over 25 MiB is an error (500) and is not stored.
