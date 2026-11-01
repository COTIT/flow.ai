# Ruaha 360 — design handoff

**For: Claude Design. Scope: visual redesign only.**

Everything described here is built, tested and working. Nothing in this
document is a request for a new feature, a new screen, a new control or a new
data point. The application's behaviour is finished and verified — 780 unit
tests, 173 end-to-end tests, 24 database policy assertions, all green. What it
has never had is a designer.

This document is the whole context you need: what the product is, who uses it,
what every screen does, what every component carries, what the current styling
actually is, and — most importantly — the handful of visual and verbal rules
that **cannot** change, because breaking them would misrepresent what the
programme promises to Tanzanian farmers.

---

## 1 · What this is and why it exists

Ruaha 360 is a rural economic development platform for **Ruaha Energy**. First
deployment: **Ilundo, Tanzania**. It is being demonstrated to stakeholders on
**30 September 2026** and currently holds demo data only.

The product does one thing, and everything else is a view over it:

> A field officer registers a farmer — their household, farm, plot, crop cycle
> and expected harvest — in one form, in one transaction. Everything after that
> is a view over what the officer wrote.

Three surfaces read that data:

| Surface | Who | Device | Language |
|---|---|---|---|
| **Farmer** | Smallholder farmers in Ilundo | Low-end Android, 3G, mobile-first | **Kiswahili required** |
| **Officer** | Field officers, one or more villages | Android phone, often outdoors | **Kiswahili required** |
| **Ops / Tower** | Programme staff and management | Desktop, office | English acceptable |

The Control Tower is the screen the demo builds towards: five aggregate tiles
where every headline number drills down to the individual farmer record behind
it. **That traceability is the product's central claim.** A number that cannot
be traced does not belong on the screen.

### The tone this product has to hold

This is a development programme, not a marketplace. The interface repeatedly
has to say "this is an estimate", "this is not a sale", "this is planned, not
measured" — and mean it. Several people in the room at the demo will be looking
for exactly those distinctions.

A redesign that makes the product feel more confident than it is would be a
worse design, not a better one. The visual language should read as: **careful,
grounded, honest about uncertainty.** Not fintech. Not startup dashboard.

---

## 2 · Hard constraints

These are not preferences. Read this section twice.

### 2.1 Every `data-testid` must survive

There are **220 `data-testid` attributes** across the codebase and they are
load-bearing: 173 end-to-end tests select elements by them. Restyle, restructure
and re-nest freely, but if an element carries a `data-testid`, that attribute
must remain on an element that still means the same thing.

Moving `data-testid="offered-total"` from a `<dd>` to a `<span>` inside it is
fine. Deleting it, or putting it on a different value, breaks the suite.

The full list is in §8.

### 2.2 Product language cannot be softened

These are product requirements from the working agreement, written down because
getting them wrong misrepresents the programme:

- **Capacity is `planned` or `nameplate`, never measured.** It is always shown
  with its basis. There is deliberately no "measured" value in the database.
- **Prospective and approved demand are separate figures and are never summed.**
  One is an application; the other is a decision. A design that combines them
  into one bar or one total is wrong.
- **An estimate is always labelled an estimate.** Never a projection, forecast
  or target.
- **Prices are always labelled "indicative".** They are not quotations.
- **An opportunity is not a sale, a delivery or a payment.** "Accepted" means
  both sides agreed to keep talking. This sentence appears on every opportunity
  surface and must stay legible, not shrink into fine print.
- **`cycle_area_ha` is "planted area across cycles", never "land area".**
  Intercropping means it can exceed the village's hectares.
- **Every record shows where its data came from** (the `ProvenanceBadge`).

If a layout change would make any of these harder to read, the layout loses.

### 2.3 Swahili is coming, and strings will get longer

The farmer and officer surfaces are specified to ship complete Kiswahili and
currently do not — 557 of 559 strings render English through a fallback, waiting
on a native reviewer (see `docs/i18n-handover.md`).

**Design for the translated version, not the English one.** Kiswahili runs
roughly 10–30% longer than English for UI labels. Buttons sized to "Verify"
will break on "Thibitisha". Anything that depends on a string being short will
fail in the language this product is actually for.

