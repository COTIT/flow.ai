# Workspace mail API

Sending a workspace's mail and its logs. Ported from vimdesk-api `src/Infrastructure/Actions/{SendMessage,SendMail,PrepareEmailSendLog}.php`, `SendMessageRequest`, `BlockDisallowedMailRecipients`, the Office 365 test controllers and `ListEmailSendLog`.

Every route sits under `/v1/workspace/{workspace}`.

## Routes

| Route | Permission | Answer |
|---|---|---|
| `POST /send-message` | `workspace.send-message` | `{"message": "ok"}` |
| `GET /settings/email-logs` | `setting.get` | paginated send logs, newest first |
| `POST /settings/office365/test-connection` | `setting.update` | `{data: {ok, stage, message, …}}`, 200 or 422 |
| `POST /settings/office365/test-send` | `setting.update` | same |
| `GET /email-profiles/{id}/test-connection` | `emailprofile.update` | `{"data": {"success_connection": true}}` |

- `setting.get`: Laravel checked `setting.view`, which the catalog doesn't offer.
- `emailprofile.update`: Laravel checked `ftpserver.view` here (a bug).

## Send `POST /send-message`

Body up to 1 MiB.

| Field | Rule |
|---|---|
| `addressed_contact_id` | required uuid; a live contact of this workspace (`The selected addressed contact id is invalid.`; Laravel's check wasn't workspace-scoped) |
| `address` | required address object (the relation address rules; errors use bare keys such as `city`) |
| `send_method` | required, `email` or `manual`, any case (`The selected send method: {value} is invalid.`) |
| `subject` | required with `email`, string ≤ 100 |
| `body` | required with `email`, HTML string |
| `to`, `cc`, `bcc` | lists of email addresses (`The to.0 must be a valid email address.`), lowercased and de-duplicated |
| `attachments` | list of stored file ids of this workspace (`The selected attachments.0 is invalid.`) |
| `email_profile_id` | a live email profile of this workspace (`The selected email profile id is invalid.`) |
| `template_id` | accepted and ignored, as in Laravel |
| `pdf_template_id` | uuid (`The selected pdf template id is invalid.`); ignored here, used by invoice sends (`invoices.md`) |

- `manual`: nothing is sent; `{"message": "ok"}`. Laravel answered 500.
- Without `to`, the addressed contact's email gets it. vimdesk-web's mail dialog sends no `to`, which was a 500 in Laravel.
- No recipient at all: nothing is sent and nothing logged (Laravel).

### How it leaves

In Laravel SendMail's order:

1. **Microsoft Graph** when settings `integrations.mail.outbound.send_enabled` is true and `integrations.mail.graph` has `tenant_id`, `client_id`, `mailbox` and a stored `client_secret`. With send enabled but Graph incomplete, a warning is logged and SMTP is used.
2. Else **the profile's SMTP server** when the profile has an `smtp.host` (its sealed password opened).
3. Else **the workspace's SMTP server** (settings `smtp`, its sealed password opened).
4. Else `422 {"message": "No smtp host specified in the settings"}`.

From:

| Case | Address | Name |
|---|---|---|
| With a profile | the profile's `email` | its `sender_name` |
| Workspace SMTP | settings `smtp.from_address` (else `422 {"message": "No smtp from address specified in the settings"}`) | the workspace profile's `company_name` |
| Graph, no profile | `outbound.senders[type]`, else `outbound.default_sender`, else the Graph mailbox | its `sender_name` |

The HTML layout wraps the body: a "Control 360°" header, the profile's `html` as footer, and a copyright line. Attachments come from the bucket.

### Workspace SMTP servers

- The host is resolved by the API, and the server is refused unless **every** address is public: `422 {"message": "The SMTP server isn't at a public address."}`. Loopback, private, link-local, CGNAT and metadata addresses all count as not public.
- The port must be 25, 465, 587 or 2525: `422 {"message": "The SMTP port must be 25, 465, 587 or 2525."}`. Without a port: 465 for `ssl`, else 587.
- The connection goes to the vetted address, while TLS checks the certificate against the host name, so DNS can't swap the target in between.
- The first vetted address that accepts a TCP connection within 5 s gets the mail; an SMTP session is opened once, so a mail is never handed to a second address.
- Encryption:
  - `ssl`: TLS from the start.
  - `tls`: STARTTLS required.
  - Unset: STARTTLS when the server offers it, and required as soon as a username is set (a password never travels in clear).
- Certificates are verified (Laravel set `verify_peer=false`). An invalid one: `422 {"message": "The SMTP server's TLS certificate isn't valid for its host name."}`.
- A refused login (535): `422 {"message": "The SMTP server refused the username or password."}`.
- Hosts in `MAIL_TRUSTED_SMTP_HOSTS` (a dev mail catcher) skip the address and port checks.

### Microsoft Graph

- The token comes from the client-credentials grant at `login.microsoftonline.com/{tenant}/oauth2/v2.0/token`. It is cached per tenant, client and secret until a minute before it expires. `tenant_id` must be letters, digits, `-` and `.`.
- The mail goes as `POST graph.microsoft.com/v1.0/users/{from}/sendMail`, with an HTML body, To/Cc/Bcc, file attachments as base64, `X-Genesy-Email-Log` and `saveToSentItems: true`.
- Calls go through the outbound HTTP client (no redirects, SSRF guard). `GRAPH_LOGIN_URL` / `GRAPH_API_URL` replace the hosts for local fakes only.
- A token refused (400/401): `422 {"message": "Could not authenticate. Check the tenant, client id and client secret."}`.
- `sendMail` refused (400/401/403/404): `422 {"message": "Microsoft Graph refused the mail: check that the sender is a mailbox the app may send as."}`.
- 429 and 5xx from either: the 503 below.

