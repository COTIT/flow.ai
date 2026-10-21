# Auth API

Base path `/v1`. Paths and success bodies match vimdesk-api, so vimdesk-web can switch by changing `VITE_API_URL`.

## Shared shapes

Success: `{"data": ...}`.

Validation failure, `422`:

```json
{
  "message": "The email field is required. (and 1 more error)",
  "errors": { "email": ["The email field is required."], "password": ["..."] }
}
```

- `message` is the first error plus a count of the rest. Fields are sorted by name, so the "first" error can differ from Laravel, which used rule order.
- A body that isn't valid JSON, or has a wrong type, gets `errors.body`.

Unauthenticated, `401`: `{"message": "You are not authorized to perform this action."}`.

Server error, `500`: `{"message": "Server error."}`. The cause is logged and never returned.

Rate limited, `429`: `{"message": "Too Many Attempts."}` plus a `Retry-After` header in seconds. Applies to the public routes (`create-workspace`, `login`, `register`, `invites/get-invite-status/{code}`, `verify-email/{token}`), which share one budget per client IP (IPv6: per /64 block, since one customer usually gets a whole /64): `RATE_LIMIT_BURST` requests at once (default 10), then one more every `RATE_LIMIT_REPLENISH_SECS` (default 6). Routes behind a Bearer token have their own limits:

- **Per session:** each bearer token has its own budget, `PRIVATE_RATE_LIMIT_BURST` requests at once (default 120), then one more every `PRIVATE_RATE_LIMIT_REPLENISH_MS` (default 50, so 20 a second). The same `429` body and `Retry-After`. The key is the token's sha256, read exactly as authentication reads it (scheme in any case, surrounding spaces trimmed), so colleagues behind one office IP don't share a budget and one token can't spread over several. A missing or malformed token is counted per client IP.
- **Per client IP:** around that, every IP has a ceiling of `PRIVATE_IP_RATE_LIMIT_BURST` (default 600), then one more every `PRIVATE_IP_RATE_LIMIT_REPLENISH_MS` (default 5, so 200 a second). Made-up tokens each get a fresh session budget, never a fresh IP one. Behind Traefik this needs `TRUST_PROXY_HEADERS` (set by `deploy.sh`), or every client shares the proxy's IP.
- Invite send has its own hourly limit (see `invites.md`).
- **In flight, per workspace and per user:** after the membership check, a `/v1/workspace/{id}/…` request takes one of the workspace's `WORKSPACE_MAX_INFLIGHT` slots (default half the request pool, `DATABASE_MAX_CONNECTIONS` / 2, at least 1) and one of the user's `USER_MAX_INFLIGHT` (default a quarter of the pool, at least 2). Requests beyond that wait in order for up to 5 seconds, then answer `503 {"message": "The server is busy, please try again."}` with `Retry-After: 5`. One tenant's or one user's slow requests can't hold every database connection, and only members take a workspace's slots (an outsider gets its 404 first). Routes outside a workspace (`/v1/me`, users, logout) are only rate-limited.

Every `/v1` request also has time limits:

- **Body deadline:** the whole request body must arrive within `REQUEST_BODY_DEADLINE_SECS` (default 20), else `408 {"message": "The request body took too long to arrive."}`. A client trickling its body can't hold a slot.
- **Request timeout:** a request must finish within `REQUEST_TIMEOUT_SECS` (default 45, above the 30s statement, 5s pool and 5s slot waits), else `503 {"message": "The server is busy, please try again."}` with `Retry-After: 5`. The request is cancelled: its slots and pooled connection go back and an open transaction rolls back. A write that committed just before the deadline still answers 503, so a client retrying a create may make it twice (idempotency keys come with the POS sub-batches).

Body too large, `413`: `{"message": "The request body is too large."}`. Limit 64 KiB.

Every `/v1` response sets `Cache-Control: no-store` and `X-Content-Type-Options: nosniff`.

Token body (signup, login, invite register):

