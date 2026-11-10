# Genesy Flow decisions

Updated 2026-11-10. Approved Phase 1 scope only. See MASTER_PLAN.md for roadmap and VERIFICATION.md for evidence.

| Decision | Why / rejected alternative |
| --- | --- |
| Home only, hero + product_grid | Small end-to-end foundation; no broad capability library now |
| Existing workspace_admin gate, owner override | User selected owner/admin; reject new permission/grant UI in Phase 1 |
| Rust schema/OpenAPI as type authority | Reuse backend trust boundary; reject independent TS design schema |
| Existing packages/ui presentation-only renderer | Avoid copied renderer/new package; fetching/auth/server dependencies remain in adapters |
| API-driven draft fixture + native Office review | User explicitly rejected visual/JSON editor; conversational authoring is later |
| Immutable revisions + page pointers + events | Keep published content separate; version/row lock prevents overwrites; restore clones draft |
| Existing keyed idempotency | Actor/payload-bound replay already exists; reject duplicate dedupe ledger |
| Audit UUID snapshots | Survive token cleanup; reject FK to expiring login token |
| Preview within Office | Reuse bearer auth; reject cross-domain token URLs/iframe/CSP relaxation |
| Separate uncached design loader | User explicitly requires existing 60s catalog cache unchanged |
| Exact legacy body + native TraceForm in design mode | Preserve routes/essential lookup/commerce; no third schema capability |
| Server-only default-false SHOP_DESIGN_ENABLED | Controlled rollout + immediate feature rollback without destructive migration |
| Dual voice exclusion | TS internal policy plus Rust resolver denial; no live design tools until later review |
| Original blocks with existing primitives | Keep restricted vendor animation blocks outside configurable capabilities |
| Four canonical docs under docs/genesy-flow | User continuity protocol; replace earlier single draft handoff, no extra service/tools |
| Stale published doc serves `data: null` + warn | Shop must fall back to legacy home, not error, if validation later tightens |
| DB size CHECK is a 64 KiB backstop | `jsonb::text` adds spaces; Rust compact 32 KiB check stays authoritative |
| Hero text has no content filter | Rendered as escaped React text; URLs/media rejected as fields; author is the workspace admin |
| Section cap 12 is effectively 5 | 1 hero + 4 grids; kept as the declared bound |
| 409 shows only the translated conflict notice; other errors keep `ApiError` | Matches every other Office page |
| Shop warns only on real failures, 2s timeout | Flag off and "nothing published" are normal states and stay silent |
| Plain `<a>` in the ui renderer; trace form below sections in design mode | ui package cannot import `next/link`; the form is composed by Shop outside the schema |
| Turbopack for local builds | Earlier webpack workaround came from a Codex-sandbox EPERM; not needed here |
| OpenAPI tag `storefront_design` | Operation ids must carry their tag prefix; repo uses underscores |
| Staging validation on `afm-demo`, flag back to false | User chose the seeded AFM data for realistic volume; the one staging Shop serves it. Published revision 3 stays stored but hidden; turning the flag on later shows it until a new design is published |
| 2A extracts the voice client engine + pure contracts into `@genesy/voice`; server stays in Office | Flow reuse needs the engine and cards. Voice routes check only the bearer (no origin/cookie), bridge signing has no origin, checkpoints are keyed by session id, so Flow can reuse the server unchanged |
| 2B plan: Flow's server proxies `/api/voice/*` to Office over the internal network | One agent engine; ElevenLabs and bridge secrets stay in one container; the checkpoint role's 10-connection budget is unchanged; no CORS. Full server extraction waits until the Office assistant is retired |
| `voice` translations stay in Office for 2A | Shared components keep `useTranslations("voice")`; the namespace moves to `packages/i18n` messages when Flow exists, keeping 2A behavior-identical |
| Host injection with Office defaults (`apiBase`, `audioBase`, `resolveHref`, `onUnauthorized`, `SAFE_HREF`, `openHref`, rights, `statusLabel`) | Smallest seam a second origin needs; Office passes nothing and behaves as before |
| Purity guards | `@genesy/voice/lib/*` must stay plain TS (vitest guard); Office server code may not import `@genesy/voice/components` (vitest guard) |
| Flow sign-in is origin-local | Flow uses the existing API login and the shared session store; its `localStorage` session is separate from Office's. Users sign in to each app. No SSO, no shared cookies, no tokens in URLs, no auth-policy change |
| Flow voice proxy is an allowlist, not a reverse proxy | Only readiness GET; session POST/DELETE; tools/batch/confirm POST; attachments POST/DELETE. Fixed runtime `VOICE_UPSTREAM_URL` origin (compose `http://office:3000`); request headers limited to authorization, content-type/length, accept, x-workspace-id, x-voice-session, x-file-name; response headers content-type, cache-control, retry-after; Office's 10 MiB + 64 KiB cap enforced while streaming; manual redirects; per-route timeouts (confirm 600 s, runs batches); no retries; generic `{message}` 502/504 |
| `secure` route and upstream Set-Cookie not proxied | `secure` serves only Office's own secure form; the only Set-Cookie is Office's gap cookie, read only by Office pages |
| Flow availability depends on Office | Accepted for Phase 2: one voice engine, secrets and checkpoint pool stay in Office. Flow carries no ElevenLabs, bridge, checkpoint or LangSmith secret |
| `voice` texts opt-in (`@genesy/i18n/voice`) | Shop, Till, landing never receive assistant texts in their HTML; Office's texts pinned by sha256 to `a432dec17` |
| Flow image has no `-staging` suffix | Its build inputs do not differ per environment (no env-specific `NEXT_PUBLIC_*`); one image serves both |
| Phase 4 references: files only, URLs off (user, 2026-11-10) | Reuses the existing attachment pipeline (sniff, 10 MiB, quota, cleanup); no fetcher, browser container or vendor. Revisit URLs only if Phase 5 demand shows the need |
| Phase 5 demand aggregates need >= 3 distinct workspaces, hard floor (user, 2026-11-10) | No single business identifiable; Phase 6 reads only these totals, never transcripts |
| Phase 6 runner, model and budget: decide later (user, 2026-11-10) | Waits for Phase 5 data. Worker credentials must never be stored in the repository or GitHub; keep them in a separate safe store, chosen when Phase 6 is planned |
| Repository stays public with `main` unprotected (user, 2026-11-10) | User's choice. Phase 6 plan noted the missing rulesets; any automated worker must not depend on GitHub-held secrets |
| Proposed 2D briefings: AI interprets a preference once, a structured config lives in PostgreSQL, and data comes from Rust APIs into approved components | Zero routine LLM/ElevenLabs calls on load; reuse analytics/attention/scheduled_reports; no Redis until measured; stored preferences never authorize mutations. Future guidance only, not 2C scope |

Approved scope additions: persistent roadmap/handoff/evidence/decision checkpoints; necessary static `storefront` segment in existing privacy-safe gap path classifier. No Phase 2 implementation.

Landing prerender failure resolved: it came only from the forced-webpack shim. No product preference requires user input. Production deployment and old-binary rollback compatibility remain unapproved/unverified; keep checksum guard intact.

Agent switching: finish atomic operation, update all four docs, inspect Git/process/migration state. Never reset --hard, clean user work, force-push, merge or deploy automatically. No push is authorized by this continuity protocol. Use feature-branch checkpoint commit only when honest and useful. Different machine/worktree needs explicit authorized Git synchronization; uncommitted files are not remotely available.
