# Full-stack e2e

Headless Playwright against the **real** stack: the Rust API, a throwaway Postgres seeded with
the demo data (`seed-demo`), and office, shop, till, field, landing and flow built for that API. The
specs in `tests/**` (the `e2e` task) use a fake API instead; this suite is the one that catches
permissions, cross-app flows and seed gaps.

## Run it

```bash
pnpm --filter @genesy/e2e stack:up       # Postgres + API + 6 apps, then leaves them running
pnpm --filter @genesy/e2e exec playwright test -c playwright.stack.config.ts   # as often as you like
pnpm --filter @genesy/e2e stack:reset    # back to the freshly seeded data (seconds; apps keep running)
pnpm --filter @genesy/e2e stack:down     # stops everything, removes the container
pnpm --filter @genesy/e2e stack:test     # up → specs → down in one go (what CI runs)
```

Extra arguments to `stack:test` go to Playwright, e.g. `stack:test tests-stack/till-sale.spec.ts --project=stack-desktop`.

- **Needs:** Docker (Postgres and the file store), Rust 1.99, the repo's `pnpm install`, Playwright's Chromium.
- **Ports** (all + `E2E_STACK_OFFSET`, default 0): Postgres 55400, API 8400, shop 4300, office
  4301, field 4302, till 4303, landing 4304, flow 4305, mail sink 2525, file store 55410. Pick another offset per checkout running at once,
  e.g. `E2E_STACK_OFFSET=20`. The fake-API suite uses 3200-3204/8199 + `E2E_PORT_OFFSET`.
- **Containers:** `genesy-e2e-stack-<offset>`, Postgres in RAM (tmpfs), and `genesy-e2e-files-<offset>`,
  a local S3 store (Garage, pinned by digest in `stack/stack.mjs`, settings in `stack/garage.toml`) in
  RAM, with a random key per stack. Staging and production use Backblaze B2; the stack never reaches
  B2 or AFM. The API stores uploads there and signs links for it; the shop's and office's
  `FILES_ORIGIN` is that one origin. The seed runs without `--images`, so it never fetches AFM's
  photos. The harness never touches any other container (not the dev Postgres on 54320).
  `stack:down` and any failed `stack:up` remove both.
- **API:** the **debug** build (`cargo build`, `SQLX_OFFLINE=true`). Release refuses
  `PAYMENTS_PROVIDER=fake`, which shop checkout needs, and compiles 2-3x slower. Target dir is
  `apps/api/target` (or `CARGO_TARGET_DIR`); `cargo clean` there when you need the disk back.
- **App builds:** Next bakes the `/v1 → API_URL` rewrite into the build, so the stack builds its
  own copies into `.next-stack/` (`NEXT_DIST_DIR`, see each `next.config.ts`) and field into
  `dist-stack/`. They never clash with the fake-API builds in `.next/` and `dist/`.
- **Logs and state:** `packages/e2e/.stack/<offset>/` (gitignored): `state.json` (ports, pids),
  `seed.json` (the logins `seed-demo` printed: **never commit**), `logs/<process>.log`,
  `mail/*.eml` (every mail the API sent).
- **Mail:** the API's `SMTP_URL` is the stack's own sink (`stack/mail-sink.mjs`, plain SMTP on
  127.0.0.1, no dependency). It writes each message to `mail/`; nothing leaves the machine.
- **Time:** see "Timings" below.

## Write a spec

