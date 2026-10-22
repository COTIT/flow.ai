# Email profiles API

A sender: name, address, footer HTML and optionally its own SMTP server. Ported from vimdesk-api `src/Domain/WorkspaceSetting/EmailProfile` (Store/UpdateEmailProfileRequest, EmailProfileResource, ListEmailProfileResource, ListEmailProfile).

Shared rules (guard order, 404s, list `meta`) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}/email-profiles`.

## Routes

| Route | Permission | Answer |
|---|---|---|
| `GET /` | `emailprofile.list` | paginated profiles, oldest first |
| `GET /{id}` | `emailprofile.get` | `{data: profile}` |
| `POST /` | `emailprofile.create` | `201 {"data": {"email_profile_id": "<uuid>"}}` |
| `POST /{id}` | `emailprofile.update` | `{"message": "ok"}` |
| `DELETE /{id}` | `emailprofile.delete` | `{"message": "ok"}` (soft delete; the sealed password is wiped) |

- `get` is the catalog's name. Laravel's policy checked `emailprofile.view`, which no team could grant.
- `GET /{id}/test-connection` (`emailprofile.update`) sends a test mail to the caller through the profile: see `messages.md`.

## Body (create and update: the same rules)

| Field | Rule |
|---|---|
| `name` | required string ≤ 80 |
| `sender_name` | required string ≤ 80 |
| `email` | required email ≤ 255 |
| `default` | required boolean. A plain flag: nothing keeps it to one profile (as Laravel) |
| `html` | footer HTML, string ≤ 25000; null clears, left out keeps |
| `smtp` | object or null; null clears the server and its password, left out keeps both |
| `smtp.host` | a host name (`The smtp.host must be a valid host name.`) or a public IP literal (`The smtp.host must be a public address.` for loopback, private, link-local, metadata and the like), ≤ 255; stored lowercase without a trailing dot. Required when a new password is sent (`The smtp.host field is required when smtp.password is present.`) |
| `smtp.username` | string ≤ 255 |
| `smtp.port` | integer 1..65535 |
| `smtp.encryption` | `ssl` or `tls` (`Invalid smtp.encryption encryption type.`) |
| `smtp.bcc_all_emails` | boolean (stored; nothing reads it yet, as in Laravel) |
| `smtp.password` | string ≤ 255, or the mask `********`, see below |
| `conditions` | as on email templates (`language`, `relation_type`, `relation_custom_type_id`); null clears, left out keeps |

- `smtp.from_name` and `smtp.from_address` are dropped, as Laravel's profile rules did.
- `meta` is skipped. Laravel's table had no `meta` column, so sending it there was a 500.

### The SMTP password

The password never sits in `smtp`. It is sealed with XChaCha20-Poly1305 under the server's credentials key (`CREDENTIALS_KEY`, as in `settings.md`), bound to the workspace and to `email_profiles.{id}.smtp.password`, and stored with the key's id.

| Sent | Effect |
|---|---|
| a new string | sealed and stored (422 `Credential storage isn't configured.` without a key) |
| `********` | keeps the stored one, only while one is stored and `host`, `port`, `username` and `encryption` are as stored; else 422 `Enter this credential again.` |
| null, blank or left out of `smtp` | cleared |

The mask check runs under the profile's row lock. A mask on create is always 422.

## Profile resource (`GET /{id}`)

```json
{
  "id": "…", "name": "Sales", "sender_name": "Acme Sales", "email": "sales@acme.test",
  "html": "<p>Kind regards</p>", "default": true,
  "smtp": { "host": "smtp.acme.test", "username": "sales", "password": "********", "port": 587,
            "encryption": "tls", "from_name": null, "from_address": null, "bcc_all_emails": true },
  "conditions": { "language": "nl", "relation_type": null, "relation_custom_type_id": null },
  "meta": null, "updated_at": "…", "created_at": "…"
}
```

- `smtp.password` is the mask when one is stored, else null. Laravel printed it in plaintext.
- Without a server, every `smtp` key is null.

## List

Rows: `id, name, sender_name, email, default, html, meta (null), updated_at, created_at`; no `smtp`. Laravel's list printed `html` as `{"host": null}`.

Filters: `page`, `page_size`, `language`, `relation_type`, `relation_custom_type_id` (equal or unset).
