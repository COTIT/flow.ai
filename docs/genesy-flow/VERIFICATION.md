# Genesy Flow verification

Updated 2026-11-10. Phase 1 remains in progress. Results below were actually executed; pending checks are not approvals.

## Passed

| Command / check | Actual result |
| --- | --- |
| `bash deploy/compose.test.sh` | Passed; default-off flag first failed as intended, then green |
| `pnpm --filter @genesy/ui test`, UI lint/typecheck | 111 tests passed; lint/typecheck green |
| `pnpm --filter @genesy/shop test`, Shop lint/typecheck | 97 tests passed; lint/typecheck green; existing catalog loader unchanged |
| Shop `next build --webpack` + `scripts/check-client-bundle.mjs` | Production build and secret-bundle check passed |
| `playwright test -c storefront-design.config.ts` | 18 passed, desktop/mobile/tablet; screenshots visually checked |
| `playwright test -c shop.config.ts --workers=3` | 248 passed, 6 viewport skips; existing expected-failure tests retained |
| Office `next build --webpack` | Production build passed |
| Office `playwright test -c voice.config.ts storefront.spec.ts` | Corrected final run 14/14 passed desktop/mobile |
| Office `playwright test -c voice.config.ts voice.spec.ts` | 72/72 passed desktop/mobile |
| Office route permission tests | Red→green, 2 passed |
| Office gap-follow-up tests | Red→green, 10 passed; new static route preserved |
| API focused `cargo test --test api storefront_design -- --test-threads=4` | 8 passed on disposable Postgres 18 |
| API schema unit tests | 2 passed; runtime/schema unknown-field parity, defaults, counts, IDs, Unicode/bounds |
| API migration up/down/up | Passed on disposable Postgres 18 |
| API `cargo check --lib`, SQLx prepare all-targets, OpenAPI generation | Passed; final gates still running |
| API client typecheck/lint/test | Green; 24 tests passed |
| Voice policy regression | Red→green, 32 passed after 8 internal ops; 491 total/396 staff tools/4 unavailable |
| Security review | No confirmed new vulnerabilities; final artifact/hash delta review pending |

## Pending / failures being resolved

- Full Rust suite, Clippy and final SQLx freshness: API child running; record final counts and exit status.
- Full Office suite first run: 605/608 passed; stale authority hash, required static `storefront` path segment, and sandbox listener EPERM. Static segment fixed; authority refresh and elevated rerun pending. Office lint/typecheck and e2e typecheck passed.
- Full-stack harness first run built/started API, applied 90 migrations and seeded 22 test logins, then stopped on unrelated landing webpack `/nl/opengraph-image` prerender error: `ERR_INVALID_ARG_TYPE`, URL realm/path mismatch. Harness cleaned its own containers. No landing source changed. Second run is active; inspect checkpoint process list/logs.
- Final integrated Rust review, final source authority hashes/voice generation/freshness, generated-client freshness and full-stack Phase 1 test remain incomplete.
- Dependency audit attempted but registry DNS failed; manifests/lockfile unchanged.

## Environment and artifacts

- Initial runtime reports Node v23.3.0. Installed path `/opt/homebrew/opt/node@22/bin/node` also reports v23.3.0; do not claim Node 22 validation from its pathname. A temporary `pnpm dlx node@22` attempt may still be finishing; check before further use.
- Turbopack production build worker-port EPERM persisted even elevated. Supported webpack builds passed without tracked builder changes. Full-stack run uses temporary `/tmp/genesy-flow-tools/pnpm` to append `--webpack` only to Next build commands.
- Disposable API test PG: `genesy-flow-phase1-pg`, 127.0.0.1:55460; test-only password `phase1-local-only`. Never production data.
- Real stack offset 120: postgres 55520, files 55530, API 8520, Shop 4420, Office 4421. State/logs under `packages/e2e/.stack/120/`; credentials there are ignored test artifacts, never commit.
- Shop screenshots: `/Users/cotit/.codex/visualizations/2026/11/09/01a1c1a3-f391-74d2-8200-1c50a5251c53/storefront-phase1/published-home-{desktop,mobile,tablet}.png`.
- Office screenshots: `/private/tmp/genesy-flow-office-oiw7x9h1/test-results-office-storefront/`, per desktop/mobile case. Child visually checked both.
- New main-content targets checked at 44px. Existing desktop/tablet header/footer controls were left unchanged; whole-shell remediation is outside Phase 1.
- Baseline catalog/commerce/voice regressions remain required; current test files define expected legacy behavior.

## Reproduce draft creation

From repository root, use a local/test API and a verified owner/admin session. Supply
`GENESY_API_URL` including `/v1`, `GENESY_WORKSPACE_ID`, and `GENESY_TOKEN` through the
environment. The following saves a draft only; secrets are never printed or placed in
command-line arguments. Start Office and open `/settings/storefront` to review/publish.

```sh
node --input-type=module <<'JS'
import { readFile } from 'node:fs/promises'
import { randomUUID } from 'node:crypto'
const { GENESY_API_URL: base, GENESY_WORKSPACE_ID: workspace, GENESY_TOKEN: token } = process.env
if (!base || !workspace || !token) throw new Error('Set GENESY_API_URL, GENESY_WORKSPACE_ID and GENESY_TOKEN')
const url = `${base.replace(/\/$/, '')}/workspace/${encodeURIComponent(workspace)}/storefront/pages/home`
const headers = { Authorization: `Bearer ${token}` }
const call = async (suffix, options = {}) => {
  const response = await fetch(url + suffix, { ...options, redirect: 'error', signal: AbortSignal.timeout(10000), headers: { ...headers, ...options.headers } })
  if (!response.ok) throw new Error(`Design API returned HTTP ${response.status}`)
  return (await response.json()).data
}
const state = await call('')
const document = JSON.parse(await readFile('apps/api/tests/fixtures/storefront-design-v1.json', 'utf8'))
const draft = await call('/revisions', {
  method: 'POST', headers: { 'Content-Type': 'application/json', 'Idempotency-Key': randomUUID() },
  body: JSON.stringify({ expected_version: state.version, document }),
})
console.log(JSON.stringify({ version: draft.version, revision_id: draft.draft_revision_id }))
JS
```