Specs live in `tests-stack/*.spec.ts` (Field's in `tests-stack/field/`, with a Field sign-in helper that skips the first-visit tour) and run at desktop and 375 px (`stack-desktop`, `stack-mobile`).
Import everything from `../src/stack`:

```ts
import { apiAs, cred, expect, loginOffice, pairTill, test, unique, urls } from "../src/stack"

test("…", async ({ page, request }) => {
  const owner = await apiAs(request, cred("office (AFM)", "owner"))   // API client as a seeded member
  const product = await owner.post<{ product_id: string }>(owner.ws("/products"), { name: `e2e ${unique()}`, price: "4.00" })
  await loginOffice(page, cred("office (AFM)", "owner"))               // real login page
  await page.goto(`${urls.office}/products/${product.product_id}`)
})
```

- `test` / `expect`: the shared guard: any console error, page error, failed request or
  response >= 400 fails the test. Expected ones go in `test.use({ allow: … })` with **absolute**
  URLs (there is no baseURL; apps differ per URL). The default, `stackAllow`, allows nothing
  today (it allowed the office's file-list 503 before the stack had a file store); keep spreading
  it into your own `allow`.
- API lists page with `page_size` (not `per_page`) and come newest first.
- `urls.{api,office,shop,till,field,landing,flow}`: where each one answers. Flow's server proxies
  `/api/voice/*` to the stack's Office (`VOICE_UPSTREAM_URL`), as compose points it at `office:3000`.
- `cred(app, role)`: a seeded login, as `seed-demo` printed it. Apps: `office (AFM)` (owner),
  `office (RUAHA)` (admin), `field (RUAHA)` (ops, field_officer, farmer, …), `till (AFM)`
  (device key, cashier PIN). Unknown pairs throw with the full list.
- `workspaces.AFM`, `workspaces.RUAHA`: the seeded workspace ids.
- `notes`: the seed's other lines (storefront app key, passports, sample shop orders).
- `apiAs(request, cred)`: `get/post/put/patch` on `/v1/…` as that member, `ws(path)` adds the
  workspace; throws on >= 400. For setup and for checking results, never instead of the UI step under test.
- `loginOffice(page, cred)`, `loginFlow(page, cred)`, `loginField(page, cred)`: through the real login pages.
- `pairTill(page, request)`: a register of the test's own (in the AFM store, like the seeded
  "AFM counter"), a real pairing code from the office API, pairing through `/setup`, then the
  seeded cashier's PIN. Returns `{ owner, register }`. Pairing a seeded register would end every
  other test's session on it, so never do that.
- `openShift(page)`: opens the paired till's shift (a new register has none; sales need one).
- `unique()`: a suffix for names of things the test creates.
- `mailTo(address)`: the newest mail the API sent there (waits up to 10 s), to follow an invite
  or verification link. Signup is on in the stack, so a spec that needs a verified owner makes
  its own workspace (`POST /v1/create-workspace`) and opens the verification link from its mail
  (see `tests-stack/accounts/invite.spec.ts`), never a seeded user's.
- A known bug: `test.fail(true, "<what is wrong> (<session or issue>)")` at the top of the test,
  so the suite stays green and flips red (unexpected pass) the day it's fixed: then drop the line.

## Data rules

One seed per stack, shared by every worker and both projects (a reseed per worker would mean a
Postgres, API and six apps per worker). So:

1. **Make your own data** where the API lets you (products, registers, customers, orders), named
   with `unique()`. Then nothing else reads or changes it.
2. **Assert deltas, not seeded numbers,** on anything shared (stock of a seeded product, a
   farmer's credit balance, order counts): read before, act, read after.
3. **Change a seeded record's own fields** (name, price, status, permissions) only when rule 1 is
   impossible, and claim it in the table below first. One spec file per record; both projects
   run it, so it must stand being run twice at once.
4. **Never** reset or reseed from a spec. `stack:reset` is for people between runs.
5. **Remove what a workspace caps.** AFM allows at most 10 webhook endpoints: a spec that adds
   one deletes it again (see `tests-stack/office/settings.spec.ts`).

| Seeded record | Claimed by | What it changes |
| --- | --- | --- |
| _(none yet)_ | | |

## CI

`stack-tests` runs two independent shards on standard public-repository `ubuntu-24.04`
runners. Each job builds its own API and apps, starts an isolated database and file store,
and executes `stack:test --shard=i/2` with two workers. Shard one checks that the discovered
unsharded inventory equals the union of both shards before testing. Real expiry and cache
propagation waits stay unchanged.

`stack-e2e` collects both results and gates final image publication. Failed, cancelled or
missing required results block release. Failure artifacts include the shard number and keep
reports, traces and stack logs for seven days. Only shard one writes dependency caches;
shard two restores them. The local gate runs the suite unsharded.

## Timings

Measured 2026-11-04 on a shared Mac (load average 45-85 from other builds, so slow):

| Step | Cold | Warm |
| --- | --- | --- |
| API debug build | 1885 s (whole dependency tree) | 11 s (nothing changed) |
| Postgres up | 20 s | 6 s |
| `seed-demo` (debug build) | ~420 s | ~190 s |
| 5 app builds (beside the two above) | ~560 s | ~270 s |
| Stack up in total | ~35 min | 284 s |
| 3 specs x 2 projects | 2.1 min | 1.5 min |
| `stack:test` in total | | 384 s |
| `stack:reset` | | 8 s |

The seed is the slowest step once the API is built: it runs the whole demo through the debug
build (password hashing included). These historical measurements predate CI sharding. Warm shard timing must be measured on GitHub runners.

## Known bugs these specs carry (`test.fail`)

None at the moment.

Worked around (not `test.fail`, since they depend on timing or on how long the API has run):
- `shop/checkout.spec.ts`: Pay again right after Cancel on the fake page reopens the cancelled
  payment until the API has recorded the cancel; the spec waits for it.

## Follow-ups

- The fake-API suite starts office and till with `API_URL=http://127.0.0.1:9/v1` to "catch
  leaks", but Next baked the rewrite at build time (`localhost:8080`), so a browser `/v1` call that
  slips past `mockApi` goes to whatever runs on 8080 (a dev API!) instead of failing. (Field's Vite
  preview proxy reads `API_URL` at start, so it is fine.)