Crop and equipment names come from the **database**, per locale, and are
already translated — so a Swahili-locale user sees "Mahindi" where an English
one sees "Maize", inside otherwise-English chrome. Both must look deliberate.

### 2.4 The stack is locked

Tailwind v4 + shadcn/ui, React 19, TanStack Router/Query/Table, react-hook-form
+ Zod, react-i18next, vite-plugin-pwa. **No new dependencies** — including no
chart library, no animation library, no icon package that is not already
present. If something appears to need one, say so rather than adding it.

Tailwind v4 uses the CSS-first `@theme` block in `src/styles/globals.css`; there
is no `tailwind.config.js`.

### 2.5 Performance is a field constraint

The farmer bundle runs on the weakest hardware in the system over 3G, and the
PWA precaches the shell. The font subset is already trimmed to latin-only for
this reason (the default Poppins import pulled 160 kB of Devanagari that neither
English nor Kiswahili can render).

Heavy background images, large decorative assets, blur-heavy effects and
web fonts beyond the four existing weights all cost real money on a field phone.

---

## 3 · The current design system

All of it lives in `src/styles/globals.css`. This is the complete visual
vocabulary today.

### 3.1 Brand tokens

```
--primary   #1d70b7    Ruaha blue    primary actions, links, active nav
--accent    #93c01f    Ruaha green   approved / positive states
--deep      #0c1f5b    deep navy     all body text, headings
--surface   #f5f3e5    warm sand     page background
```

`--primary` and `--accent` deliberately occupy shadcn's semantic slots, so the
system has exactly one blue and one green rather than a brand set and a
component set that drift apart.

Derived:

```
--background         var(--surface)       the warm sand
--foreground         var(--deep)
--card               #ffffff
--muted-foreground   oklch(0.5 0.02 260)
--destructive        oklch(0.577 0.245 27.325)
--border             oklch(0.2 0.08 265 / 12%)
--input              oklch(0.2 0.08 265 / 16%)
--ring               var(--primary)
--radius             0.625rem   (sm/md/lg/xl derived as 0.6/0.8/1.0/1.4×)
```

**Chart tokens exist but no chart library is installed**, and none will be
added. Tower figures are rendered as typography and one CSS meter bar. If a
redesign wants data visualisation, it has to be done with CSS and SVG that is
written by hand.

**There is no dark palette.** The `dark:` variant compiles so shadcn components
keep working, but nothing defines dark values and nothing toggles the class.
Dark mode is not a product requirement. Do not introduce one as a side effect.

### 3.2 Typography

**Poppins**, self-hosted, weights 400/500/600/700, latin subset only. It is both
`--font-sans` and `--font-heading`.

The type scale in use today is Tailwind defaults, applied ad hoc:

| Use | Current |
|---|---|
| Screen title | `text-lg font-semibold` |
| Section heading | `text-sm font-semibold` |
| Body | `text-sm` |
| Secondary / notes | `text-xs text-deep/60` |
| Figures | `text-lg font-semibold tabular` or `text-sm tabular` |

**`.tabular` (`font-variant-numeric: tabular-nums`) is mandatory wherever
figures stack** — every table column of numbers, every Tower tile. This is a
specification requirement, not a preference. Figures that do not align are
figures that cannot be compared.

There is no defined scale beyond this, no line-height system, and no vertical
rhythm. **This is the largest single opportunity in the redesign.**

### 3.3 Spacing, borders, elevation

Current conventions, applied by hand rather than systematically:

- Card: `rounded border border-deep/10 bg-white/70 p-4`
- Section stack: `space-y-4` / `space-y-5`
- Field stack: `space-y-1`
- Table cell: `px-2 py-2`
- Input: `w-full rounded border border-deep/20 bg-white px-3 py-2`
- Primary button: `rounded bg-primary px-3 py-2 text-sm font-medium
  text-primary-foreground disabled:opacity-60`
- Destructive button: `rounded border border-destructive/30 text-destructive`

**There is no shadow anywhere in the application.** Depth is expressed entirely
through borders and the white-on-sand contrast. That is a defensible choice on a
warm background and worth keeping deliberately or changing deliberately, but not
by accident.

