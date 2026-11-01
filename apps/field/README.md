# Ruaha 360 (`@genesy/field`)

> Ported from the RUAHA repository into the Genesy monorepo (Vite app, port 3002). It runs on the Genesy API
> (`apps/api`, the `/v1/farm/*` endpoints) through `@genesy/api-client`; there is no other backend. Run with
> `pnpm --filter @genesy/field dev`. The product notes below are RUAHA's own.

Rural economic development platform for Ruaha Energy. First deployment:
Ilundo, Tanzania.

**This is an MVP for a stakeholder demo on 30 September 2026. It holds demo
data only — no real farmer data has been entered, and none should be.**

---

## What it does

A field officer registers a farmer, their household, farm, plot, crop cycle
and expected harvest — one page, one submit, one transaction. Everything after
that is a view over what the officer wrote:

- the farmer sees their own records, with provenance, and can request powered
  equipment with a live energy estimate
- ops reviews and decides those requests, records buyer demand, and matches it
  against village supply
- the Control Tower aggregates all of it, and every headline drills back to
  the records underneath it

The specifications are `docs/schema.md`, `docs/business-rules.md` and
`docs/screens-and-components.md`; the rules themselves live in the Genesy API.

---

## Running it

```bash
pnpm --filter @genesy/field dev     # needs the Genesy API running (see the root README)
pnpm --filter @genesy/field test    # vitest
pnpm --filter @genesy/field typecheck
```

`.env.local` needs only `VITE_DATA_MODE=demo` and `VITE_FEATURE_RESOURCES` (see
`.env.example`). The browser suite lives in `packages/e2e/tests/field`.

---

## Known gaps, stated plainly

### Swahili is a draft, not reviewed

Every screen is in Swahili, the guided tour for each role included, and so are
Ops and the Tower. **No native reader has seen any of it.** It was drafted on
29 and 30 September 2026 from a sourced glossary (`docs/i18n-glossary.md`),
back-translated blind as a cross-check, and shipped so the demo is usable by
people who do not read English. CLAUDE.md requires Swahili on the farmer and
officer surfaces only; Ops and Tower are covered as well because the tour
walks through them.

What "draft" means in the repo:

- `src/i18n/sw/reviewed.json` lists the strings a named person has reviewed.
  Today that is the two language names. Everything else is a draft
- `src/i18n/sw/flags.json` holds the doubts the drafter and the cross-check
  raised, and they appear in the Notes column of the review packet
- `docs/i18n-handover.md` and `docs/i18n-handover.csv` are that packet: every
  string, its draft, its status, and the labelling rules a translator needs —
  "estimate", "indicative price" and "planned capacity" are claims about what
  the programme does and does not promise
- `src/i18n/sw/en-source.json` records which English each Swahili string was
  written from. If the English changes, `bundles.test.ts` fails until the
  string is re-translated and `pnpm i18n:seal` is run. This exists because the
  tour's welcome text once kept promising "seven short stops" in Swahili after
  the English stopped saying so
- The glossary marks 80 of 176 terms `unverified`: no Tanzanian source was
  found. Those go to the reviewer first

A wrong Swahili string is worse than English, because English is visibly
untranslated and a wrong string is not. `src/i18n/bundles.test.ts` keeps the
bundle complete, keeps placeholders intact, and rejects the incentive being
described as earnings, a wallet, a balance or a payment. It cannot check that
the Swahili is right. Only a native reviewer can.

Not translated: any API message not listed in `src/lib/dbMessages.ts`, which
is shown as the API wrote it. The sign-in screen also opens in English until a person
switches language, because the signed-out choice is not remembered.

### What is deliberately absent

Training, services, progress tracking, photo upload, farm polygons, offline
sync queues, notifications, meter screens, tariffs, finance terms, repayments,
crowdfarming, wallets, export tracking, buyer self-service, and any AI
surface. Reference decks show several of these; a screen existing does not put
it in scope. Plan v2 controls that.

### Open research

Finance terms and Bank of Tanzania Tier 2 classification (item C), consent and
registration for real farmer data (items A and B), and the season, grade and
confidence taxonomies (Plan v2 S22). The demo does not depend on any of them,
and none should be guessed at.

---

## Where the numbers come from

Every figure on a screen is read from the Genesy API. The client does not
aggregate — see `docs/business-rules.md` §7 and §11. The seeded demo figures
are part of the specification and are asserted by the test suite:

```
Ilundo maize, Sept    12,000 kg expected  (superseded 3,200 excluded)
committed              6,400 kg
available              5,600 kg
demand 9,000 kg    →   62.2% coverage
approved peak          (15.0 + 2×1.5) × 0.600 = 10.800 kW
headroom               489.200 kW
```

If a change moves one of these, either the change is wrong or the
specification has moved. Both are worth stopping for.