### Limits

- Attachments together: at most 20 MB over SMTP and 2 MB over Graph (its whole request is limited to 4 MB, and base64 attachments plus the body must fit). Error: `The attachments must not be greater than {N} kilobytes in total.`
- One send (resolve, connect, TLS and the transfer; or the token and the request) is cut off after 15 s. The mail may or may not have left, so the log says `unknown` and the answer is `503 {"message": "The mail server didn't answer in time: check the send log before sending again."}`.
- Any other server or service failure: `503 {"message": "Failed to send message."}`, logged `failed`.
- Once a mail has gone out, a failure to update its log row doesn't fail the request. The row stays open until the sweep marks it `unknown`.

## The recipient allow-list

`MAIL_ALLOWED_RECIPIENT_DOMAINS` (a comma list, any case; empty allows all) applies in one place every mail goes through: platform mail (signup, invites), workspace SMTP, Graph and the Office 365 test send. It covers To, Cc and Bcc.

- A domain matches exactly. Subdomains and look-alikes don't: `mail.cotit.eu` is not `cotit.eu`.
- One recipient outside the list blocks the whole mail; so does a mail without recipients once a list is set.
- A blocked mail sends nothing. The caller still gets `{"message": "ok"}`, as in Laravel, and the log row says `blocked`. Laravel left its row stuck in `sending`.
- The warning log names the blocked domains and the recipient count only.

Prod holds demo data, so it is set there (`cotit.eu,genesy.app`).

## Send logs

Each send that reaches a transport commits an `email_send_logs` row as `pending` before it is sent, plus one `email_send_log_recipients` row per address. The row then moves:

| Status | When |
|---|---|
| `sending` | right before the transport |
| `accepted` | the server or Graph took it (`sent_at`, the SMTP `Message-ID`) |
| `failed` | refused or failed; `error_message` is a short class (`smtp: permanent 550`, `graph: sendMail 503`) or the 422 text, never the server's own words |
| `blocked` | the allow-list stopped it |
| `unknown` | the 15 s limit ran out mid-send (`error_message` `timed out`), or the row is still `pending` or `sending` 10 minutes on, closed by a sweep that runs every 5 minutes. Such a mail is never sent again; a late outcome still updates it |

A mail about a document (an invoice send) also stores `entity_type` and `entity_id`, for that document's mail history (F20).

A finished row (`accepted`, `failed`, `blocked`) never moves back. Provider events (Resend, F20) move accepted rows on to `delivered`, `bounced`, `opened`, `clicked` and `spam_complaint`.

`GET /settings/email-logs` (`page`, `page_size`) rows:

```json
{ "id": "…", "subject": "…", "status": "accepted", "provider": "smtp", "mailer": "<workspace id>",
  "recipient_count": 3, "opened_count": 0,
  "to_recipients": ["…"], "cc_recipients": [], "bcc_recipients": [],
  "recipients_summary": "a@…, b@…", "sent_at": "…", "delivered_at": null, "bounced_at": null,
  "opened_at": null, "created_at": "…" }
```

`provider` is `smtp` or `microsoft-graph` (Laravel also had `postmark`).

Application logs never carry a recipient address, a subject, a body or a credential: domains, counts, the log id and the error class only.

## Connection tests

### `GET /email-profiles/{id}/test-connection`

Sends "Test email profile connection" (subject and body) to the caller's address through the profile, with the full pipeline, logs included. A 404 for an unknown profile.

### `POST /settings/office365/test-connection` and `test-send`

| Field | Rule |
|---|---|
| `graph.tenant_id`, `graph.client_id` | required string ≤ 255 |
| `graph.client_secret` | required string ≤ 1000; the mask `********` uses the stored secret (422 `Enter this credential again.` when none is stored) |
| `graph.mailbox` | required email |
| `from` | test-send only: email; else the mailbox |
| `to` | test-send only: required email |

Answers are always `{data: {...}}`, Laravel's texts:

| Case | Status | `data` |
|---|---|---|
| no token | 422 | `ok: false, stage: "token", message: "Could not authenticate. …"` |
| connection: the inbox can't be read | 422 | `ok: false, stage: "mailbox", message: "Authenticated, but could not read the mailbox. …", reason: "Graph answered 403."` |
| connection: ok | 200 | `ok: true, stage: "mailbox", message: "Connection successful. …", messages_with_attachments: 0 or 1` (the inbox's first message, as Laravel) |
| send: failed | 422 | `ok: false, stage: "send", message: "Authenticated, but sending failed. …"` |
| send: allow-list | 422 | `ok: false, stage: "send", message: "This server may not send to that address's domain."` |
| send: ok | 200 | `ok: true, stage: "send", message: "Test e-mail sent to {to} from {from}. …"` |

- The test mail is Laravel's ("Vimdesk test e-mail") and isn't logged.
- Laravel's test send skipped the allow-list; this one doesn't. A blocked test send answers 422 with the reason, not ok: it is an admin's check.

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `MAIL_ALLOWED_RECIPIENT_DOMAINS` | empty (all) | the allow-list above |
| `MAIL_TRUSTED_SMTP_HOSTS` | empty | workspace SMTP hosts dialled without the address and port checks |
| `GRAPH_LOGIN_URL`, `GRAPH_API_URL` | Microsoft's | local fakes only; an overridden host is trusted by the outbound client |