Opacity-suffixed brand colours (`text-deep/60`, `border-deep/10`, `bg-white/70`)
are used constantly and inconsistently — `/10`, `/15`, `/20`, `/25`, `/30`,
`/50`, `/60`, `/70` all appear. **Collapsing these into a named set is an easy,
high-value win.**

### 3.4 Layout

- **Farmer / officer:** single column, floating bottom tab pill fixed to the viewport
  (clear of the system gesture area via `--tab-bar-bottom`); `main` pads by the measured
  `--tab-bar-height` plus 1rem to clear it. Mobile-first, genuinely.
- **Ops / Tower:** sidebar beside content at `lg` and above; below `lg` the
  sidebar becomes a horizontally scrollable strip above the content. This was a
  bug fix — at 375px the fixed sidebar squeezed the content column until figures
  were cut mid-number.
- Content widths: `max-w-xl` (forms), `max-w-lg` (farmer detail), `max-w-3xl`
  (opportunity), `max-w-2xl` (demand create). Tower uses a two-column grid at
  `lg`.
- Wide tables sit in `overflow-x-auto` wrappers and scroll inside themselves.
  The page itself must never scroll sideways.

---

## 4 · Component inventory

Nine shared components in `src/components/`. Each has a job that survives any
visual treatment.

### 4.1 `ProvenanceBadge` — the most important component in the system

`data-testid="provenance-badge"`, with `provenance-verification` and
`provenance-confidence` inside it.

Renders, as one chip group:

- **source** — `field_verified` · `farmer_reported` · `officer_estimated` ·
  `partner_supplied` · `calculated` (5 distinct treatments required)
- **verification** — `unverified` · `pending` · `verified` · `disputed`
- **confidence** — `low` · `medium` · `high`
- captured at, captured by

It appears on **every record on every surface**. It is how the
"reported / verified / measured / estimated" discipline actually reaches a user.

Today it is three small text chips in a row and it reads as noise — it repeats
down a list until the eye stops seeing it. **Making provenance legible at a
glance, and distinguishable between the five sources, is the single highest-value
design problem in this application.**

Constraint: it must not become so quiet that it disappears, and `verified` must
never be visually indistinguishable from `unverified`.

### 4.2 `StatusPill`

`data-testid="status-pill"`, plus `data-status="<value>"` which tests select on.

Three independent status vocabularies, deliberately in separate namespaces
because the same word means different things:

- **request** — `draft` · `submitted` · `under_review` · `approved` ·
  `rejected` · `withdrawn`
- **demand** — `open` · `matched` · `closed` · `cancelled`
- **opportunity** — `proposed` · `shared` · `accepted` · `declined` · `lapsed`

Current treatment: `rounded-full border px-2 py-0.5 text-xs font-medium`, with
per-status border/background/text colours.

An opportunity's `accepted` must **not** read as strongly as a request's
`approved`. One is a decision; the other means two parties agreed to keep
talking. The current palette makes them look alike, which is the kind of thing a
designer should catch.

### 4.3 `EnergyEstimatePanel`

`data-testid="estimate-panel"`, with `estimate-rated-power`, `estimate-quantity`,
`estimate-hours`, `estimate-days`, `estimate-power`, `estimate-kwh-day`,
`estimate-kwh-week`, `estimate-basis`.

Shows inputs, the arithmetic, and the outputs (kW, kWh/day, kWh/week), and
recalculates live as the farmer changes the assumptions. Currently a dashed
border box — the dashes are carrying "this is provisional" on their own.

It must always read as an estimate. It mirrors database generated columns
exactly; the numbers are not negotiable, but how the arithmetic is shown is
entirely open. There is a real opportunity here to make the calculation *visible*
rather than just its result.

When an input is impossible (99 hours in a day), the panel is replaced by a
short explanation instead of a confident figure. Both states need design.

### 4.4 `CoverageBar`

`data-testid="coverage-pct"`, `coverage-committed`, `coverage-available`, with
`role="meter"` and `aria-valuenow/min/max`.

Demand against available supply, with the already-committed slice visible. This
is the one genuinely graphical element in the application. It has to show three
quantities at once — demand, available, already committed — without implying
that committed supply is available.

### 4.5 `DataTable`

TanStack Table wrapper. Sorting (headers carry `aria-sort`), URL-synced filters,
empty state, an `overflow-x-auto` wrapper. Used for: people, requests, demand,
catalogue, buyers, villages, and all three Tower drill-downs.

