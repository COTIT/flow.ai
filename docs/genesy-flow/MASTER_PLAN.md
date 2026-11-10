# Genesy Flow master plan

Status: Phase 1 merged and staging-validated (flag false). Phase 2 in progress: PR 2A (shared voice package) under review; 2B and 2C not started. Phases 2–6 are proposed future work. Updated 2026-11-10.

Read [CURRENT_HANDOFF.md](CURRENT_HANDOFF.md) before resuming. Actual check evidence belongs in [VERIFICATION.md](VERIFICATION.md); rationale belongs in [DECISIONS.md](DECISIONS.md).

## Instructions and approved scope

Read root `CLAUDE.md`, the user's supplied AGENTS rules, relevant per-app instructions,
`.claude/skills/genesy-360-api/SKILL.md` and its checklist before work. Office also has
`apps/office/AGENTS.md`; consult installed Next.js docs. Read
`docs/design/web-redesign/README.md` and the closest approved artboards before UI edits.
No root `AGENTS.md` existed during this implementation.

Caveman Ultra governs communication; Ponytail Full governs implementation. Use strict
TDD: observe the intended failure before implementation. Parent reviews every child
plan before edits; assign exclusive ownership and integrate generated artifacts serially.
Child plans state objective, scope, findings, read/change files, implementation, API/DB
impact, dependencies, security, compatibility, tests, risks and conflicts. Children cannot
merge or deploy. Supported inherited Codex agents were used; Opus 5.5/Auto was unavailable.

Phase 1 is home-only, with `hero` and `product_grid`. Owners and workspace admins may
edit, preview, publish and restore. Draft creation is API-driven. No editor, Flow app,
voice design tools, reference analysis, component-demand extension or coding worker.
Keep payment/auth rules, catalog caching and existing business behavior unchanged.

The full product remains voice/prompt-first with visual preview, never a drag-and-drop
builder. Flow's proposed domain is `flow.genesy.app`. Customer prompts propose validated
configuration; they must never generate and execute arbitrary production React code.

## Verified starting architecture

- Monorepo: pnpm/Turbo; Rust/Axum/PostgreSQL API; Next.js Shop and Office; generated
  OpenAPI client; shared `packages/ui`. Actual source takes precedence over stale docs.
- Shop catalog uses a server-only `STOREFRONT_APP_KEY`, whose workspace is authoritative.
  `catalog:read` gates catalog reads. Catalog requests have a 60-second Next data cache;
  orders/payment/passport requests already have their own freshness rules.
- Office uses an origin-local bearer session, same-origin `/v1` proxy, current workspace
  membership and backend permissions. There is no shared cross-subdomain session.
- ElevenLabs handles voice/text conversation. LangGraph persists review interrupts;
  signed bridge requests still require native staff authentication/authorization.
- CSP defaults to `frame-ancestors 'none'`. Phase 1 preview renders inside Office.
- Existing gap ledger accepts signed, redacted workspace-scoped writes, has 90-day
  retention and SQL reports; it is not a component-development service.
- Voice catalog generation classifies every OpenAPI operation. Rust independently
  inventories bearer operations, so frontend policy alone cannot exclude design writes.
- Initial `main` was `fbe1fcf15556c22257dddb6a2ce6d89b43cbcb7b`. It was pulled with
  `--ff-only`; implementation branch is `codex/flow-foundation`.

### Source map for the next parent

- `apps/office/lib/voice/`: `provider.ts`, `bridge.ts`, `server.ts`, `workflow.ts`,
  `checkpoint.ts`, `catalog.ts`, operation policy/authority/generated files and gap capture.
- `apps/office/app/api/voice/`: session admission/binding, references and bridge routes (server stays in Office).
- `packages/voice` (`@genesy/voice`, PR 2A): client engine `components/provider.tsx` (host props
  `apiBase`, `audioBase`, `resolveHref`, `onUnauthorized`), reusable cards/results/controls, and pure
  `lib/*` contracts (`client-contract`, `views`, `confirmation`, `capabilities`) plus client helpers.
  `apps/office/components/voice/` keeps Office-only UI: `conversation.tsx`, `secure-form.tsx`, `design-preview.tsx`.
- `apps/api/src/modules/assistant/`: authoritative sessions/actions, signed approval
  dispatch and gap ledger. `apps/api/src/http/assistant.rs` verifies bridge signatures.
