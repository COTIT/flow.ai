# Genesy Flow current handoff

Checkpoint: 2026-11-10. **Phase 1 implemented, final tests/review incomplete. Do not start Phase 2.**

Read repository instructions → [MASTER_PLAN.md](MASTER_PLAN.md) → this checkpoint → actual Git/source → [VERIFICATION.md](VERIFICATION.md) and [DECISIONS.md](DECISIONS.md).

## Git and ownership

- Branch: `codex/flow-foundation`. Base and last known HEAD: `fbe1fcf15556c22257dddb6a2ce6d89b43cbcb7b`.
- No implementation commit, push, PR, merge or deployment yet. Changes are local and uncommitted.
- Root parent owns generated TS/voice artifacts, configs, full-stack test, docs and integration. API child owns Rust/SQL + API tests/docs; Shop child finished UI/Shop then read-only Rust review; Office child owns Office caller fix/tests and validation.
- User approved routine efficient/safe decisions autonomously; stop only for critical unresolved issue. Remain strictly Phase 1.

## Verified completed milestones

- API schema/registry, relational identities, immutable JSONB revisions/events, owner/admin routes, public published-only API, version/idempotency/publish/restore workflow; focused API8 + schema2 green.
- Pure shared two-block renderer; separate uncached Shop design loader; original catalog loader unchanged; native trace and exact legacy fallback retained.
- Office review/preview/publish/restore, owner/admin nav/route, translated states, stale-version and workspace/logout guards.
- New browser18 + legacy Shop248 green; Office design14 + voice72 green. Full results/limits in VERIFICATION.md.
- Eight operations excluded by both voice policy and Rust; generated client/catalog present. Authority hashes need final refresh after API formatter.
- Default-off deployment flag, reproducible JSON fixture, new browser CI/local gate wiring, four-document continuity checkpoint.

## Exact unfinished tasks / next executable action

1. Inspect active full-stack run session12630 / `packages/e2e/.stack/120/logs/`. First run session52069 failed unrelated landing webpack URL prerender. Both an installed "node@22" path and default report Node23.3; do not misreport runtime. Ephemeral npm Node22 attempt session1969 may still exist. No landing edits.
2. Finish full Rust gates (API child active): full suite, fmt, Clippy, SQLx check, OpenAPI freshness. Record actual results. New migration `20261109191001_storefront_design` up/down/up passed; no partially applied migration expected.
3. Refresh only reviewed source authority hashes, run voice:generate/check; final DTO formatting changed hash. Do not bypass freshness gate. Then elevated full Office unit rerun; static storefront segment fix already approved.
4. Run real-stack `tests-stack/storefront-design.spec.ts` desktop/mobile against real services. Each test owns workspace/key/Shop subprocess, leaves shared seed untouched. Fix only Phase 1 issues.
5. Finish independent Rust/security reviews, resolve findings and rerun affected checks. No confirmed security finding so far.
6. Update these docs at each milestone; replace pending results and refresh exact inventory/Git state. Run prose cleanup and final diff review. Do not declare Phase 1 complete on red/unchecked required gates.

## Resume commands

```sh
git status --short --untracked-files=all
git log -3 --oneline
# Inspect only dedicated local test state; never reset/reseed from a spec.
# Host/API test PG uses test-only localhost55460; read VERIFICATION.md before running gates.
pnpm --filter @genesy/office voice:check
pnpm --filter @genesy/e2e exec playwright test -c storefront-design.config.ts
pnpm --filter @genesy/e2e exec playwright test -c shop.config.ts --workers=3
E2E_STACK_OFFSET=120 pnpm --filter @genesy/e2e exec playwright test -c playwright.stack.config.ts tests-stack/storefront-design.spec.ts --output=test-results-flow-stack
```

Full-stack startup currently uses a temporary builder CLI shim; see VERIFICATION.md. Do not blindly rerun `scripts/local-gate.sh`: its cleanup prunes Docker builders/cargo target. Run required equivalent checks against disposable resources.

## Working tree snapshot

No staging was performed at this checkpoint. `M` below is unstaged tracked work; `??` is untracked. Snapshot generated before writing this file; run actual git status before takeover.