Every table in the ops surface is this component. Improving it improves six
screens at once. Row height, header treatment, zebra vs borders, numeric column
alignment and the sort affordance are all open.

### 4.6 `EmptyState` / `ErrorState`

`data-testid="empty-state"` / `error-state`.

These are **not interchangeable**, and the distinction is a specification rule:

- **Empty** = zero rows, which is a legitimate answer. Row-level security
  returning nothing means "you may not see this". It is never an error, it is
  never red, and it never offers a retry.
- **Error** = something actually failed. `role="alert"`, and it may offer
  "Try again".

A redesign that makes empty states look like failures would break a rule the
whole data model is built on.

### 4.7 `DrillLink`

`data-testid="drill-link"`. Turns any aggregate cell into a path to its rows —
person, farm, cycle, request, demand, opportunity. Currently a plain underlined
primary-coloured link inside a table cell.

This is the visual expression of the traceability claim. It appears dozens of
times in the Tower drill-downs. It deserves better than a default link.

Related: `tile-drill` on the Tower tiles is deliberately **not a link while its
tile is still loading** — it renders as dimmed text instead, so nobody navigates
from a figure they have not seen. That state needs a treatment.

### 4.8 `UnsavedDraftBadge`

`data-testid="unsaved-draft-badge"`. Says a form has been saved locally and
**not** to the server. It must look unsaved. A successful-looking treatment here
would be a lie on a phone that just lost signal mid-registration.

### 4.9 `RequireRole`

No visual output. Route-guard convenience only.

---

## 5 · Screen inventory

23 routes. Every one is built; none is a placeholder.

### 5.1 Shell — all surfaces

- **Demo banner** (`demo-banner`) — always visible, top of every page: "Demo
  data — Every figure here is invented. Nothing is a measured Ruaha result."
  It must stay prominent. It is the reason nobody can mistake the demo for a
  report.
- **Header** — brand mark, current user name, language switch, sign out.
- **Nav** — bottom tabs (`nav-tabs`) for farmer and officer; sidebar
  (`nav-sidebar`) for ops.

### 5.2 Auth

| Route | testid | Notes |
|---|---|---|
| `/login` | `login-email`, `login-password`, `login-submit`, `login-error` | Four states: idle · submitting · invalid credentials · network error |
| `/select-role` | `select-role` | Only reachable when a user genuinely holds more than one role |
| `/no-access` | `no-access` | A real screen with a next step, never a blank page |

### 5.3 Officer surface — Kiswahili required

| Route | testid | What it is |
|---|---|---|
| `/officer` | `officer-home` | Assigned villages, counts (people / farms / requests), outstanding-verification call to action |
| `/officer/register` | `register-submit` | **The most important screen in the build.** One page, ~20 fields in 5 fieldsets, one submit, one transaction |
| `/officer/people` | `people-table` | Searchable, filterable by verification status; URL-held |
| `/officer/people/$personId` | `person-detail` | Person → household → farms → plots → cycles → harvest series, with inline verify actions and section-level correction forms |
| `/officer/farms/$farmId` | `farm-detail` | Farm, GPS point, plots |
| `/officer/cycles/$cycleId` | `cycle-detail` | Cycle with its harvest series — current figure plus every superseded one, labelled |
| `/officer/verify` | `verify-queue` | 14 mixed-type records awaiting verification; each summary links to contextual detail, while a separate Verify button requires confirmation |

**`/officer/register` deserves specific attention.** It is the longest form in
the application, used outdoors, one-handed, possibly in sunlight, by someone
standing in a field with a farmer waiting. It currently renders as five
`<fieldset>` blocks of stacked inputs with no visual progress, no grouping
hierarchy beyond a legend, and validation messages appearing below fields on
submit. The data model behind it cannot change; how it is presented is
completely open, including whether it should feel like one long form or a
sequence.

### 5.4 Farmer surface — Kiswahili required

