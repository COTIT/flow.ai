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
- The field build needs `VITE_FEATURE_RESOURCES=on` (CI sets it in `.github/workflows/ci.yml`), or its equipment, loans and requests tests fail: `VITE_FEATURE_RESOURCES=on pnpm turbo run build --filter=@genesy/field --force` (force, because turbo caches the flag-off build).
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

CI uses standard ubuntu-24.04 GitHub runners. Compute is free for this public repository; artifact and cache storage retain GitHub's plan limits. Peak parallel work is 17 jobs, below the Free plan's 20-job allowance. Other organization workflows can still queue these jobs.

The changes job compares inputs with the last fully green main commit. Unknown bases run every check, and a newer push cancels the older run. Stable api, web and stack-e2e checks collect results from quality checks and every test shard. Failed, cancelled or missing required children block release.

Browser tests run across eight jobs with three workers each. Each builds its own apps. Two full-stack jobs each own their database, file store and seed. API integration tests use two isolated Postgres instances and a deterministic partition of compiled test names. Migration prechecks run before each template is migrated. Other API targets run once. The local gate keeps the complete suite unsharded.

Image preparation starts alongside checks: production API, staging API and web are separate producers. They push only ci-<run-id>-<attempt> candidates after smoke tests and vulnerability scans. The two images checks validate all producer manifests and publish commit-SHA tags by immutable digest after every gate passes. Unchanged image groups reuse digests from the last green commit. A partial publication is a failed release; always require a green main workflow before deploying.

Clippy and test dependency caches have separate keys and one writer each. Stack caches remain separate. The web quality job writes the pnpm cache; browser shards only restore it. Both API image variants export their own GHCR layer caches. Trivy comes from its digest-pinned official container image; its binary SHA-256 is verified after extraction and every cache restore. Project test results are not persisted as a shortcut around executing tests.

Producer manifests last seven days. Failed-job reruns can reuse earlier successful producers from the same run and commit. Full reruns select the newest producer attempt. Missing or expired manifests require a full workflow rerun. Candidate cleanup is deferred because candidate and release tags can share the same package version.

The old self-hosted runner on the VPS was removed on 2026-11-02; the VPS only pulls images and runs them.

### Release timing

The target is a median below ten minutes across three representative warm releases: web-only,
API-only and combined changes. Measure CI from workflow creation through image publication,
including runner queues. Add bundle creation/transfer and deployment execution; production
includes staging first. Exclude only human waiting between manual promotions. Report each
result and the maximum. Cold builds may take longer; a faster failed run is not a release.

Each deployment writes timing events to stderr and `deploy/<env>/deploys.log`, including its
final status. Keep backups, migration checks, health probes and pruning inside the measured
execution. Before benchmarking production, configure its catalog app key and require the runtime
scan to pass under the repository's reviewed vulnerability policy. Retain the existing zlib
exceptions until a patched distroless rebuild is available, as recorded in `.trivyignore`. Production shop HTTP 500 is a failed
health check even when its container reports healthy.

To roll back the CI restructuring, revert its workflow/helper changes. Existing SHA tags,
manual deploy commands and application/database rollback rules remain usable.