```text
 M .github/workflows/ci.yml
 M README.md
 M apps/api/CHANGELOG.md
 M apps/api/openapi.json
 M apps/api/src/modules/assistant/service.rs
 M apps/api/src/modules/mod.rs
 M apps/api/src/modules/storefront/handlers.rs
 M apps/api/src/modules/storefront/mod.rs
 M apps/api/src/openapi.rs
 M apps/api/tests/api/main.rs
 M apps/office/components/shell.tsx
 M apps/office/lib/routes.test.ts
 M apps/office/lib/routes.ts
 M apps/office/lib/voice/catalog.generated.json
 M apps/office/lib/voice/catalog.schemas.generated.json
 M apps/office/lib/voice/gap-follow-up.ts
 M apps/office/lib/voice/gaps.test.ts
 M apps/office/lib/voice/operation-policy.authorities.json
 M apps/office/lib/voice/operation-policy.json
 M apps/office/lib/voice/operation-policy.test.ts
 M apps/office/messages/en.json
 M apps/office/messages/nl.json
 M apps/office/messages/sw.json
 M apps/shop/.env.example
 M apps/shop/app/page.tsx
 M apps/shop/lib/price.ts
 M deploy/compose.test.sh
 M deploy/production/.env.production.example
 M deploy/production/docker-compose.production.yml
 M deploy/staging/.env.staging.example
 M deploy/staging/docker-compose.staging.yml
 M packages/api-client/src/generated/index.ts
 M packages/api-client/src/generated/model/index.ts
 M packages/e2e/mock/server.mjs
 M scripts/local-gate.sh
 M turbo.json
?? apps/api/.sqlx/query-2f7c3f9638eb35c9acdf0c74d963f4c3a12108692cb604bfcfedf5f3b46d9f00.json
?? apps/api/.sqlx/query-40c2f6c4c27615faa2231af23afd1035508a31d48fc1881dd8aa49d57627e724.json
?? apps/api/.sqlx/query-5277a83f0bff5f1771be55b7cdccbd616e3c523d910095a1cebe4e026951ec14.json
?? apps/api/.sqlx/query-57a23e2c19506f266d45c4a952eb9b8515c1dacec238218a99836a445f452914.json
?? apps/api/.sqlx/query-587b98e96c51febfbfbbde827024eaab451bc43f5ba0e83784e675fceb22d60b.json
?? apps/api/.sqlx/query-6a0423af0cfccc3f78b931b67c97cb23367743b3e123c5bd65ef5899ce36e492.json
?? apps/api/.sqlx/query-8b3796818fcd472f63c33f31ce6ddc9f4ed27dced1c943d1ec52af5c0f6f0749.json
?? apps/api/.sqlx/query-d5be9e8557535d36112630ee591ce0b04b23e30a839cbf686a0e311505ffe4fd.json
?? apps/api/.sqlx/query-eadba8ae1e03bc34df700b1dacd89a2f029392b72a8ffbd71249adb11be1bf0a.json
?? apps/api/.sqlx/query-f48d86ec09b57dc905bb03b78554ee7e5ee41e17100d0dc44209926b696ba769.json
?? apps/api/.sqlx/query-fb0a55a6ee682db0d30aff53326fbeca45925e9fd15befcc4f48db6e19819c8a.json
?? apps/api/.sqlx/query-fd7cfa15eafc577d9ce248c7c870f404093a4c843fdf068c2a172c38fb0e1207.json
?? apps/api/docs/api/storefront-design.md
?? apps/api/migrations/20261109191001_storefront_design.down.sql
?? apps/api/migrations/20261109191001_storefront_design.up.sql
?? apps/api/src/modules/storefront/service.rs
?? apps/api/src/modules/storefront_design/dto.rs
?? apps/api/src/modules/storefront_design/handlers.rs
?? apps/api/src/modules/storefront_design/mod.rs
?? apps/api/src/modules/storefront_design/repo.rs
?? apps/api/src/modules/storefront_design/routes.rs
?? apps/api/src/modules/storefront_design/service.rs
?? apps/api/tests/api/storefront_design.rs
?? apps/api/tests/fixtures/storefront-design-v1.json
?? apps/office/app/(app)/settings/storefront/page.tsx
?? apps/shop/app/page.test.tsx
?? apps/shop/components/legacy-home.tsx
?? apps/shop/lib/storefront-design.test.ts
?? apps/shop/lib/storefront-design.ts
?? docs/genesy-flow-handoff.md
?? docs/genesy-flow/DECISIONS.md
?? docs/genesy-flow/MASTER_PLAN.md
?? docs/genesy-flow/VERIFICATION.md
?? packages/api-client/src/generated/model/envelopeOptionStorefrontPublishedDesign.ts
?? packages/api-client/src/generated/model/envelopeStorefrontDesignPreview.ts
?? packages/api-client/src/generated/model/envelopeStorefrontDesignState.ts
?? packages/api-client/src/generated/model/paginatedStorefrontDesignRevisionSummary.ts
?? packages/api-client/src/generated/model/storefrontDesignCollection.ts
?? packages/api-client/src/generated/model/storefrontDesignColumns.ts
?? packages/api-client/src/generated/model/storefrontDesignDocument.ts
?? packages/api-client/src/generated/model/storefrontDesignPage.ts
?? packages/api-client/src/generated/model/storefrontDesignPreview.ts
?? packages/api-client/src/generated/model/storefrontDesignPreviewParams.ts
?? packages/api-client/src/generated/model/storefrontDesignRevision.ts
?? packages/api-client/src/generated/model/storefrontDesignRevisionSummary.ts
?? packages/api-client/src/generated/model/storefrontDesignRevisionsParams.ts
?? packages/api-client/src/generated/model/storefrontDesignSave.ts
?? packages/api-client/src/generated/model/storefrontDesignSection.ts
?? packages/api-client/src/generated/model/storefrontDesignSelect.ts
?? packages/api-client/src/generated/model/storefrontDesignSource.ts
?? packages/api-client/src/generated/model/storefrontDesignState.ts
?? packages/api-client/src/generated/model/storefrontDesignTheme.ts
?? packages/api-client/src/generated/model/storefrontHeroLayout.ts
?? packages/api-client/src/generated/model/storefrontHeroProps.ts
?? packages/api-client/src/generated/model/storefrontProductGridProps.ts
?? packages/api-client/src/generated/model/storefrontPublishedDesign.ts
?? packages/api-client/src/generated/storefront-design/storefront-design.ts
?? packages/e2e/shop.config.ts
?? packages/e2e/storefront-design.config.ts
?? packages/e2e/tests-stack/storefront-design.spec.ts
?? packages/e2e/tests/office/storefront.spec.ts
?? packages/e2e/tests/shop-design/home.spec.ts
?? packages/ui/src/components/storefront-page.test.tsx
?? packages/ui/src/components/storefront-page.tsx
?? packages/ui/src/lib/storefront-design.ts
?? packages/ui/src/lib/storefront-price.ts
```

Safe takeover: repository has no merge/rebase conflict; implementation is uncommitted and required verification is incomplete. Observe/finish active operations before another agent edits. Another machine cannot access these local changes until explicitly authorized synchronization.
