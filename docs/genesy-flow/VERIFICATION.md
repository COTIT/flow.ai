# Genesy Flow verification

Updated 2026-11-10. Results below were actually executed: staging on `dd6678bfe`, local gates on `codex/flow-foundation`.
Pending items are listed as pending, never as passed.

## Phase 3 P3-C — Flow design proposal UI (2026-11-11)

Design approved by the user in Claude Design (v7). Child commits `8e3f26691`…`a840f1a43` on `claude/flow-p3c-design-51f0ef`, merged onto P3-A+B (`1af34f819`) and main (`c112fab20`).

| Check | Result |
| --- | --- |
| Child runs | voice vitest 138, Flow vitest 188; Flow e2e 79/79 at desktop + 375 (+ edit review at 834), design-conversation 20/20 after hardening; Office voice e2e 155 passed / 2 skipped (stock-transfer rows kept by the narrowed "label · Field" rule); stack `flow-design.spec` 8/8 (real dry run in the frame at 375/1280, moved draft 409 → no frame + Approve disabled, member 403) |
| Security + React review | No issues above low. Hardened test-first: hold while the dry run is pending; hold keyed to login + workspace and cleared on session end; Flow query cache confirmed cleared on logout / user change |
| Full gate (`SKIP_API=1`: no API change vs gated P3-A+B), clean checkout `1af34f819` | Web turbo green; fake-API e2e 2164 passed / 17 skipped / 1 failed (Office `supply-transfers.spec.ts:192` 409 alert, recurring load flake, rerun 20/20); storefront-design 21; voice-dev 12; stack 207 passed / 1 skipped |

## Phase 3 P3-A + P3-B — targeted design edits + voice tools (2026-11-11)

Commits `0f7cab4df` (P3-A), `768e27f5a` (review fixes), `785ce5c4c` (P3-B), `0b60600d6` (`text` field), merged with main as `39c0fd2f2`.

| Check | Result |
| --- | --- |
| Child runs (disposable PG 55490; 54320 never touched) | fmt/clippy clean; lib storefront 16, lib assistant 29, api storefront 43, api assistant 114 (incl. `assistant_storefront_design` 3: edit + publish dispatch through signed single-use cards, tampered body 409, write after review 409 with nothing stored, closed ops refused, member 403), openapi 70 (+2 paths, +5 schemas); `sqlx prepare --check` clean; api-client generate:check 0; `voice:check` 493 operations / 402 tools; Office lib/voice + i18n + eval 584; `@genesy/voice` 126 |
| Reviews (Rust + security, independent) | No critical/high. Fixed test-first: dry run reads in one READ ONLY REPEATABLE READ snapshot; new-section facts list every field; category rows carry `category_id`. Accepted: silent partial no-ops, O(n·m) category naming, unlocked capture (dispatch re-checks under lock) |
| Bug found by the gate | Design facts used `base`; the dispatch claim requires `locked`, so no design write could dispatch. Fixed red→green (`785ce5c4c`) |
| Full gate, clean detached checkout `39c0fd2f2` | API: fmt, clippy, migrations, 2113 cargo tests passed / 0 failed, OpenAPI committed spec; web turbo green; fake-API e2e 2135 passed / 17 skipped / 10 failed, all 30 s timeouts under load (Office voice, Field) → rerun 52/52 (2 repeats); storefront-design 21; voice-dev 12; stack 199 passed / 1 skipped |

Not verified: live ElevenLabs use of the design tools (agent prompt block is a draft until the D9 staging validation run).

## Phase 5 — component demand (2026-11-10)

| Check | Result |
| --- | --- |
| Child (disposable PG 55480; shared 54320 never touched) | Rust `assistant_gaps` 12/12 (4 red first: function missing), `assistant_retention` 6/6; fmt/clippy clean; migrate run/revert/run OK; `cargo sqlx prepare --check` OK (`.sqlx`, `openapi.json`, client unchanged); function SECURITY INVOKER (test asserts `prosecdef=false`); floor test: 2 workspaces stay hidden with k=1, 0, -5; Office related vitest 103/103 (red first), tsc/eslint/prettier clean |
| Full gate (`scripts/local-gate.sh`, with API, on Phase 5 + main `281ca739a`) | API: fmt, clippy, migrations, 2090 cargo tests passed / 0 failed, OpenAPI committed spec; web turbo green; fake-API e2e 2039 passed / 16 skipped / 3 failed under load (Office supply-transfers 409 notice, Office PWA offline ×2: HTTP 500 `/v1/me`; unrelated, no UI change in Phase 5) → rerun 12/12 (3 repeats, desktop/mobile/PWA); storefront-design 21; voice-dev 12; stack 195 passed / 1 skipped |

## PR 2C — Flow conversation + storefront device preview (2026-11-10)

