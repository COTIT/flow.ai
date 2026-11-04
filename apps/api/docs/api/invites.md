# Invites API

Base path `/v1`. Shared shapes (422 body, 401, 500, token body) are in `auth.md`.

An invite code is 64 hex chars, sent only in the email link. The DB stores its sha256. Invites expire 7 days after the latest send.

404 body for every "not found" case: `{"message": "The requested resource could not be found."}`.

## POST /v1/workspace/{workspace}/invites

Requires a Bearer token and the `invite.create` permission (owner and `workspace_admin` always pass; see `permissions.md`). The caller's email must be verified.

| Field | Rules |
|---|---|
| `email` | required, valid email, max 255 |
| `team_id` | optional: null or blank (no team), or the id of a live team in this workspace. Accepting the invite adds the user to that team. A team holding `workspace_admin` needs an inviter who is the owner or holds `workspace_admin` (403 otherwise). |

| Status | When |
|---|---|
| `201` | `{"data": {"invite_id": "<uuid>"}}`. Email sent to `email` with `{FRONTEND_URL}/accept-invitation?code=<code>`. |
| `401` | no or bad token |
| `403` | membership inactive, no `invite.create`, or `team_id` is an admin team and the caller is no admin: `This action is unauthorized.` Or the caller's email isn't verified yet: `Verify your email address first.` |
| `404` | unknown, deleted or malformed workspace id, or the caller isn't a member (so outsiders can't tell the workspace exists) |
| `422` | email invalid, or already a member: `errors.email = ["user already exists in the workspace"]`. `team_id` not a uuid, from another workspace, or deleted: `errors.team_id = ["The team id is an invalid id for the team domain."]` |
| `429` | `{"message": "Too Many Attempts."}`: caller sent `INVITES_PER_HOUR` invite emails (default 20) in the last hour. Re-sends and failed SMTP attempts count. |
| `500` | SMTP failed. Nothing is saved: a new invite is not made, and a re-invite keeps the code mailed before. Sending again is safe. |

Inviting the same email again while the invite is pending reuses the row and the same `invite_id`, issues a new code, extends the expiry and takes the new `team_id`. The old link stops working, even for a request already in flight with it.

Accept (`POST /v1/register` or `POST /v1/invites` with `accept: true`) joins the workspace and, in the same transaction, the invite's team. A team deleted after the invite was sent is skipped; the user still joins the workspace.

`email` must also parse as a deliverable address (lettre `Address`), so values like `a"b@x.co` are rejected before any row is written.

## GET /v1/workspace/{workspace}/invites

Needs `invite.list`. Used by the pending invites modal: `?filter[invite_status]=pending`. Query params: `filter[invite_status]` (`pending` | `accepted` | `denied`; absent = all), `page`, `page_size` (default 50, max 100). Unknown params are ignored.

```json
{ "data": [ { "invite_id": "<uuid>", "email": "new@example.com", "invite_status": "pending",
              "invited_by": "owner@example.com", "expired_at": "…", "meta": null,
              "created_at": "…", "last_updated_at": "…", "relationships": { "team": null } } ],
  "meta": { "current_page": 1, "from": 1, "last_page": 1, "per_page": 50, "to": 1, "total": 1 } }
```

- Invites are listed oldest first. An expired pending invite still shows as `pending`; `expired_at` tells.
- `relationships.team` holds the live team (TeamResource, users not loaded) or null.
- Laravel also sent `invite_code`. Here only its hash is stored, so the link goes to the invitee alone and the code is never echoed.

## DELETE /v1/workspace/{workspace}/invites/{invite}

Needs `invite.delete`. Deletes the invite (any status) and returns `{"message": "ok"}`. Its emailed link stops working. An unknown or malformed id, or an invite of another workspace, is 404.

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
| `invite_code` | required. Either the emailed code (64 hex), or the invite id from `GET /v1/me` `pending_invites[].invite_code`. The id works only when the caller's email is verified. |

| Status | When |
|---|---|
| `200` | `{"message": "ok"}`. Accept joins the workspace and decline only records the answer. Accepting an already accepted invite returns `200` only while the user is still a member (Laravel COT-241). It never re-adds a removed member. |
| `401` | no or bad token |
| `403` | invite id used by a caller whose email isn't verified: `Verify your email address first.` |
| `404` | unknown code or id, or the invite is for a different email |
| `422` | expired, or already answered (for example accept after decline) |

## Laravel differences

- The code is random hex and stored hashed. Laravel used a raw UUID plus an `APP_KEY`-encrypted link value.
- The link base comes from `FRONTEND_URL`. Laravel took it from the request `Origin` header, which a caller can spoof.
- Existing users get the same `/accept-invitation` link. vimdesk-web sends them on to `/pending-invites`, which lists invites from `GET /v1/me` and accepts them by invite id (verified email required). Laravel sent an `APP_KEY`-encrypted code there instead.
- A decline is stored and reported as `denied`, the word vimdesk-web checks.
- Expiry is an exact timestamp. Laravel cut it at midnight because the column was cast to a date.
