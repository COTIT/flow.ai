# CLAUDE.md — genesy

One Rust backend (`apps/api`) and four Next.js apps (`apps/shop`, `apps/office`, `apps/field`, `apps/till`) in a pnpm + Turborepo monorepo. See `README.md` for the folder map and commands.

## Working rules

- Plain English. Attach an analogy when a technical term is needed.
- Before implementing: a short step-by-step plan, then wait for approval. One step at a time; explain what changed and why after each step.
- Strict TDD: failing test first, then the least code to pass. Never report done on a red check.
- Keep code, paths, commands and error strings exact.
- No new dependency without saying why and getting approval.

## Product rules

- Money is exact: integer cents end to end, no floats. Farmer passbook lines are append-only.
- An account (workspace) is the privacy boundary. Every query on account data filters by `workspace_id`, and a test proves another account is refused.
- Sharing between accounts happens only through explicit supply links, with farmer consent.
- Business rules live in `apps/api`. Frontend apps display and collect data; they never re-implement rules.
- Pre-release (since 2026-11-14): no real users on staging or production yet. Ship features on. Don't add feature flags, admission switches or staged-rollout gates. Revisit at the first stable release with users.

## Per area

- `apps/api`: invoke the `genesy-360-api` skill (`.claude/skills/genesy-360-api/SKILL.md`) before any action.
- Frontend apps: React + Next.js, Tailwind, shadcn/ui, TanStack Query, Zustand, axios. Shared code goes in `packages/*`, not copied between apps.
- Web changes are verified by headless Playwright, not by a person clicking through. Every user-facing change adds or updates a Playwright test (desktop + 375px, no console errors, no failed requests), and the e2e run must be green before reporting done. CI runs the same suite.

## Old repo

`/Users/cotit/Documents/cotit/genesy-360` is reference only. Its `apps/genesy-360-api` is frozen; all API work happens here.