```json
{ "data": { "access_token": "<64 hex chars>", "token_type": "Bearer", "workspace_ids": ["<uuid>"] } }
```

Send it back as `Authorization: Bearer <access_token>`. Tokens expire after `TOKEN_TTL_DAYS`, default 30. Laravel tokens never expired.

## POST /v1/create-workspace

Creates a user, a workspace owned by that user, the membership and a token, all in one transaction.

| Field | Rules |
|---|---|
| `first_name` | required, trimmed, max 55 chars |
| `middle_name` | optional, blank = null, max 55 |
| `last_name` | required, max 55 |
| `email` | required, valid email, max 255, unique ignoring case |
| `password` | required, min 8, max 255, upper + lower case, a letter, a number, a symbol. Every failed rule is listed. Not trimmed. |
| `workspace_name` | required, max 55, lowercase letters/numbers/single dashes, unique |
| `workspace_type` | optional, `retail` (default) or `manufacturing` |

Unknown fields (for example `meta`) are ignored, and `null` counts as missing.

| Status | When |
|---|---|
| `201` | token body, `workspace_ids` = the new workspace |
| `422` | any rule above fails. Taken values: `The email has already been taken.`, `The workspace name has already been taken.`, reported together. |

Laravel returned `200` here. vimdesk-web accepts both (`handleAuthResponse`).

## POST /v1/login

| Field | Rules |
|---|---|
| `email` | required, valid email. Matched ignoring case and surrounding spaces. |
| `password` | required, max 255 (longer can't match any stored password, so it's rejected before hashing). No strength rules, they apply only when a password is set. Not trimmed. |

| Status | Body |
|---|---|
| `200` | token body. `workspace_ids` = every live workspace where the user is an active member, oldest membership first. |
| `401` | `{"message": "Invalid login details"}` for an unknown email, a wrong password or a deleted user. Same body and similar timing in each case (unknown emails still run one argon2 verify). |
| `403` | `{"message": "No workspace found associated with your account"}`. Checked only after the password matched, and no token is issued. |
| `429` | `{"message": "Too Many Attempts."}`: 10 failed logins for this email (any case) in the last 15 minutes, from any IP. Unknown emails lock the same way, so a 429 doesn't reveal whether an account exists. Checked before any password hashing. A successful login clears the count. |
| `422` | missing field or bad email format |

Laravel differences: the `401` message sits at the top level instead of `data.message`; vimdesk-web reads both. The "no workspace" case is `403` (Laravel sent a malformed `422`).

## POST /v1/logout

Requires `Authorization: Bearer <token>`. Deletes the token used for this request, and the user's other sessions stay valid.

| Status | Body |
|---|---|
| `200` | `{"message": "ok"}` |
| `401` | missing, unknown, expired or already revoked token |

## Email verification

Signup (`create-workspace`) emails `{API_PUBLIC_URL}/v1/verify-email/{token}`. The token is 64 hex chars, stored as sha256 and valid 24 hours. It can be opened more than once: mail scanners (Outlook SafeLinks and the like) open links before the user does, so a second open still counts as success. If that mail fails, signup still returns `201`, and the user can ask for a resend.

### GET /v1/verify-email/{token}

Public and rate-limited, opened by a browser from the email. It always answers `303 See Other`:

- valid (also on later opens): `{FRONTEND_URL}/login?email_verified=1`. The **first** verification logs out every session of that account. Someone who signed up with this address before its owner clicked loses access that way.
- expired, unknown, or the user was deleted: `{FRONTEND_URL}/login?email_verified=0`

### POST /v1/email/verification-notification

Bearer. Sends a new link and returns `200 {"message":"ok"}`. If the email is already verified, it sends nothing and still returns `200`. Limits per user, counting the signup mail: one link per 60 seconds, 5 per hour, 10 per day, then `429`. A send that fails doesn't count against the limit.

Other ways an email becomes verified: signing up through an invite (`POST /v1/register`), or answering an invite with the emailed code (`POST /v1/invites`). Either one proves the user reads that inbox.

