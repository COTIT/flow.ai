# Production

Two Supabase projects, two databases, no shared rows:

| | Dev / demo | Production |
|---|---|---|
| Supabase project | the one in `.env` | `hwjwpeaadfdvjdkzkqje` |
| Data | demo seed, e2e records | real; nothing until staff are granted |
| Local tooling | `pnpm db:*` | `pnpm db:prod:*` (reads `.env.prod`) |
| Netlify context | Deploy previews + Branch deploys | Production |
| Banner / resources | `VITE_DATA_MODE=demo`, `VITE_FEATURE_RESOURCES=on` | both unset |

**Never against production:** `seed.sql`, `seed_surveys.sql`,
`seed_resources.sql`, `pnpm e2e`, `pnpm db:rls`. The seeds refuse once the
`ILUNDO` project exists; e2e and the RLS suite only ever read `.env`, which
stays pointed at dev. CI is dev-only too.

## Local access: `.env.prod`

Same keys as `.env.example`, production values, gitignored by `.env.*`.
`scripts/db-url.mjs` reads it when `RUAHA_ENV_FILE=.env.prod` is set, which
every `db:prod:*` script does — and reads nothing else, so dev and prod values
never mix. From the production dashboard:

```
SUPABASE_PROJECT_REF=hwjwpeaadfdvjdkzkqje
SUPABASE_DB_HOST=aws-0-<region>.pooler.supabase.com   # Connect -> Session pooler
SUPABASE_DB_PORT=5432
SUPABASE_DB_USER=postgres.hwjwpeaadfdvjdkzkqje
SUPABASE_DB_PASSWORD=<database password>
```

## First setup (done once)

1. Schema — the same migrations as dev, nothing else:
   ```bash
   pnpm db:prod:push:dry
   pnpm db:prod:push
   pnpm db:prod:list
   ```
2. Prove it matches dev (no output from `diff` means identical):
   ```bash
   psql "$(node scripts/db-url.mjs)" -f supabase/tests/schema_fingerprint.sql > dev.txt
   pnpm -s db:prod:psql -f supabase/tests/schema_fingerprint.sql > prod.txt
   diff dev.txt prod.txt
   ```
   A difference is fixed with a new migration, never by editing an old one.
3. Configuration rows — country, the `ILUNDO` project, the crop list:
   ```bash
   pnpm db:prod:psql -f supabase/prod/bootstrap.sql
   ```
4. Supabase dashboard (production project):
   - Authentication → Sign In / Providers: turn **Allow new users to sign up**
     off. Staff logins come from `grant_staff.sql`, farmer logins from the
     officer's RPC; neither uses sign-up.
   - Authentication → URL Configuration: Site URL = the Netlify production URL.
5. Netlify → Site configuration → Environment variables, each set to
   *different value for each deploy context*:

   | Variable | Production | Deploy previews, Branch deploys |
   |---|---|---|
   | `VITE_SUPABASE_URL` | `https://hwjwpeaadfdvjdkzkqje.supabase.co` | dev URL |
   | `VITE_SUPABASE_ANON_KEY` | production anon key | dev anon key |
   | `VITE_DATA_MODE` | *(unset)* | `demo` |
   | `VITE_FEATURE_RESOURCES` | *(unset)* | `on` |

   Remove `SUPABASE_ACCESS_TOKEN`, `SUPABASE_DB_*` and `SUPABASE_PROJECT_REF`
   from Netlify: the build reads only `VITE_*`, and a database password has no
   reason to sit there. Vite inlines `VITE_*` at build time, so redeploy after
   any change.

## Staff logins

No screen does this (user admin is out of scope). One run per person:

```bash
pnpm db:prod:psql -v email=jane@example.org -v name='Jane Doe' -v role=admin -f supabase/prod/grant_staff.sql
pnpm db:prod:psql -v email=juma@example.org -v name='Juma Said' -v role=field_officer -v village=ILUNDO -f supabase/prod/grant_staff.sql
```

- `role`: `admin`, `ops` or `field_officer`. A field officer needs a village
  code (ops creates villages at /ops/villages), and admin and ops take none.
- A new login prints `temp_password` once. Give it to the person directly;
  the app makes them choose their own at first sign-in.
- Running it again for the same email adds the role only, with no new
  password. A revoked role comes back.
- Revoke — never delete, the audit trail keeps the name:
  ```sql
  update membership set revoked_at = now()
   where user_id = (select id from auth.users where email = 'juma@example.org')
     and role = 'field_officer';
  ```

Order for a fresh launch: grant the admins and ops first, ops adds the
villages in the app, then grant the field officers to those village codes.

## Schema changes from now on

A new migration goes to dev first (`pnpm db:push`, `pnpm db:rls`, CI), then
to production (`pnpm db:prod:push`), then the fingerprint diff again.

## Bug reports

`20261025090001_bug_reports` goes with the next `pnpm db:prod:push` like any
other migration. To read production reports with `pnpm bugs:prod:show`, add a
real admin's login to `.env.prod` — never the demo admin, which does not exist
there — together with the anon key the script signs in through:

```
VITE_SUPABASE_URL=https://hwjwpeaadfdvjdkzkqje.supabase.co
VITE_SUPABASE_ANON_KEY=<anon key>
BUG_READER_EMAIL=<an admin's email>
BUG_READER_PASSWORD=<that admin's password>
```

Production reports may show real people's screens: download them only to
triage, and clear `.bug-reports/` when the ticket is closed.
