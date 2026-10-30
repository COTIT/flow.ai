# Email templates API

A subject and body with `{variable}` placeholders, per domain. Ported from vimdesk-api `src/Domain/WorkspaceSetting/EmailTemplate` (Store/Update/ListEmailTemplateRequest, EmailTemplateResource, CreateEmailTemplate, DisableDefaultEmailTemplate, GetTemplateVariablesController).

Shared rules (guard order, 404s, list `meta`) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}/email-templates`.

## Routes

| Route | Permission | Answer |
|---|---|---|
| `GET /` | `emailtemplate.list` | paginated templates, newest first |
| `GET /variables/{domain}` | membership only (as Laravel) | `{data: {variables}}` |
| `GET /{id}` | `emailtemplate.get` | `{data: template}` |
| `POST /` | `emailtemplate.create` | `201 {"data": {"email_template_id": "<uuid>"}}` |
| `POST /{id}` | `emailtemplate.update` | `{"message": "ok"}` |
| `DELETE /{id}` | `emailtemplate.delete` | `{"message": "ok"}` (soft delete) |

- `get` is the catalog's name. Laravel's policy checked `emailtemplate.view`, which the catalog doesn't offer, so no team could grant it.
- An unknown, malformed, deleted or other workspace's id is a 404.

## Body

| Field | Create | Update |
|---|---|---|
| `template_name` | required string ≤ 255 | string ≤ 255; null or left out keeps it |
| `template_domain` | required; one of Laravel's 84 `Domains` (`invoice`, `estimate`, `order`, …), any case. Error: `The selected template domain: {value} is invalid.` | ignored: the domain never changes |
| `default_template` | required boolean | boolean; null or left out keeps it |
| `subject` | required string ≤ 255 | string ≤ 255; null or left out keeps it |
| `body` | required string ≤ 25000 | **required** string ≤ 25000 (Laravel's update rules) |
| `conditions` | object: `language` (≤ 3), `relation_type` (`business` or `personal`), `relation_custom_type_id` (a live relation custom type) | same; null clears, left out keeps |
| `meta` | object, ≤ 1000 JSON nodes / 16 KiB | same; null clears, left out keeps |

- Strings are trimmed and a blank one counts as not sent.
- **One default per domain.** A template saved with `default_template: true` clears the flag on the domain's other templates, in the same transaction, under a lock per workspace and domain. A partial unique index backs it. Laravel did this only on create, so updates could leave several defaults.
- Template attachments (Laravel's file associations) aren't ported.

## Template resource

```json
{
  "id": "…", "template_name": "Invoice NL", "template_domain": "invoice", "default_template": true,
  "subject": "Invoice {invoice.invoice_nr}", "body": "<p>…</p>",
  "conditions": { "language": "nl", "relation_type": "business", "relation_custom_type_id": null },
  "created_at": "…", "last_updated_at": "…", "meta": null
}
```

## List filters

`page`, `page_size`, plus:

| Query | Matches |
|---|---|
| `template_domain` | that domain (validated like the body) |
| `q` | name or subject, case-insensitive substring (Laravel: Meilisearch) |
| `language`, `relation_type`, `relation_custom_type_id` | templates whose condition equals the value **or is unset** |

## Variables `GET /variables/{domain}`

Laravel's variable lists, generated from the PHP source with the PDF template fields (`tests/fixtures/pdf_template_fields_oracle.php`) and served as written, key order kept:

```json
{ "data": { "variables": { "model": { "invoice.invoice_nr": "invoice number", … }, "relation": { … } } } }
```

Domains with lists: `invoice`, `estimate`, `order`, `purchase_order`, `collective_invoice`, `work_order`, `cash_register`. Any other domain gives `{"data": {"variables": []}}`. Laravel also listed `leave_request` and the order's custom fields; neither is ported.

Placeholders are filled when a mail is sent (F5b2).