Branch `flow/2c-conversation`: children A (conversation, `68894749f`), B (storefront review + preview frame, `6aabf5cbd`), C (client-bundle check, `92ef9dbc5`), parent `2e3130118` (bundle check in build, `FILES_ORIGIN` img-src), stack-spec update `af5768fc7`, edge-case children (headings `46fd1df5c`, cross-tab `47e29069d`), main merged (`541261b61`, incl. #21), revision short code fix (see table).

| Check | Result |
| --- | --- |
| Children (related tests only) | A: Office voice e2e 158 passed / 2 skipped before and after the component extraction; B: Flow storefront + preview e2e incl. frame-ancestors, cross-origin refusal, 375/834/1280 real breakpoints, inertness, member refusal, logged-out frame; C: bundle-check fixtures 27/27; cross-tab: Flow vitest 179/179, related e2e 60/60 (desktop + 375); headings: ui unit red→green, Office/Shop markup byte-identical, Flow frame e2e red→green |
| Security reviews (2, independent) | No confirmed or plausible issues; low finding "Flow CSP lacks `FILES_ORIGIN`" fixed test-first (`2e3130118`) |
| First gate (`2e3130118`, `SKIP_API=1`) | Deploy tests pass; fake-API e2e 7/7 tasks; storefront-design 21; voice-dev 12; stack 197 passed / 1 skipped / **2 failed**: `flow-proxy.spec` still expected the 2B landing's email line → updated to the member name (`af5768fc7`), rerun Flow stack specs 10/10. Web step stopped on the known `gap-triage` timeout (fixed by #21) |
| Final gate (`541261b61`, `SKIP_API=1`: no `apps/api` change) | Deploy tests all pass; web turbo 43/43 (typecheck, lint, unit, builds incl. Flow bundle check); fake-API e2e 2141 passed / 17 skipped / 2 failed under load (Office voice-interface:1094 got a 503 from `/api/voice/confirm`, Office PWA got a 500 from `/v1/me`; machine load 29 on 11 cores) → both rerun 9/9 (3 repeats, desktop + mobile, load 4); storefront-design 21; voice-dev 12; stack 199 passed / 1 skipped |
| Flow Docker image (`next.Dockerfile APP=flow`, run with NO env) | Builds (first attempt hit a transient Debian mirror 404; rebuilt green); bundle check ran inside the image build: clean. 38/38 header checks: `/storefront/preview` and `?revision=<uuid>` send exactly one CSP with `frame-ancestors 'self'`, no ElevenLabs origins, `Cache-Control` no-store; `/`, `/login`, `/storefront`, `/storefront/preview/x`, `/storefront/previewx` send exactly one CSP with `frame-ancestors 'none'`; all have nonce + `strict-dynamic`, no `X-Frame-Options`; `/healthz` 200; `/api/voice/readiness` 502, `/api/voice/secure` 404; with `FILES_ORIGIN` set, `img-src` contains it on `/` and the preview |
| Office Docker image | Builds; `/healthz` 200; one CSP header, `frame-ancestors 'none'` (unchanged); voice audio files present |
| Privileged data caching | API sets `Cache-Control: no-store` on every `/v1` response (`apps/api/src/lib.rs`); preview HTML carries no draft data (fetched client-side with the bearer header) |
| Revision short code (found in screenshots) | Both apps showed `revision_id.slice(0, 8)`, the UUIDv7 timestamp head, so revisions minted within ~65 s shared one code. Now the random tail `slice(-8)` in Flow and Office; e2e red first ("00000002" missing), then Flow + Office storefront specs 34/34 (desktop + mobile) |
| Screenshots | 10 captured with the fake API (desktop 1280, tablet 834, mobile 375: empty conversation + sales chart; storefront review with preview in Mobile/Tablet/Desktop modes; full page at 375). Kept outside the repository (public) in the parent session's scratchpad |

Not verified: a live ElevenLabs session from a Flow origin (needs the agent's allowed origins + DNS); staging/production deployment (none performed). Mocked browser tests use a fake ElevenLabs socket and a mock Office upstream behind Flow's real proxy; the real-stack specs use the real API/Office/Flow with the assistant off.

## PR 2B — Flow app foundation (2026-11-10)

Branch `flow/2b-foundation` = child B (`e8c295cd0`, `f6b67e128`: opt-in voice texts, shared audio script), child A (`1ca1607ea`…`aa0f6471a`: apps/flow, auth, restricted voice proxy, e2e/stack), child C (`f257bea2d`…`2f45f4592`: compose, health hosts, images, docs), then parent `10b89d3dc` (Flow opts into voice texts + audio build; test red in all 3 locales first) and `4f0b75144` (spawn-test timeout).

| Check | Result |
| --- | --- |
| Child A | Flow vitest 50/50 (45 proxy incl. 5 deliberate-break checks, 5 readiness); Flow Playwright desktop + 375px 54/54; real-stack `flow-proxy.spec` 6/6 (readiness through Flow equals Office; session DELETE through Flow equals Office: bad token 401, foreign workspace 403, own 404). Stack spec caught a real bug: Office readiness answers 200 `{enabled:false}` when off; fixed red→green |
| Child B | i18n 20/20, voice 107/107, Office 615/615; Office `voice` texts sha256-pinned to `a432dec17`; no app gets `voice` by default; audio old vs new byte-identical (`08b6ef29…`, `ca6e162f…`, `3bea39a5…`) |
| Child C | 14/14 deploy test scripts; deploy/ci unittest 33 OK; `docker compose config` staging + production OK; Flow env exactly `API_URL`, `VOICE_UPSTREAM_URL`, no secrets with every overlay |
| Integration: Flow turbo tasks | 12/12; Flow `public/voice-audio` hashes equal Office's |
| `scripts/local-gate.sh` (`SKIP_API=1`: no `apps/api` file changed) | Deploy tests pass; fake-API e2e 7/7 tasks (incl. Flow projects); `storefront-design` 21; `voice-dev` 12; full stack 195 passed, 1 skipped. **Web step red once**: new `prepare-voice-audio.test.mjs` timed out (spawned node took 21 s under load vs 5 s default) → 30 s timeout (`4f0b75144`); then all web tasks 43/43 |
| `voice:check` | 491 operations, storefront design ops still `internal` |
| Flow Docker image (`next.Dockerfile APP=flow`), run with NO env | healthy; `/healthz` 200; `/` and `/login` 200; `/api/voice/readiness` 502 `{message}` (no upstream); unknown route 404, wrong method 405; `/voice-audio/*` served; log `voice proxy readiness not_configured` only |
| Flow CSP | `connect-src 'self' https://api.elevenlabs.io wss://api.elevenlabs.io`; `frame-ancestors 'none'`; nonce + `strict-dynamic`; same `cspProxy({ voice: true })` as Office |
| Office Docker image | Builds; `/app/apps/office/public/voice-audio/` holds all three audio files (shared script path works in the image) |

Not verified: a real voice session POST through Flow (needs paid ElevenLabs keys, `VOICE=1`); staging/production deployment (none performed; DNS for `staging-flow`/`flow` not created).

## PR 2A — shared voice package (2026-11-10)

Branch `flow/2a-shared-voice` (child commits `a0c7dcca5` move, `71a367267` injection, `87925f95a` guards; parent docs on top). Base `main` `b70b909a2`.

| Check | Result |
| --- | --- |
| Test parity across the move | Before: Office 61 files / 608 tests. After commit (a): Office 47/532 + voice 14/76 = 61/608 |
| New injection tests (child, red first) | Observed red: `SAFE_HREF` not exported; `./fetch` module missing; controls `href="/assistant"`; provider `audioBase` mismatch; results `resolveHref` + `statusLabel`; suggestions rights (`'weekly_sales,todays_orders'` vs `''`). Default-path tests stayed green throughout |
| `@genesy/voice` lint / typecheck / test | 0 / 0 / 20 files, 105 tests |
| Office lint / typecheck / test (alone) | 0 / 0 / 48 files, 608 tests |
| `voice:check`, voice schema check, `deploy/assistant-agent.test.sh` | Pass |
| `scripts/local-gate.sh` (`SKIP_API=1`: no `apps/api` file changed; CI runs API) | Deployment helper tests pass; fake-API e2e 6/6 tasks; `storefront-design` 21 passed; `voice-dev` 12 passed; full stack 189 passed, 1 skipped. **Web step red once**: `gap-triage.test.ts` `planHeal > skips one workspace's few gaps…` timed out at 5 s (6.9 s) under parallel turbo load. Untouched by 2A (no diff to the module or its imports); alone 27/27 three times; full Office suite alone 608/608 |
| Web tasks rerun to completion (turbo stopped after that failure) | 39/39 successful (26 cached) |
| Office voice e2e `voice.config.ts` desktop + mobile (child) | 1039 passed, 10 skipped, 1 failed: office-mobile `voice.spec.ts:489` (duplicate reference names/focus); rerun alone 5× per project: 10/10. Original error text not kept, so cause unconfirmed |
| Office + Shop Docker images (`deploy/images/next.Dockerfile`, full build) | Both exit 0; Shop client-bundle check "no storefront key". First attempt failed only on a Docker Hub frontend fetch timeout (`docker/dockerfile:1`), retried after pulling it |

Security: no new route, env read, header, CORS or CSP change. Provider defaults equal Office's previous literals (`/api/voice`, `/voice-audio`, identity `resolveHref`, `signOut("expired")`); `SAFE_HREF` unchanged; `@elevenlabs/react` 1.21.1 unchanged. Guards: `@genesy/voice/lib/*` stays plain TS; Office server code cannot import `@genesy/voice/components`.

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
