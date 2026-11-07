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

## Office voice assistant

Open `/voice` in Office for ElevenLabs voice or text conversations. Voice also accepts typed messages and temporary JPEG, PNG, WebP and PDF references. The conversation persists across Office navigation. Rust enforces the signed-in staff member's current permissions and workspace on every operation. Credential entry and physical verification open secure native forms after the conversation ends.

Use `apps/office/.env.example` for server configuration. Set `ASSISTANT_WORKSPACE_ALLOWLIST` to the comma-separated UUIDs of approved workspaces. Empty or malformed scope refuses new work. Enable admissions with `ASSISTANT_ENABLED=true` after migrations, restricted checkpoint-role provisioning and `/api/voice/readiness` pass. Configure `ELEVENLABS_API_KEY`, `ELEVENLABS_AGENT_ID`, `ASSISTANT_BRIDGE_SECRET` and `ASSISTANT_CHECKPOINT_URL`; the Rust API needs the same bridge secret and its provider cleanup key. Runtime secrets support `NAME_FILE` instead of `NAME`. See [operator setup and rollback](deploy/ASSISTANT.md).

The native agent requires signed sessions and the `discover_genesy_tools` and `execute_genesy_tool` client tools. ElevenLabs requires its **ElevenAgents Write** key permission to issue signed sessions. Its primary and explicit fallback models must support images and PDFs. One native planner serves voice and text; no separate OpenAI integration is required.

Set `LANGSMITH_API_KEY`, `LANGSMITH_TRACING=true` and `LANGSMITH_PROJECT` for workflow tracing. Inputs and outputs are hidden; credentials, conversation content, file bytes and raw business responses stay outside checkpoints and exported traces. Exporter failure never retries a business action.

Effects show an immutable review and expire after 120 seconds. LangGraph persists the review interruption in PostgreSQL; Rust atomically consumes the exact approved request before dispatch. Office exposes the confirmation phrase only on the native review card, never in model tools or contextual updates. Users can read it aloud or approve with its button. Spoken approval passes through ElevenLabs ASR as a user utterance; later echoes cannot dispatch the action again. Lost acknowledgement remains unknown until reconciled. Mail-server acceptance does not establish delivery.

References are limited to 10 MiB each, ten per session and two concurrent uploads. Inspection never implicitly saves or sends a file. End/clear schedules durable provider cleanup; used references remain in model context until that conversation is deleted. Browser reload restores task status through opaque IDs and requires a fresh review; conversation content and original file bytes are not restored.

After backend contract changes, run `pnpm --filter @genesy/office voice:generate`. `voice:check` and the Office unit tests detect a stale catalog. Run the focused browser checks against an Office production build with `pnpm --filter @genesy/e2e exec playwright test --config voice.config.ts voice.spec.ts`; both desktop and 375px layouts are covered.

## End-to-end tests (Playwright)

Every web change is checked headless in a real browser; no clicking through by hand.

```bash
pnpm --filter @genesy/e2e exec playwright install chromium   # once per machine (downloads Chromium)
pnpm turbo run e2e                                            # builds shop + office, then runs every test
```

- Each test runs twice: desktop (1280 px) and phone (375 px). Chromium only.
- The shop runs `next start` against a mock storefront API (`packages/e2e/mock/server.mjs`, 127.0.0.1:8199).
  The office runs `next start` and its browser calls are answered by `mockApi` (`page.route`). No database, no Docker.
- Two checkouts at once (e.g. two worktrees): give one `E2E_PORT_OFFSET=40` so its ports don't clash.
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

## CI runners and cost

`.github/workflows/ci.yml` runs on GitHub-hosted runners (2-core `ubuntu-24.04`). The repo is private, so every minute is billed ($0.006/min, each job rounded up to a whole minute, $12 a month included on the Free plan). The spend cap is a budget in GitHub billing. What keeps the bill down:

- **Skip what didn't change.** The `changes` job compares the commit with the last green `main` commit (`deploy/ci/changes.sh`, tested by `deploy/ci/changes.test.sh`). `api` runs only for `apps/api/`, `web` (checks, builds, e2e) only for web code or `apps/api/openapi.json`. Changing the workflow or `deploy/ci/` runs everything. A missing or unknown base runs everything.
- **A newer push cancels the older run**, on `main` too.
- **Images:** the two `images` jobs (api, web) build, scan and push only a group whose inputs changed. Otherwise they copy the last green commit's tags in the registry, so every green commit still has a full set of `ghcr.io/cotit/genesy-<app>:<sha>` tags.
- **Caches:**
  - Rust dependencies (one entry per `Cargo.lock`, saved from `main`), the pnpm store, `cargo-audit` and the Trivy DB (one a day) use the Actions cache.
  - The API image's dependency layer is cached in GHCR (`genesy-buildcache`).
  - Playwright browsers come in the job's container image.

The old self-hosted runner on the VPS was removed on 2026-11-02; the VPS only pulls images and runs them.
