# Genesy Flow verification

Updated 2026-11-10. Results below were actually executed on branch `codex/flow-foundation`.
Pending items are listed as pending, never as passed.

## Full local gate (`scripts/local-gate.sh`) on commit `42985896e`

`main` had not moved (`git log HEAD..origin/main` empty), so this was the merged tip.

| Step | Result |
| --- | --- |
| Deployment helper tests | Passed |
| `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | Passed |
| Migration tests + `migrate` on an empty template DB | 2 passed; migrate passed |
| `cargo test` lib | 475 passed |
| `cargo test` api | 1538 passed, 6 ignored |
| `cargo test --test openapi` | 69 passed, **1 failed**: `operation_ids_are_unique_and_scoped` (tag `storefront-design` vs ids `storefront_design_*`). Fixed in `6f049b0d3`, rerun 70/70 |
| No internal names in spec | Passed |
| `generate:check` (api-client), `voice:check`, voice schema check | Passed |
| `turbo typecheck lint test build` (all web packages) | 36/36 tasks |
| Fake-API e2e (`turbo run e2e`) | 1987 passed, 16 skipped, **1 failed**: `supply-transfers.spec.ts:192` office-mobile. Unrelated to this work; rerun in isolation `--repeat-each=3`: 30/30 passed (load flake) |
| `storefront-design.config.ts` | 21 passed (desktop, 375px, tablet) |
| `voice-dev.config.ts` | 12 passed |
| Full stack (`stack:test`) | 187 passed, **2 failed**: `tests-stack/storefront-design.spec.ts` desktop + mobile still located revisions by UUID after the Office accessible-name fix. Fixed in `6f049b0d3`; rerun below |

## Reruns after `6f049b0d3`

| Check | Result |
| --- | --- |
| `cargo test --test openapi` | 70 passed |
| `voice:check`, voice schema check, `vitest run lib/voice` (Office) | Passed; 41 files, 453 tests |
| `generate:check` (api-client) | Passed |
| Full-stack storefront spec (desktop + 375px), `stack:test` | 2 passed |

## Child-session evidence (before integration)

- API child: `cargo test --test api storefront_design` 13 passed (8 + 5 new: field-path 422, 64 KiB backstop, stale published doc → null, key without `catalog:read` → 403, publish race with different keys → one 200 + one 409). dto unit tests 2 passed. Migration up/down/up on disposable PG 55460 only.
- Frontend child: UI 111, Shop 97 unit; Office `voice.config.ts storefront.spec.ts` 14 passed; `storefront-design.config.ts` 21 passed. Every fix had an observed red first, except the hero-CTA e2e, which is new coverage and was green on first run.
- Real-stack storefront spec on the pre-fix tree with Turbopack: 2/2 passed. The earlier landing `ERR_INVALID_ARG_TYPE` came only from a temporary shim forcing `next build --webpack`; CI and the stack use Turbopack.

## Reviews

- Security review (read-only, commit `42985896e`): no confirmed or plausible exploitable issues. Workspace scoping in every query plus composite FKs, admin gate, app-key scope, actor-bound idempotency, escaped text with fixed hrefs, bounded documents, server-only key.
- React review: no blocking bugs. Low: the focused preview button loses focus while busy (cosmetic, left as is).
- Two spec audits (API, frontend) drove the 13 fixes listed in DECISIONS.md.

## Not verified

- Staging rollout, production deploy, and old-binary rollback compatibility (no deploy in Phase 1).
- Node 22 specifically. Local runtime is Node v23.3.0; CI pins its own version.

## Reproduce draft creation

From repository root, use a local/test API and a verified owner/admin session. Supply
`GENESY_API_URL` including `/v1`, `GENESY_WORKSPACE_ID`, and `GENESY_TOKEN` through the
environment. This saves a draft only and never prints secrets. Then open Office
`/settings/storefront` to review and publish.

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
