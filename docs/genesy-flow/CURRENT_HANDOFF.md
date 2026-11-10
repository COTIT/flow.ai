# Genesy Flow current handoff

Checkpoint: 2026-11-10. **Phase 1 merged and staging-validated (flag false). Phase 2: 2A merged (#16), 2B merged (#17, `4d4f8b455`); follow-ups #18 (Office proxy skips `api/`, voice uploads no longer cut at 10 MiB) and #19 (Shop payment-wait test flake) merged. 2C not started. Product direction recorded in MASTER_PLAN.md.**

Read repository instructions → [MASTER_PLAN.md](MASTER_PLAN.md) → this checkpoint → actual Git/source → [VERIFICATION.md](VERIFICATION.md) and [DECISIONS.md](DECISIONS.md).

## Git

- `main` includes Phase 1 (COTIT/genesy#13, merge `e041285e7`) and the shop image fix (COTIT/genesy#14, merge `dd6678bfe`).
- Staging evidence merged in COTIT/genesy#15 (`b70b909a2`).
- PR 2A merged (#16, `a432dec17`). `packages/voice` (`@genesy/voice`) holds the voice client engine (`components/provider.tsx`), reusable cards/results/controls and pure `lib/*` contracts; Office keeps its conversation screen, secure form, voice pages, all server code (`lib/voice` server modules, `app/api/voice/*`), generated catalog/policy and translations. See DECISIONS.md for the boundary and VERIFICATION.md for evidence.

## Staging (2026-11-10)

- Runs `dd6678bfe` (API, Office, Shop, Till, Field, landing). Schema `20261109191001`. Previous: `fbe1fcf15` / `20261109130000`, with a pre-deploy dump recorded in `.deploy-state`.
- One Shop instance; its key serves `afm-demo`. Validation used that workspace (user-approved): draft → Office preview → publish → flag on → responsive checks → second revision → restore → republish → failure/isolation checks → flag restored to false. All passed; details and evidence in VERIFICATION.md.
- afm-demo now holds 3 revisions; revision 3 (fixture content) is published but hidden while the flag is off.
- Production: untouched (`fbe1fcf15`, no design migration).

## Known gaps

- `scripts/local-gate.sh` and PR CI build no Docker images; the image build runs only on `main`. Build `deploy/images/next.Dockerfile` locally when a change could touch the image context (that is how the #14 defect slipped through).
- Staging API logs no request paths, so "flag off → no design request" is proven by unit test, not runtime logs.
- Binary rollback past the design migration needs `--restore-db` or `--compatible-assistant`; never bypass the schema guard.

## PR 2B (merged, #17)

- `apps/flow`: own Next app (port 3005 dev), sign-in via the shared login form and API login, origin-local session (separate from Office; sign in per app), workspace choose/switch, assistant-first shell, `/healthz`, landing with user, workspace and assistant readiness.
- Voice proxy `apps/flow/app/api/voice/[route]` → `lib/voice-proxy.ts`: allowlisted routes/methods/headers to fixed runtime `VOICE_UPSTREAM_URL` (compose `http://office:3000`; dev `http://localhost:3001`). Details in DECISIONS.md.
- Voice texts opt-in via `@genesy/i18n/voice` (Office + Flow). Audio via `packages/voice/scripts/prepare-voice-audio.mjs public` in both builds.
- Deploy kit: `flow` service in staging/production compose (internal + edge), Traefik hosts `staging-flow.genesy.app` / `flow.genesy.app`, health hosts, `ghcr.io/cotit/genesy-flow` built by CI after green main.

## Before the first deploy that includes Flow (user actions)

- DNS A/AAAA for `staging-flow` and `flow` (values in the staging README table). Without them Let's Encrypt fails for that host and the watch alert fires each run.
- After the first green main promote: set GHCR package `genesy-flow` private with Actions Write, or the VPS pull fails (the deploy stops before backup/migrate).
- A rollback to a commit before Flow leaves the flow container running: `deploy/deploy.sh <env> compose stop flow`.
- Memory: +384M staging, +512M production limits.

## Known issues

- Flow voice depends on Office's internal voice service (accepted for Phase 2).

## Next

1. PR 2C (needs the user's go): add `flow.genesy.app` and `staging-flow.genesy.app` to the ElevenLabs agent's allowed origins; mount `GenesyVoiceProvider` in Flow (`apiBase` default works through the proxy; `resolveHref` maps Office-relative links to the Office origin); two-pane conversation UI from `useVoice()` + shared cards; storefront revision preview reusing `StorefrontPage` and the Phase 1 API (design ops stay `internal` for voice).
2. Optional: decide whether staging keeps the shop design flag on for AFM demos (would show revision 3 until a new design is published).
3. Production rollout of the design flag is a separate, later decision.
4. Later phases follow the product direction in MASTER_PLAN.md once planned and approved.

## Working rules

- Children: Auto, Opus 5.5, high effort, own worktree, plan first; parent approves before edits; children commit locally, never push or deploy.
- Ask the user only for scope, auth/permission/payment/business rules, destructive DB ops, production/credentials/vendors/cost, security exceptions.
- SQLx macros read `GENESY_DATABASE_URL`; `.env` points it at the shared dev DB on 54320. Point it at a disposable DB explicitly when preparing queries.
- `scripts/local-gate.sh` cleanup runs `cargo clean` and `docker builder prune -a`; expect a cold rebuild afterwards.
