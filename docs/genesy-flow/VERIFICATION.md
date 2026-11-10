# Genesy Flow verification

Updated 2026-11-10. Results below were actually executed: staging on `dd6678bfe`, local gates on `codex/flow-foundation`.
Pending items are listed as pending, never as passed.

## Staging validation (2026-11-10) — PASS

Environment: staging VPS stack, API/Office/Shop images `dd6678bfe` (Phase 1 + shop image fix #14), deployed by `deploy/deploy.sh staging dd6678bfe… --bundle` after `main` `ci` was fully green including both `images` jobs. Pre-deploy backup `genesy-staging-20261110T043217Z-pre-dd6678bfe….dump` (also off-site). Migration applied: `20261109191001` (schema `20261109130000` → `20261109191001`). Production untouched (`fbe1fcf15`). Workspace: `afm-demo` (`01a1b252-0e26-…146`, 72 public products), the one workspace the staging Shop key serves; user-approved. Office actions used a signed-in afm-demo owner session; API steps ran from that page through Office's `/v1` proxy (no secret left the browser). Shop-key reads ran inside the shop container.

Before this, `main` `ci` for `e041285e7` failed `image-prepare (web)`: shop tests statically imported the API fixture, which the web image context excludes (TS2307). Reproduced with `deploy/images/next.Dockerfile`, fixed in #14 (runtime read), verified shop + office image builds green.

| Step | Result |
| --- | --- |
| A. Legacy baseline, flag off (desktop 1280, iPad, 375px) | Pass: legacy h1, header/footer, batch form, `/shop` → product → add to cart → checkout form, passport `/b/2RH1SSHG8Z`, `nl` locale; no console errors, failed requests or overflow (`legacy-flag-off.report.json`) |
| B. Draft via API (fixture, expected_version 0, Idempotency-Key) | 201; version 0→1; draft set; published null; one `saved` event; public read with Shop key `{"data":null}` |
| C. Office preview | Revision list + preview render (`c1`, `c2`); 6 grid products hash-identical to the public catalog entries (signed image URLs excluded): prices/stock authoritative |
| D. Publish via confirmation dialog | Dialog required (`d1`); version 2; both pointers rev 1; "Storefront published." (`d2`); public read returns only `revision_id`, `document`, `collections`. Flag still off → Shop stayed legacy |
| Flag on (`SHOP_DESIGN_ENABLED=true`, shop recreated) | Shop renders design immediately (no-store design loader); grid links open matching products, 6/6 card prices equal product-page prices; hero CTA → `/shop`; batch form still present |
| E. Responsive, design mode (3 viewports × rev 1, rev 2, rev 3) | Pass, 0 problems each; split hero + 3 cols desktop, 1 col 375px; rev 2 centered hero, 3 cols tablet, 8 products |
| F. Restore | Second revision saved + published (version 4, Shop updated). Restore rev 1 via its own dialog (`f1`) → new draft rev 3 (parent rev 2, restored_from rev 1), version 5, Shop unchanged until publish; publish → version 6, Shop shows rev 1 content. Every revision/event has author + login snapshot |
| G. Failure + isolation (no writes) | No auth 401 (GET, POST); Shop app key on staff route 401; foreign workspace state 404; foreign-workspace preview of an afm revision 404; unknown revision 404; stale save 409; stale publish 409; unknown component 422; URL field 422; duplicate id 422 `document.sections.1.id`; missing Idempotency-Key 422; state unchanged |
| Flag restored | `.env.staging` restored byte-identical from the pre-validation copy (diff showed only the added line); container `false`; final legacy run 3/3 viewports, 0 problems |

Covered by existing automated tests, not exercised on shared staging: non-admin member refused, design API outage → legacy fallback, catalog outage error page, "nothing published" with flag on, concurrent first saves, idempotent replay, transaction rollback (API `storefront_design` tests, Shop unit tests, fake-API `storefront-design` e2e).

Not verifiable on staging: the API logs no request paths, so "flag off makes no design request" rests on the Shop unit test plus the container env, not a runtime log.

Evidence (committed, compressed): `docs/genesy-flow/evidence/staging-2026-11-10/` (JSON reports per run, Office screenshots, home screenshots). Full-resolution PNGs (local, git-ignored): `packages/e2e/test-results/flow-staging-2026-11-10/`.

Final staging state: flag false (original); afm-demo keeps published revision 3 (fixture content) stored, invisible while the flag is off. Rollback: feature = flag false (done). Binary rollback to `fbe1fcf15` is guarded by the schema check: plain `rollback` refuses; it needs `--restore-db` (loses writes since deploy) or `--compatible-assistant` with the assistant disabled. Not executed.

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

- Production deploy. Old-binary rollback was not executed on staging (guarded by the schema check; see staging section).
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
