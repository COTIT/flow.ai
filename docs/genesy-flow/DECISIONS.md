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
| Webpack for local verification | Turbopack worker blocked by environment; tracked build config unchanged |
| Four canonical docs under docs/genesy-flow | User continuity protocol; replace earlier single draft handoff, no extra service/tools |

Approved scope additions: persistent roadmap/handoff/evidence/decision checkpoints; necessary static `storefront` segment in existing privacy-safe gap path classifier. No Phase 2 implementation.

Unresolved engineering blocker: real-stack harness landing prerender failure, being investigated without touching landing source. No product preference requires user input. Production deployment and old-binary rollback compatibility remain unapproved/unverified; keep checksum guard intact.

Agent switching: finish atomic operation, update all four docs, inspect Git/process/migration state. Never reset --hard, clean user work, force-push, merge or deploy automatically. No push is authorized by this continuity protocol. Use feature-branch checkpoint commit only when honest and useful. Different machine/worktree needs explicit authorized Git synchronization; uncommitted files are not remotely available.
