# Farmer logins

Farmers sign in with the phone their officer registered and a temporary password the officer issues. They must choose their own password at first sign-in. No email, no SMS, no third party. Code: `src/modules/farm_logins/`, migration `farm_logins`.

## Accounts

- A farmer's account is a `users` row with `phone` (`+255…`) and **no email**. `users.email` is nullable now; every account has an email, a phone, or both (`users_email_or_phone`). One account per phone (`users_phone_unique`).
- `users.must_change_password` is set when an officer issues a temporary password, and cleared only by a real change.
- The account is linked to its person (`farm_persons.user_id`) and gets a `farmer` role in the person's village.
- **Phone-only accounts reach farm routes and nothing else.**
  - The account routes (`GET /v1/me`, `GET /v1/me/workspaces`, `POST /v1/invites`, `POST /v1/email/verification-notification`) answer `403`.
  - Workspace routes answer `404` because farmers are not workspace members. That includes products, invoices, relations and invites, both listing and creating.
  - `POST /v1/logout` works.
  - Workspace create is the email signup, so a phone can't create one (`422` on the email).
- `UserResource.email`, `UserEmailResource.email` and `InviteResource.invited_by` are nullable in the spec, since an account may have no email. Workspace members always have one in practice: farmers never become members.

## Issue or reset: `POST /v1/workspace/{workspace}/farm/persons/{person}/login`

Field staff of the person's village. A farmer gets `403`, another village or an unknown person `404`, no token `401`.

- First call: creates the account (name from the person, phone from the person, an 8-character temporary password), links it, grants the farmer role. `kind: "initial"`.
- Later calls: a new temporary password, `must_change_password` set again, and **every session of the farmer ended**. The sign-in lock on that phone is cleared. `kind: "reset"`.
- Reply `201 {"data": {phone, temp_password, kind}}`. The password is in this reply and nowhere else; only its argon2 hash is stored.
- Refusals (`422 {"message"}`, shown as written):
  - `add a phone number before issuing a login`
  - `this phone number already has an app login`: another account, in any workspace, signs in with this phone.
  - `this person signs in as staff: their password cannot be reset here`: the linked account has an email or a staff role.
- Every issue is in `farm_login_issues`, which is append-only, with the officer.

## History: `GET /v1/workspace/{workspace}/farm/persons/{person}/login-history`

Field staff of the person's village. Newest first, at most 100: `[{issued_at, kind, issued_by {id, name}, must_change_password}]`.

## Sign in: `POST /v1/farm/login`

Public, behind the IP rate limit.

```json
{ "phone": "0712 345 678", "password": "…" }
```

- Any spelling of the phone works; it is normalised first.
- `200 {"data": {access_token, token_type: "Bearer", must_change_password, memberships: [{workspace_id, role, village_id}]}}`. The token is an ordinary bearer token.
- An unknown phone, junk in the phone field, or a wrong password all get the same `401` body, with the same argon2 work.
- **Lockout:** 10 attempts on one phone in 15 minutes, then `429` even with the right password. Unknown phones lock the same way. An officer's reset clears the lock.
- Email accounts don't sign in here.

## Choose a password: `POST /v1/farm/password`

The signed-in phone account. Email accounts get `403`.

```json
{ "current_password": "…", "password": "…" }
```

- `current_password` must match (`422` on `current_password`).
- The new password may not be the temporary one (`choose a new password first`).
- The farmer rule (NIST 800-63B style, no composition rule):
  - 8 to 128 characters;
  - not one character repeated;
  - not a straight run (`12345678`, `abcdefgh`, `87654321`);
  - not an obvious word (`password`, `nenosiri`, …);
  - not containing the last 7 digits of the farmer's phone.
- `204`; `must_change_password` is cleared. Other sessions stay signed in.
- Staff passwords keep the staff rule (`auth::password::violations`).

## The signed-in account: `GET /v1/farm/me`

For phone (farmer) and email (staff) tokens alike. No token: `401`.

```json
{ "data": {
  "user": { "id": "uuid", "name": "Neema Mwakalinga", "email": null, "phone": "+255712000001" },
  "locale": "sw",
  "must_change_password": false,
  "memberships": [ { "id": "uuid", "workspace_id": "uuid", "workspace_name": "ruaha", "display_name": "Ruaha Coffee Co-op",
                     "role": "farmer", "village_id": "uuid", "village_name": "Ilundo", "person_id": "uuid" } ]
} }
```

- `memberships` holds the caller's own **live** farm roles across all workspaces; revoked ones are left out.
- A staff role (field officer, ops, admin) is listed only while the caller is an active member of that workspace, the same rule as the farm guard.
- `person_id` is the caller's registered person in that workspace, or `null`.
- `locale` is `sw`, `en` or `nl`. Farmer accounts start with `sw`, others with `en`.
- The farmer sign-in reply (`POST /v1/farm/login`) carries the same `memberships` shape.

## Choose the language: `PATCH /v1/farm/me`

`{ "locale": "sw" | "en" | "nl" }` → `204`. Any other value, or any other field: `422`.
