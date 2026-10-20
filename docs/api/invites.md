# Invites API

Base path `/v1`. Shared shapes (422 body, 401, 500, token body) are in `auth.md`.

An invite code is 64 hex chars, sent only in the email link. The DB stores its sha256. Invites expire 7 days after the latest send.

404 body for every "not found" case: `{"message": "The requested resource could not be found."}`.

## POST /v1/workspace/{workspace}/invites

Requires a Bearer token. Only the workspace owner may invite (roles and teams aren't ported yet).

| Field | Rules |
|---|---|
| `email` | required, valid email, max 255 |
| `team_id` | accepted and ignored |

| Status | When |
|---|---|
| `201` | `{"data": {"invite_id": "<uuid>"}}`. Email sent to `email` with `{FRONTEND_URL}/accept-invitation?code=<code>`. |
| `401` | no or bad token |
| `403` | caller is a member but not the owner: `This action is unauthorized.` |
| `404` | unknown, deleted or malformed workspace id, or the caller isn't a member (so outsiders can't tell the workspace exists) |
| `422` | email invalid, or already a member: `errors.email = ["user already exists in the workspace"]` |
| `429` | `{"message": "Too Many Attempts."}`: caller sent `INVITES_PER_HOUR` invite emails (default 20) in the last hour. Re-sends and failed SMTP attempts count. |
| `500` | SMTP failed. The invite row exists, and sending again rotates the code. |

Inviting the same email again while the invite is pending reuses the row and the same `invite_id`, issues a new code and extends the expiry. The old link stops working, even for a request already in flight with it.

`email` must also parse as a deliverable address (lettre `Address`), so values like `a"b@x.co` are rejected before any row is written.

## GET /v1/invites/get-invite-status/{code}

Public.

```json
{ "data": { "id": "<uuid>", "status": "pending", "new_user": true,
            "workspace_name": "acme", "email": "new@example.com",
            "invited_on": "2026-10-20T06:47:35.174246Z" } }
```

- `status`: `pending`, `accepted`, `denied`, or `expired` (a pending invite past its expiry).
- `new_user`: true when no account uses the invited email. vimdesk-web shows the signup form when it's true, and otherwise sends the user to login.
- Unknown or malformed code: `404`.

## POST /v1/register

Public. Creates an account through an invite. Same person fields and rules as `create-workspace` (`first_name`, `middle_name?`, `last_name`, `email`, `password`), plus `invite_code` (required).

| Status | When |
|---|---|
| `201` | token body, `workspace_ids` = the invited workspace. Invite becomes `accepted`. |
| `404` | unknown code, or `email` differs from the invited email (case-insensitive) |
| `422` | field rules; `errors.invite_code` = `Workspace invite has expired` or `This invitation has already been used.`; `errors.email` = `The email has already been taken.` (that person should log in and accept instead) |

The user, membership, invite answer and token are written in one transaction. Two parallel registers on one code give one `201` and one `422`.

## POST /v1/invites

Requires a Bearer token. The logged-in user answers an invite sent to their own email.

| Field | Rules |
|---|---|
| `accept` | required boolean |
| `invite_code` | required |

| Status | When |
|---|---|
| `200` | `{"message": "ok"}`. Accept joins the workspace and decline only records the answer. Accepting an already accepted invite returns `200` only while the user is still a member (Laravel COT-241). It never re-adds a removed member. |
| `401` | no or bad token |
| `404` | unknown code, or the invite is for a different email |
| `422` | expired, or already answered (for example accept after decline) |

## Laravel differences

- The code is random hex and stored hashed. Laravel used a raw UUID plus an `APP_KEY`-encrypted link value.
- The link base comes from `FRONTEND_URL`. Laravel took it from the request `Origin` header, which a caller can spoof.
- Existing users get the same `/accept-invitation` link. Laravel sent them to `/pending-invites`, which needs `/me` (not ported yet).
- Expiry is an exact timestamp. Laravel cut it at midnight because the column was cast to a date.