| Route | testid | What it is |
|---|---|---|
| `/farm` | `farm-home` | Farm summary (farms/plots/cycles), latest request status, opportunity count |
| `/farm/my-farm` | `my-farm` | Their own records with provenance on everything |
| `/farm/equipment` | `equipment-list` | Catalogue with rated power and indicative price |
| `/farm/equipment/$equipmentId` | `equipment-detail` | Detail plus the request form with the live estimate |
| `/farm/requests` | `requests-list` | Their own requests with status pills |
| `/farm/requests/$requestId` | `request-detail` | One request, the stored estimate, and any action they may take |
| `/farm/opportunities` | `farmer-opportunities` | Opportunities their harvest is inside — deliberately no buyer details, because a farmer cannot see buyer records |

This surface has the **lowest literacy assumptions and the worst hardware** in
the system. It is also the surface that most needs to not overpromise: a farmer
reading "6,400 kg" next to a buyer's name could reasonably think they have sold
something. They have not.

### 5.5 Ops surface — English acceptable

| Route | testid | What it is |
|---|---|---|
| `/ops` | `ops-home` | Three queue counts: requests awaiting review, open demands, records to verify |
| `/ops/requests` | `requests-table` | Pipeline, filterable by status and village |
| `/ops/requests/$requestId` | `request-review` | Review screen with a decision note and Start review / Approve / Reject |
| `/ops/catalogue` | `catalogue-table` | Equipment with indicative prices |
| `/ops/buyers` | `buyers-table` | Buyers plus a create form |
| `/ops/demand` | `demand-table` | The order book plus an 8-field create form |
| `/ops/demand/$demandId` | `demand-detail` | One demand, villages that could supply it, per-village coverage blocks |
| `/ops/opportunities/$opportunityId` | `opportunity-detail` | Supply lines with drill links, the status machine, the attach-supply form |
| `/ops/villages` | `villages-table` | Village capacity with basis and simultaneity factor |

### 5.6 Control Tower — the demo's destination

`/ops/tower` (`tower`), with three drill-downs: `/ops/tower/production`,
`/ops/tower/energy`, `/ops/tower/market`.

Five tiles, each `tile-<id>`:

1. **Production** — expected and actual harvest by crop and window, plus
   planted area across cycles
2. **Equipment pipeline** — requests by status with indicative catalogue value
3. **Energy** — planned capacity with basis, prospective peak, approved peak,
   headroom, simultaneity factor
4. **Market** — open demand against available supply, with coverage
5. **Data quality** — verified share, GPS coverage, cycles with estimates

Every tile except data quality carries a drill link. The tiles are currently
uniform bordered boxes in a two-column grid, and they should almost certainly
not be uniform — production and energy carry far more information than the
others, and the energy tile in particular has to hold five figures plus three
explanatory notes without any of them being summed together.

**The real figures**, which the tests assert and which will be on screen at the
demo:

```
production   12,000.00 kg expected      (a superseded 3,200 kg estimate excluded)
market       9,000 demand · 5,600 available · 62.2% coverage
energy       500.000 kW planned, "Basis: Planned"
             prospective 7.200 kW · approved 10.800 kW   (never summed)
             headroom 489.200 kW · simultaneity 0.6
quality      3/6 verified · 3/4 with GPS · 5/7 with an estimate
```

---

## 6 · States every screen already has

Each of these is implemented and tested. They need design, not invention.

