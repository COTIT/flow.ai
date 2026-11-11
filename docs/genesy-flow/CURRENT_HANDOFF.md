# Genesy Flow current handoff

Checkpoint: 2026-11-10. **Phase 1 merged and staging-validated (flag false). Phase 2: 2A (#16) and 2B (#17) merged; follow-ups #18, #19, #21 (gap-triage test speedup) merged; main at `a465a7697`. 2C integrated on `flow/2c-conversation` (see "PR 2C"), PR next; merge when CI is green (standing user authorization). Phase 3 approved, P3-A (Rust) in progress; Phase 5 in progress. Phases 4/6 planned; 2D proposed. Nothing deployed beyond Phase 1 staging.**

Read repository instructions → [MASTER_PLAN.md](MASTER_PLAN.md) → this checkpoint → actual Git/source → [VERIFICATION.md](VERIFICATION.md) and [DECISIONS.md](DECISIONS.md).

## Git

- `main` includes Phase 1 (COTIT/genesy#13, merge `e041285e7`) and the shop image fix (COTIT/genesy#14, merge `dd6678bfe`).
- Staging evidence merged in COTIT/genesy#15 (`b70b909a2`).
- PR 2A merged (#16, `a432dec17`). `packages/voice` (`@genesy/voice`) holds the voice client engine (`components/provider.tsx`), reusable cards/results/controls and pure `lib/*` contracts; Office keeps its conversation screen, secure form, voice pages, all server code (`lib/voice` server modules, `app/api/voice/*`), generated catalog/policy and translations. See DECISIONS.md for the boundary and VERIFICATION.md for evidence.

## Staging (checked 2026-11-11, read-only)

- Runs `76d32c1ad` at schema `20261110060000`: a Codex payments branch (`codex/globadyme-payments` / `codex/shop-connections-payment-environments`, Globadyme gateway migration not in main), deployed 2026-11-10 12:05 UTC. Before that: Phase 1 `dd6678bfe`.
- **Flow / Phase 2–5 staging deploy waits until that payments branch is merged to main** (user decision 2026-11-11). Deploying main earlier would hit an unknown applied migration and drop the payments test state.
- Prerequisites ready in code: `flow` service, Traefik router + `TRAEFIK_CERTRESOLVER` (Let's Encrypt HTTP challenge retries until DNS resolves), `OFFICE_PUBLIC_URL`, health hosts (`deploy/ops/lib.sh`), rollback-compatible Phase 5 migration (#27).
- User-only before/around that deploy: DNS A `staging-flow` → `185.77.96.44` (not set on 2026-11-11), GHCR `genesy-flow` access check, ElevenLabs allowed origin `https://staging-flow.genesy.app`. Then the D9 validation run adds the Phase 3/4A prompt blocks and runs the live probe.
- First Flow deploy: the health step uses the old checkout's host list, so probe `staging-flow` by hand; `vps-watch` covers it only after the ops kit is reinstalled from a checkout with Flow.
- Production: untouched.

## Known gaps

- `scripts/local-gate.sh` and PR CI build no Docker images; the image build runs only on `main`. Build `deploy/images/next.Dockerfile` locally when a change could touch the image context (that is how the #14 defect slipped through).
- Staging API logs no request paths, so "flag off → no design request" is proven by unit test, not runtime logs.
- Binary rollback past the design migration needs `--restore-db` or `--compatible-assistant`; never bypass the schema guard.

## PR 2B (merged, #17)

- `apps/flow`: own Next app (port 3005 dev), sign-in via the shared login form and API login, origin-local session (separate from Office; sign in per app), workspace choose/switch, assistant-first shell, `/healthz`, landing with user, workspace and assistant readiness.
- Voice proxy `apps/flow/app/api/voice/[route]` → `lib/voice-proxy.ts`: allowlisted routes/methods/headers to fixed runtime `VOICE_UPSTREAM_URL` (compose `http://office:3000`; dev `http://localhost:3001`). Details in DECISIONS.md.
- Voice texts opt-in via `@genesy/i18n/voice` (Office + Flow). Audio via `packages/voice/scripts/prepare-voice-audio.mjs public` in both builds.
- Deploy kit: `flow` service in staging/production compose (internal + edge), Traefik hosts `staging-flow.genesy.app` / `flow.genesy.app`, health hosts, `ghcr.io/cotit/genesy-flow` built by CI after green main.

## PR 2C (integrated, `flow/2c-conversation`)

- Conversation-first Flow `/`: `GenesyVoiceProvider` in `apps/flow/components/signed-in.tsx` (`(app)` only); two panes from 1024 px (conversation left, contextual results right), results sheet with a persistent "Review pending" reopen control below. Shared transcript/review/attachments/composer now live in `@genesy/voice` and Office uses them too. Nothing starts on load.
- Office handoff: `apps/flow/lib/links.ts` exact-match policy to `OFFICE_PUBLIC_URL` (runtime, per environment); secure forms stay in Office (proxy unchanged).
- Storefront: Flow `/storefront` (admin-gated review: revisions, badges, separate publish/restore dialogs, 409 handling, device toggle 375/834/1280) + same-origin frame `/storefront/preview` (only route with `frame-ancestors 'self'`). `useStorefrontReview` shared from `@genesy/api-client` with Office.
- Edge cases: cross-tab session follow (`apps/flow/lib/other-tabs.ts`), page heading outline in the frame (`StorefrontPage headings="page"`).
- Build: `apps/flow/scripts/check-client-bundle.mjs` runs after `next build` (also inside the image). `FILES_ORIGIN` in Flow CSP via the uploads overlays.
- Design ops remain `internal` for voice in 2C.

## Parallel work in flight (child sessions, own worktrees)

- Phase 3 merged (#28, #30). Open: agent prompt `[Storefront design]` block (D9 staging validation). Open item: agent prompt `[Storefront design]` block waits for the D9 staging validation run.
- Phase 4A (files only): PR `flow/p4a` (probe scenarios + grader, schema guard, attachment refusal on design writes, Flow e2e). Open: `[Design references]` prompt rule + live probe in the D9 run (drafts in the shared notes, not in the public repo).
- Phase 5 component demand: see "Phase 5" below.
- Phase 4/6 plans: worktree `.claude/worktrees/flow-p46-plan-d8481a/side-session-notes/` (not committed).

## Phase 5 (component demand)

Demand export: `psql -At -v demand=1 -v days=7 -f apps/api/scripts/voice-gaps.sql | pnpm --filter @genesy/office voice:demand -- --days 7 --out demand` → `demand.json` + `demand.md`. Thresholded aggregates only (≥ 3 distinct workspaces, ≤ 90 days, no workspace ids); per-row triage input is refused. Migration `20261110120000_assistant_gap_demand` is additive (function only). Phase 6 decides the access path (DB role or operator export) for the worker.

## Before the first deploy that includes Flow (user actions)

- DNS A/AAAA for `staging-flow` and `flow` (values in the staging README table). Without them Let's Encrypt fails for that host and the watch alert fires each run.
- After the first green main promote: set GHCR package `genesy-flow` private with Actions Write, or the VPS pull fails (the deploy stops before backup/migrate).
- A rollback to a commit before Flow leaves the flow container running: `deploy/deploy.sh <env> compose stop flow`.
- Memory: +384M staging, +512M production limits.
- Add `https://flow.genesy.app` and `https://staging-flow.genesy.app` to the ElevenLabs agent's allowed origins (live voice on Flow is unverified until then).

## Known issues

- Flow voice depends on Office's internal voice service (accepted for Phase 2).

## Next

1. Open the 2C PR from `flow/2c-conversation`; merge when every check is green; then remove the 2C child worktrees/branches.
2. Phase 3: integrate P3-A, then P3-B (policy flip, resolver, prompt) and P3-C (Flow UI; Claude Design approval first).
3. Phase 5: integrate as its own PR off main.
4. Deploy (staging, then production) needs the user's go plus the prerequisites above. Staging design flag decision stays open.

## Working rules

- Children: Auto, Opus 5.5, high effort, own worktree, plan first; parent approves before edits; children commit locally, never push or deploy; children run only related tests, the parent runs the full gate on the merged tip.
- UI/UX design decisions: Claude Design artifact from the real JSX, user approval before implementation.
- Merge PRs once all CI checks are green (user standing authorization, 2026-11-10); deployments still need an explicit go.
- Ask the user only for scope, auth/permission/payment/business rules, destructive DB ops, production/credentials/vendors/cost, security exceptions.
- SQLx macros read `GENESY_DATABASE_URL`; `.env` points it at the shared dev DB on 54320. Point it at a disposable DB explicitly when preparing queries.
- `scripts/local-gate.sh` cleanup runs `cargo clean` and `docker builder prune -a`; expect a cold rebuild afterwards.