- `docs/office-assistant-{plan,validation,latency}.md`, `deploy/ASSISTANT.md`: prior agent
  decisions, evidence and operator procedures. Verify their claims against current source.
- `apps/api/src/modules/storefront_design/`, `apps/api/docs/api/storefront-design.md`:
  Phase 1 model, registry, operations and detailed HTTP contract.
- `apps/shop/lib/storefront.ts`: existing cache contract. Separate `storefront-design.ts`
  owns published lookup; `components/legacy-home.tsx` retains original home.
- `packages/ui/src/components/storefront-page.tsx`: shared presentation;
  `apps/office/app/(app)/settings/storefront/page.tsx`: authorized review controller.
- `packages/e2e/STACK.md`, `playwright.stack.config.ts`, `storefront-design.config.ts`
  and focused `shop.config.ts`: test environments and data-ownership rules.

## Approved Phase 1 contract

Home only; hero + product_grid; API-authored drafts; owner/admin review, immutable revisions, explicit publish and restore-to-draft. Preserve catalog caching and existing routes/trace/commerce. Shared UI is presentation-only; app adapters own fetching/auth. Public reads follow published pointer only. New operations stay outside voice execution.

Canonical schema, fields, routes and migration behavior: `apps/api/docs/api/storefront-design.md`, Rust `storefront_design` DTO/service/repo and generated OpenAPI. Do not maintain a second schema here.

## Roadmap: proposed future work

| Phase | Deliverable | Prerequisites / acceptance |
| --- | --- | --- |
| 1 | Versioned home design, safe preview/publication/restore | Actual tests/reviews in ledger; legacy remains default |
| 2 | Dedicated `apps/flow` at `flow.genesy.app`, shared agent foundation and design tools, device preview UI | Green Phase 1; inventory voice module callers; preserve Office compatibility; independent-origin auth/proxy/CSP/deploy design; explicit tool review policy |
| 3 | Conversational design and targeted version-aware edits | Shared orchestration + discoverable registry; previews/review cards; explicit publish approval; conflict/retry/undo tests |
| 4 | Screenshot/image/document and optional URL design references | Reuse existing attachment lifecycle; SSRF-safe isolated URL/browser boundary with DNS/redirect/resource controls; untrusted-content handling; original approved assets |
| 5 | Component demand extension of existing gap ledger | Structured classifications; privacy-safe aggregates, unique-business demand, workarounds/support/resolution; backward-compatible reports/retention |
| 6 | Internal weekly component worker, at most one draft PR/cycle | Sanitized specifications; least-privilege isolated runner/API credential and spending limit; registry/schema/tests/responsive screenshots; human merge review; no production/customer transcript access or automatic merge |

No Phase 2–6 functionality is claimed by this Phase 1 delivery.

## Phase 2 continuation checklist

1. Read this handoff and governing instructions, then inspect current branch/CI/source.
2. Resolve any remaining Phase 1 verification failures before extracting agent code.
3. Inventory Office catalog/discovery, bridge, workflow/checkpoints, identity, attachment
   lifecycle, approval cards, result cards and callers. Shared code moves to packages;
   Office adapters and behavior remain tested during extraction.
   Retain ElevenLabs + LangGraph and existing charts/visual responses, review/result
   cards, temporary attachments, cleanup, loading/error/retry states and business tools.
4. Plan Flow's own sign-in/session lifecycle on its origin. Preserve native backend
   permission checks; never transfer bearers through URLs or broaden domain cookies.
5. Review each design API's tool effects, authorization, confirmation/idempotency and
   reconciliation before changing `internal` policies or Rust assistant denial.
6. Plan full device preview separately from Phase 1's body preview. Preserve catalog
   caching, preview isolation and explicit version-bound publication.
7. Assign child ownership after written plans; parent reviews before edits. Avoid
   combining extraction, auth/deployment and conversational behavior into one large PR.

For Phase 6, a dedicated Anthropic API credential is preferred if Claude Code is chosen;
a personal Claude Max subscription is not the unattended credential default. Choose
runner/vendor/budget only during that phase. Demand may justify a property, discovery
fix or clarification instead of a new component. Existing gap reports stay compatible.

## Rollout and remaining risk

Keep renderer flag false until a controlled staging workspace passes workflow checks.
Feature rollback is flag disable or restore followed by explicit publication. Leave
additive migration/history installed. Existing migration checksum guards may reject old
binary rollback; never bypass them or assume compatibility. No production deployment,
credentials, new vendor commitments or later-phase implementation are included.