| State | Rule |
|---|---|
| **Loading** | Every screen has its own `*-loading` testid. Currently plain "Loading…" text. No skeletons anywhere. |
| **Empty** | Zero rows is an answer. Never red, never a retry. |
| **Error** | `role="alert"`. Database messages are shown **verbatim** — some are long and specific, e.g. `over-commitment: 4100.00 kg available, 4100.00 kg already committed, 100.00 kg requested`. The container must handle a two-line technical sentence without truncating it. |
| **Saving** | Controls disable in flight; one "Saving…" indicator per write, never one per button. |
| **Inline validation** | Field-level messages, one per reason. |
| **Confirmation** | One `role="alertdialog"` for the irreversible action (releasing an opportunity's supply). |
| **Not found** | A real empty state, including for a malformed id in the URL. |

---

## 7 · Accessibility — current state, honestly

Accessibility was explicitly deferred to the "make it right" phase and has had
one partial pass. What exists:

- `role="alert"` on 7 error surfaces, `role="alertdialog"` on the one
  confirmation, `role="meter"` on the coverage bar
- `aria-sort` on sortable table headers, `aria-busy` on loading Tower tiles,
  `aria-invalid` + `aria-describedby` on 3 login fields
- 4 `aria-label`s, on the nav landmarks and the language switch
- `<html lang>` follows the active language
- Every input has an associated `<label>`

What does not exist:

- No focus-visible treatment beyond the browser default and `--ring`
- No skip link
- Contrast has never been measured. `text-deep/60` on `#f5f3e5` is used for
  every secondary string in the application and is the most likely failure.
- No reduced-motion handling (there is currently no motion at all)
- Touch target sizes have never been checked against the 44px guideline, on a
  surface whose primary users are outdoors on phones

**Contrast and touch targets are the two to fix while redesigning**, because
both are cheap during a visual pass and expensive afterwards.

---

## 8 · Working with the code

### 8.1 Where things are

```
src/
  app/            shell, providers, router, guards, LanguageSwitch, SignOutButton
  components/     the nine shared components
  features/
    officer/      register, people, person detail, verify queue, records
    farmer/       home, my-farm, equipment, requests, opportunities
    ops/          home, requests, review, demand, opportunity, buyers,
                  villages, catalogue
    tower/        TowerScreen, TowerTile, TowerDrillScreens
  routes/         TanStack Router file routes (thin — they render feature screens)
  styles/globals.css    ← the entire design system
  i18n/en/common.json   ← all 559 strings
```

Screens are plain `.tsx` in `src/features/**`. Styling is Tailwind utility
classes inline; there are no CSS modules and no styled-components.

### 8.2 Copy changes

**Every user-facing string is an i18n key.** Do not hard-code text into a
component. `src/i18n/en/common.json` holds all 559, and
`docs/i18n-handover.md` is the translator's export — if you add or rename a key,
regenerate it with `pnpm i18n:handover`.

Copy improvements are welcome and in scope, with one exception: the sentences
listed in §2.2 carry product meaning and must not be shortened into something
punchier.

### 8.3 Verifying a change

```bash
pnpm dev          # localhost:5173
pnpm typecheck
pnpm test         # 780 unit tests
pnpm e2e          # 173 browser tests — this is what catches a broken testid
```

Demo accounts, all password `demo1234`:

```
officer.ilundo@demo.ruaha360.test   → /officer
neema@demo.ruaha360.test            → /farm
ops@demo.ruaha360.test              → /ops
admin@demo.ruaha360.test            → /ops
```

Switching accounts requires signing out first.

**Do not click one-way controls on seeded records while exploring** — Verify,
Approve, Reject, and an opportunity's Decline or Lapse cannot be undone, and the
seeded figures are part of the specification. Creating new records is fine; the
suite cleans up anything it marks.

**Do not switch language while signed in as a seeded user** — it persists to
that user's row and the cleanup script cannot revert a column change.

### 8.4 The full `data-testid` list

Preserve every one of these:

```
attach-closed attach-error attach-harvest attach-kg attach-submit buyer-channel
buyer-contact-note buyer-create-submit buyer-name buyer-name-error buyers-loading
buyers-note catalogue-loading catalogue-note catalogue-price coverage-pct
create-opportunity create-opportunity-error current-user cycle-card cycle-detail
cycle-detail-loading cycle-harvest decision-note decision-note-error demand-buyer
demand-create-error demand-create-submit demand-crop demand-delivery demand-detail
demand-detail-loading demand-loading demand-price demand-quality-note
demand-quantity demand-window-end demand-window-start demo-banner drill-link
empty-state energy-excluded energy-row energy-table equipment-card equipment-detail
equipment-detail-loading equipment-list equipment-loading equipment-price
error-retry error-state estimate-basis estimate-blocked estimate-panel farm-card
farm-detail farm-detail-loading farm-home farm-home-latest-request farm-home-loading
farm-home-no-requests farm-home-opportunities farm-home-opportunities-link
farm-home-summary farm-plot farmer-opportunities farmer-opportunities-loading
farmer-opportunities-note farmer-opportunity filter-status filter-village
language-error language-switch login-email login-email-error login-error
login-password login-password-error login-submit market-opportunity market-row
market-table my-contribution my-farm my-farm-loading nav-sidebar nav-tabs no-access
no-matching-supply offered-total offered-total-note officer-home officer-home-loading
officer-home-register officer-unverified officer-unverified-link officer-village
opportunity-actions opportunity-detail opportunity-loading ops-home ops-home-loading
ops-requests-loading ops-review-loading outlet people-filter-verification
people-loading people-search person-detail person-loading person-outstanding
plot-card production-cycle production-row production-table provenance-badge
provenance-confidence provenance-verification register-another register-confidence
register-crop register-cycle-area register-cycle-tree-count register-cycle-unit-count
register-error register-family-name register-farm-label register-farm-latitude
register-farm-longitude register-given-name register-harvest-end register-harvest-kg
register-harvest-start register-household-label register-is-head register-loading
register-phone register-phone-hint register-plot-area register-plot-label
register-submit register-success register-view-person release-confirm
release-confirm-no release-confirm-yes released-note request-action-error request-days
request-detail request-detail-loading request-error request-hours request-purpose
request-quantity request-review request-submit request-success request-view
requests-list requests-loading review-error review-estimate select-role
select-role-loading sign-out sign-out-error status-error status-pill status-saving
supply-row tile-drill tower tower-village unsaved-draft-badge verify-error
verify-queue verify-queue-count verify-queue-loading verify-queue-row village-basis
villages-loading villages-note
```

Plus dynamic ones: `tile-<id>`, `action-<name>`, `request-action-<name>`,
`verify-<table>-<id>`, `match-row-<villageId>`, `coverage-<villageId>`,
`register-<field>-error`, `register-<field>-rounded`.

---

## 9 · Where the design effort pays most

Ranked by how much they would improve the demo and the product, not by effort.

1. **`ProvenanceBadge`.** It is on every record on every surface and currently
   reads as noise. Making five sources and four verification states legible at a
   glance would improve every screen simultaneously. This is the most valuable
   single piece of work in the list.

2. **A real type scale and spacing system.** There is none. Everything is
   `text-sm`/`text-xs` with ad-hoc opacity. A defined scale, consistent
   secondary-text treatment, and a named set of surface/border tokens would
   remove the hand-applied inconsistency across ~30 screens.

3. **The Control Tower.** It is the demo's destination and the clearest
   expression of the product's claim. Five uniform boxes do not reflect that some
   tiles carry five figures and others carry three. The energy tile especially
   needs a structure that keeps prospective and approved visibly separate.

4. **`/officer/register`.** The most important screen in the build and the one
   with the hardest usage conditions — outdoors, one-handed, someone waiting.
   ~20 fields currently presented as five undifferentiated stacks.

5. **`DataTable`.** Six ops screens are this one component.

6. **Loading states.** Plain "Loading…" text everywhere. On 3G, this is what the
   farmer surface shows most often.

7. **Contrast and touch targets.** Cheap now, expensive later, and the current
   secondary-text colour is the most likely accessibility failure in the app.

8. **The farmer surface as a whole.** Lowest literacy assumptions, worst
   hardware, highest stakes for not overpromising.

---

## 10 · Out of scope

To be explicit, because the temptation will be there:

- No new screens, routes, features, fields or controls
- No dark mode
- No new dependencies — no chart library, no animation library, no icon set
- No changes to what any number means or how it is calculated
- No inventing Kiswahili strings (see `docs/i18n-handover.md`)
- No removing or weakening the product-language sentences in §2.2
- No changes to the database, queries, or the tests' selectors

Everything else — layout, type, colour, spacing, hierarchy, density, component
structure, copy that is not listed in §2.2, motion within reason — is open.

---

## 11 · Further reading, in the repo

| File | What it holds |
|---|---|
| `CLAUDE.md` | The working agreement, non-negotiables, product-language rules |
| `docs/screens-and-components.md` | The screen-by-screen specification, 501 lines |
| `docs/business-rules.md` | What every function does and what the database enforces |
| `docs/schema.md` | Tables, conventions, role matrix |
| `docs/demo-script.md` | The eight-step stakeholder walkthrough |
| `docs/i18n-handover.md` | All 559 strings, marked by priority, for the translator |
| `QA-FINDINGS.md` | 33 findings across three QA sweeps, 32 closed — useful for what has already been tried and why |
| `README.md` | Setup, commands, known gaps |
