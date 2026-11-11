# Flow personas

Simulated users who try to break Flow. Each spec here is one persona: a long, seeded random walk
over realistic actions, with invariants checked after every step. SIMULATED backend: the mocked
`/v1` API (`FakeApi`) and the mocked voice agent behind Flow's real proxy. No live ElevenLabs or Rust.

## Run one persona

```bash
pnpm --filter @genesy/flow build
cd packages/e2e
PERSONA_SEED=12345 npx playwright test personas/power-user            # all Flow widths
PERSONA_SEED=12345 npx playwright test personas/power-user --project flow-mobile
```

| env | meaning | default |
|---|---|---|
| `PERSONA_SEED` | replays one walk exactly | `1` in CI, random locally (printed) |
| `PERSONA_STEPS` | steps per session | `40` |
| `PERSONA_BUDGET_MS` | time budget per session | `90000` |
| `E2E_PORT_OFFSET` | ports, when another stack uses 3205/8199 | `0` |

Try many seeds locally, e.g. `for s in 7 99 2024; do PERSONA_SEED=$s PERSONA_STEPS=80 npx playwright test personas/<name>; done`.

## Write a persona

```ts
import { test } from "./harness"

test("double-clicker: ...", async ({ persona, page, api }) => {
  await page.goto("/")
  await persona.run([
    { name: "approve twice", weight: 3, run: () => persona.click(approveButton, "double") },
    // ...
  ])
})
```

- `persona` gives seeded randomness (`chance`, `int`, `pick`) and actions that use the UI as a
  person would: `click` (single/double/triple), `clickAnything`, `say`, `startText`, `backHome`,
  `keys`, `tab`, `history` (stays inside Flow), `reload`, `offline`, `failApi` (an injected
  failure the guard allows), `resize`, `rotate`, `bargeIn` (talk over the agent), `secondTab`
  (same login, own guard, API and agent). `persona.agent` is the fake agent (`call`, `say`, `emit`).
- Invariants after every step (`persona.check()`), on every open tab: nothing stays busy past
  `settleMs`; one scroll root, no page overflow, nothing sideways; the dock is there and an open
  task stays reachable; no `foreign` text (another workspace or login); no action approved twice
  (from the confirm requests the browser sent); no console errors or failed requests except the
  ones a step injected (`failApi`, `offline`).
- Mock honesty: give every proposal its own ticket, as Office does (`stockReview(total, \`stock-${total}-${n}\`)`);
  `stockRoutes`' default ticket repeats for the same total, so two separate proposals of 130 would
  look like one action approved twice. Agent answers need their own `event_id` (see `harness.ts`), or
  the provider merges rows. Neither is a Flow bug.
- Allowed failures are injected per page with `markInjected` (from `src/test.ts`); everything
  else still fails the test.

## A persona broke Flow: report it

The error says `Persona broke at step N (<step>). Replay: PERSONA_SEED=<seed>` plus the trail.
Report to the coordinator (and append to the findings file):

- **id**: `P<persona>-<n>` (e.g. `P2-3`), **seed**, **project** (flow-desktop / flow-tablet / flow-mobile)
- **steps**: the trail (shorten it to the steps that matter)
- **expected / actual**, **severity**, suspected files
- commit the reproduction as a red spec marked `test.fail()` with the finding id; never weaken an
  invariant to make a persona pass.
