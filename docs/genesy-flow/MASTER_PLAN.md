# Genesy Flow master plan

Status: Phase 1 merged and staging-validated (flag false). Phase 2: 2A merged (#16), 2B merged (#17), 2C merged (#23, `dd8d4ad2f`). Phase 3: P3-A+B merged (#28); P3-C (Flow proposal preview) PR open; agent prompt block waits for the D9 staging validation. Phase 5 component demand built (SQL `assistant_gap_demand`, k ≥ 3 workspaces; Office `voice:demand`), merged (#25). Phases 4 (files only) and 6 planned; Phase 6 choices deferred. 2D proposed only. Updated 2026-11-10.

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
- `apps/flow` (PR 2B): sign-in, workspace, shell, `/healthz`, and the restricted voice proxy `app/api/voice/[route]` → `lib/voice-proxy.ts` (allowlisted routes/methods/headers, fixed `VOICE_UPSTREAM_URL`, bounded streaming, no retries). Voice texts: `@genesy/i18n/voice` (opt-in for Office and Flow); audio: `packages/voice/scripts/prepare-voice-audio.mjs <publicDir>`.
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

## Product direction (approved 2026-11-10; guides future planning, not scheduled work)

**"Genesy Flow — A business operating system that adapts to how you work."** Voice-first, adaptive business operating environment: value without learning navigation, building dashboards or configuring UI first.

Target experience:
1. Users tell Flow, in plain conversation, which information and workflows matter to them.
2. Flow saves explicit preferences across sessions.
3. Later sessions restore the right views and fetch current business data.
4. Flow composes existing approved UI components from validated data and presentation configurations.
5. Users refine saved views through conversation.
6. Users explicitly configure recurring reports, notifications and approved business routines.
7. Scheduled routines run independently of browser and voice sessions.
8. Users can see and control saved preferences, routines, permissions and approvals.
9. Existing Rust APIs stay authoritative for calculations, business data and operations.
10. Useful defaults even when the user has configured nothing.

Architectural distinctions:
- Conversational memory does not replace structured, durable preferences.
- Saved display preferences are separate from scheduled routines.
- Reporting is separate from business mutations.
- Reuse existing UI components; never generate arbitrary runtime React code.
- Never silently broaden permissions; never auto-start expensive voice sessions.
- Initial views stay fast and non-blocking.
- Creating or enabling recurring external actions needs explicit user confirmation.

Not part of this direction: a general app builder, drag-and-drop dashboards, autonomous business writes without approval, or changes to the Phase 1 storefront design architecture. It adds no scope to the current phases: no dynamic report composition, scheduling, cron or email automation is built until a later phase is planned and approved.

## Proposed Phase 2D: token-efficient personalized briefings (recorded 2026-11-10; not scheduled)

Architecture decision for later planning. Not part of 2C; nothing below is implemented.

Goal: personalized, live business briefings with zero routine LLM calls on page load.

1. AI interprets a new preference once.
2. A validated, structured briefing configuration is saved in PostgreSQL, scoped by workspace and user.
3. Later sessions load that configuration directly.
4. Current metrics resolve through authorized Rust APIs.
5. Results render through approved reusable React components.
6. Data refreshes independently of the agent.
7. AI runs only for new instructions or explanations.
8. Rendering a saved briefing never starts an ElevenLabs conversation.

Reuse, don't duplicate: `apps/api/src/modules/{analytics,attention,scheduled_reports}`, the job scheduler (`apps/api/src/jobs`), existing reporting services, the API client with React Query, and `@genesy/ui` components.

Caching: no Redis by default. Start with existing request caching, measure query cost, and add Redis only when measurements justify new infrastructure. Preserve workspace isolation, user authorization, metric correctness, the reporting timezone, currency semantics (integer cents), explicit freshness indicators and safe invalidation.

Components: a finite registry of approved metric cards, line/bar charts, attention summaries, report tables and action/review cards. Each declares its supported properties, allowed data source, valid filters, permission requirements, loading/error/empty states and responsive behavior. No AI-generated React or executable code.

Routines: reuse `scheduled_reports` before proposing any new cron or automation. Keep four things separate: saved views (rendered on open), background scheduled reports, notifications, and business-changing operations. A stored preference never authorizes future business mutations.

## Roadmap: proposed future work

| Phase | Deliverable | Prerequisites / acceptance |
| --- | --- | --- |
| 1 | Versioned home design, safe preview/publication/restore | Actual tests/reviews in ledger; legacy remains default |
| 2 | Dedicated `apps/flow` at `flow.genesy.app`, shared agent foundation and design tools, device preview UI | Green Phase 1; inventory voice module callers; preserve Office compatibility; independent-origin auth/proxy/CSP/deploy design; explicit tool review policy |
| 2D (proposed) | Token-efficient personalized briefings (see section above) | Approved plan; reuse analytics/attention/scheduled_reports; measured caching; component registry; no routine LLM calls on load |
| 3 | Conversational design and targeted version-aware edits | Shared orchestration + discoverable registry; previews/review cards; explicit publish approval; conflict/retry/undo tests |
| 4 | Screenshot/image/document and optional URL design references | Reuse existing attachment lifecycle; SSRF-safe isolated URL/browser boundary with DNS/redirect/resource controls; untrusted-content handling; original approved assets |
| 5 | Component demand extension of existing gap ledger | Structured classifications; privacy-safe aggregates, unique-business demand, workarounds/support/resolution; backward-compatible reports/retention . Built: SQL `assistant_gap_demand` (k≥3 floor, ≤90 days, no workspace data) + Office `voice:demand` remedies/resolution. Support signal deferred |
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
