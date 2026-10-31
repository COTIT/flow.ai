# Genesy

One shared backend and four apps, in one repo.

| Folder                                                 | What it is                                                                                  | Who uses it              |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------- | ------------------------ |
| `apps/api`                                             | Rust backend: all business rules and the database                                           | every app                |
| `apps/shop`                                            | Public web shop, product pages, QR batch pages                                              | shoppers                 |
| `apps/office`                                          | Back office: products, stock, batches, invoices, orders                                     | staff                    |
| `apps/field`                                           | Farmer registration, harvests, passbooks                                                    | field officers, farmers  |
| `apps/till`                                            | Point of sale and cash payouts                                                              | shop and office counters |
| `packages/ui`                                          | Shared shadcn components and theme                                                          | all four apps            |
| `packages/api-client`                                  | Backend connection (axios), TanStack Query provider, and typed hooks generated from the API | all four apps            |
| `packages/eslint-config`, `packages/typescript-config` | Shared lint and TypeScript settings                                                         | every package            |

Ports: shop 3000, office 3001, field 3002, till 3003, api 8080.
Each app reads the backend address from `NEXT_PUBLIC_API_URL`: copy its `.env.example` to `.env.local`.

Add a shadcn component once for every app: `pnpm dlx shadcn@4.23.0 add <name> -c apps/office` (it lands in `packages/ui`).

## Requirements

- Node 22 or newer, pnpm 9 (`corepack enable`)
- Rust (stable) and Postgres for `apps/api`

## Commands

```bash
pnpm install
pnpm dev        # run every app
pnpm build      # build everything
pnpm lint
pnpm typecheck
pnpm test
```

Run one app: `pnpm --filter shop dev`.

## Typed API calls (OpenAPI bridge)

```
Rust handlers ──▶ apps/api/openapi.json ──▶ orval ──▶ packages/api-client/src/generated
```

- After changing a backend route or its request/response types, regenerate both files:
  `pnpm turbo run generate --filter=@genesy/api-client`. Commit `openapi.json` and `src/generated` with the change.
- Both are checked: `cargo test --test openapi` fails when `openapi.json` is stale, and
  `pnpm turbo run generate:check` fails when the generated client is.
- In an app: `lib/api.ts` calls `configureApi(...)` once; then use the hooks, e.g.
  `import { useCurrenciesList } from "@genesy/api-client/endpoints/currencies"` and types from `@genesy/api-client/model`.

## End-to-end tests (Playwright)

Every web change is checked headless in a real browser; no clicking through by hand.

```bash
pnpm --filter @genesy/e2e exec playwright install chromium   # once per machine (downloads Chromium)
pnpm turbo run e2e                                            # builds shop + office, then runs every test
```

- Each test runs twice: desktop (1280 px) and phone (375 px). Chromium only.
- The shop runs `next start` against a mock storefront API (`packages/e2e/mock/server.mjs`, 127.0.0.1:8199).
  The office runs `next start` and its browser calls are answered by `mockApi` (`page.route`). No database, no Docker.
- Debug: `pnpm --filter @genesy/e2e exec playwright test --ui` (or `--headed`, `-g "<test name>"`).
  CI uploads the HTML report and traces as the `playwright-report` artifact when the job fails.

### How to add a test

Add one file, `packages/e2e/tests/<app>/<screen>.spec.ts`:

```ts
import { mockApi } from "../../src/api-mock" // browser-side apps (office, till, field)
import { seedSession } from "../../src/session" // start logged in
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test("the screen shows its data", async ({ page }) => {
  await seedSession(page)
  await mockApi(page, { "GET /me/workspaces": { body: { data: [] } } })
  await page.goto("/")
  await expect(page.getByRole("heading", { name: "Products" })).toBeVisible()
  await expectNoHorizontalScroll(page)
})
```

- `test` fails on any console error, page error, failed request or HTTP status >= 400.
  A test that expects one lists it exactly: `test.use({ allow: { responses: [{ url: "/b/ZZZZZZZZZZ", status: 404 }] } })`
  (or `console: ["<full message>"]`). No patterns.
- Shop data lives in `packages/e2e/mock/` (`fixtures.mjs` holds the ids and codes specs use).
- Query by role and name (`getByRole`), as a screen-reader user would find things.
- A new app gets its projects with `projectsFor("<app>", { testDir, baseURL })` and a `webServer` entry in `packages/e2e/playwright.config.ts`.
