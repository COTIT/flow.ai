# Handoff: Ruaha 360 — visual redesign

> **Superseded, 29 September 2026: no striped backgrounds.** Wherever this handoff
> (and the `.dc.html` mockups beside it) says *hatch* or *hatched*, the build uses a
> solid ground instead (`--sand-2` or `--paper`) because stripes made text hard to
> read. "Provisional" is carried by the word ("estimate", "planned"), italics and the
> pill wording. `--hatch` no longer exists; `src/styles/tokens.test.ts` fails if a
> repeating gradient comes back.

## Overview

Ruaha 360 is a rural economic development platform for **Ruaha Energy**, first deployed in
**Ilundo, Tanzania**, demonstrated to stakeholders on **30 September 2026**. A field officer
registers a farmer — household, farm, plot, crop cycle and expected harvest — in one form, in
one transaction; every other screen is a view over what the officer wrote. The Control Tower
aggregates it, and every headline drills back to the record underneath it.

The application is built, tested and finished: 780 unit tests, 173 end-to-end tests, 24
database policy assertions, all green. What it has never had is a designer. This bundle is
that pass: **layout, type, colour, spacing, hierarchy, density and iconography only.** No new
screens, routes, features, fields or controls; no change to what any number means.

**Start with `IMPLEMENTATION_PROMPT.md`** — paste it into Claude Code as the task brief. This
README is the specification behind it.

## About the design files

The files in `design/` are **design references created in HTML** — prototypes showing the
intended look, not production code to copy. Open them in a browser (`support.js` sits beside
them; no server needed). The task is to **recreate them inside the existing codebase** using
its established patterns: React 19, Tailwind v4 (CSS-first `@theme`, no `tailwind.config.js`),
shadcn/ui, TanStack Router/Query/Table, react-hook-form + Zod, react-i18next, vite-plugin-pwa.

| File | Role |
|---|---|
| `design/Ruaha 360 - Foundations.dc.html` | The design system. Sections 00–11: brand mark, colour, type, provenance, status, controls, states, figures, length, icons, unit marks, bars. **Implement first.** |
| `design/Ruaha 360 - Redesign.dc.html` | All 23 routes and 9 components, sections 01–14. Section 14 maps every route to its section. |
| `design/Ruaha 360 - Current UI.dc.html` | The current build recreated from source. The "before", for comparison only. |
| `tokens.css` | Ready to merge into `src/styles/globals.css`. |
| `assets/` | Brand lockup SVGs and the four Poppins weights, as supplied in the brand book. |

## Fidelity

**High-fidelity.** Final colours, typography, spacing, iconography and states. Recreate
pixel-perfectly using the repo's own libraries. Where this README and a design file disagree,
the design file wins.

---

## Design tokens

Full values in `tokens.css`. Summary:

**Colour** — brand unchanged: `--primary #1d70b7`, `--accent #93c01f`, `--deep #0c1f5b`,
`--surface #f5f3e5` (the same blue and green as the logo).
New ink scale replacing every `text-deep/NN`: `--ink #0c1f5b` (12.5:1), `--ink-2 #46547d`
(6.6:1), `--ink-3 #5d698b` (4.8:1). Deep brand steps for text on tints: `--primary-ink
#14548a`, `--green-ink #3f5410`, `--flag-ink #9e1b1b`. Surfaces: `--paper #fff`, `--sand
#f5f3e5`, `--sand-2 #efeada`. Rules: `--rule rgba(12,31,91,.12)`, `--rule-2 rgba(12,31,91,.22)`.
Tints: primary `.08`, green `.20`, flag `.07`.

> `text-deep/60` on the sand ground measures **4.17:1** and is used for every secondary string
> in the app today. It is the most likely accessibility failure in the product. Replace it.

**Type** — Poppins 400/500/600/700, latin subset, self-hosted. Eight steps:
display 30/34 700 · figure 22/26 600 tabular · title 19/26 600 (24/29 700 on a screen shell) ·
section 12/16 600 uppercase 0.12em · body 15/22 400 · body-strong 15/22 500 · small 13/19 ·
note 12/17. Nothing below 12px ships. `.tabular` (`font-variant-numeric: tabular-nums`) is
**mandatory** wherever figures stack.

**Spacing** — 4 / 8 / 12 / 20 / 32 / 48.
**Radii** — control 8, card 12, frame 14, pill 999.
**Depth** — borders and white-on-sand only. No shadow, no filter, no blur.

---

## Components

### `ProvenanceBadge` — the highest-value change
`provenance-badge`, with `provenance-verification` and `provenance-confidence` inside.
It appears on every record on every surface, and today reads as three equal text chips —
noise that repeats down a list until the eye stops seeing it.

Redesigned as **one two-segment lozenge**, pill radius, 1px `--rule-2` border, `overflow:hidden`:

- **Left — source**, quiet: a 4×13px rounded colour cap plus the source word at 12px/500.
  Five treatments: `field_verified` primary cap on `--primary-tint` in `--primary-ink`;
  `farmer_reported` `--ink-2` cap on paper; `transaction_derived` deep cap on `--sand-2`;
  `sensor_derived` green cap on `--green-tint` in `--green-ink`; `model_estimated` hatched
  cap on a hatched ground, italic.
- **Right — verification**, loud: divided by a 1px border, 12px/600, with a **13px mark whose
  shape differs**, so state is legible without colour: `verified` filled `--green-ink` disc
  with a white check; `pending` 2px `--primary-ink` ring half-filled by a linear-gradient;
  `unverified` empty 1.5px dashed `--ink-3` ring; `disputed` filled `--flag-ink` disc with a
  white bang, on `--flag-tint`.
- **Confidence** becomes a three-bar meter (5/8/11px tall, 3px wide, 2px gap) — it stops
  competing with verification for the same attention. Keep the text label for screen readers.
- **Compact variant** for lists and nested records: the 18–22px mark leads the row, and the
  words collapse to one 12px line (`Person · Farmer reported · captured 2 Sep 2026`).

`verified` must never be visually indistinguishable from `unverified`, and the badge must not
become so quiet it disappears. Both are satisfied by the mark, not by colour.

### `StatusPill`
`status-pill` plus `data-status="<value>"` (tests select on it). Pill radius, `px-11 py-4`,
12px. Three vocabularies stay in separate namespaces:
- **request**: draft (outlined, `--ink-3`) · submitted (primary tint) · under_review (primary
  border + tint, 600) · **approved (solid `--accent` fill, `#22300a` ink)** · rejected (flag
  tint) · withdrawn (hatched).
- **demand**: open (primary tint) · matched (green tint, `--green-ink`) · closed (`--sand-2`) ·
  cancelled (hatched).
- **opportunity**: every state is **outlined on white, never filled**. proposed · shared ·
  **accepted (1.5px `--green-ink` edge only)** · declined · lapsed (hatched).

A request's `approved` is a decision with capacity consequences and is the only solid fill in
the system. An opportunity's `accepted` means both sides agreed to keep talking. The current
palette makes them look alike; the redesign must not.

### `EnergyEstimatePanel`
`estimate-panel` + `estimate-rated-power|quantity|hours|days|power|kwh-day|kwh-week|basis`.
Card with a **hatched header band** carrying the title, an `ESTIMATE` pill, "This is an
estimate, not a measurement." and the basis line. The body shows **the arithmetic as three
equation lines**, not a two-column list:
`15.000 kW rated × 1 unit = 15.000 kW estimated peak` / `× 6 hours a day = 90.000 kWh a day` /
`× 5 days a week = 450.000 kWh a week`, each result at 17px/600 with its unit icon.
Blocked and impossible states are hatched panels with the existing copy — never a confident
figure. The dashed border is gone; the hatch carries "provisional" instead.

### `CoverageBar`
`coverage-pct`, `coverage-committed`, `coverage-available`, `role="meter"` +
`aria-valuenow/min/max`. Three quantities, distinguished three ways:
16–18px track, **solid green = available now**, **hatched = not covered**, and
**committed drawn outside the track, below a rule, as an outlined swatch** — committed supply
is not available and no stacked bar may imply it is. Every legend row carries its own icon and
figure; the percentage sits at 22px above the track.

### `DataTable`
Improving this improves six ops screens. Header row on `--sand-2`, 11px/600 uppercase 0.08em
`--ink-3` labels, `aria-sort` preserved. Rows ~48px, one `--rule` hairline each, **no zebra**,
hover `--primary-tint`, `tabIndex` kept on clickable rows. Numeric columns right-aligned and
tabular; text columns left. **The sort caret shows only on the sorted column** (`↑`/`↓` in
`--primary-ink`) — five idle `↕` glyphs were competing with the data. Unit icons go in the
column header, never in every cell. Wrapper keeps `overflow-x-auto`.

### `EmptyState` / `ErrorState`
Not interchangeable, and the distinction is a specification rule.
**Empty** = zero rows, a legitimate answer: `--sand-2` card, 12px radius, centred, a dashed
ring mark, title 16/600 and detail 13/1.5. Never red, never a retry, no `role="alert"`.
**Error** = something failed: `role="alert"`, `--flag-tint` ground, **4px `--flag-ink` left
rule**, filled-disc bang mark, and the database's sentence **verbatim** at 14/1.55 with
`tabular-nums` and `text-wrap: pretty` — it must hold
`over-commitment: 4100.00 kg available, 4100.00 kg already committed, 100.00 kg requested`
on two lines without truncating. Optional `Try again` at 40px.

### `DrillLink`
`drill-link`. The visual expression of the traceability claim, dozens of times per drill-down.
The **value itself** becomes the link: `--primary-ink`, 600, a 1.5px `rgba(29,112,183,.4)`
bottom border that goes solid on hover, and a 15px `arrow-right` after it. `tile-drill` on a
loading tile stays dimmed unlinked text at 60% — nobody navigates from a figure they have not
seen.

### `UnsavedDraftBadge`
`unsaved-draft-badge`. Pill, **hatched ground, 1.5px `--flag-ink` edge, `--flag-ink` text, an
8px filled dot**. It must look unsaved: a successful-looking treatment here would be a lie on
a phone that just lost signal mid-registration. In the register form it lives in the sticky
footer beside the only button that ends it.

### `RequireRole`
No visual output.

---

## Screens

Section numbers refer to `Ruaha 360 - Redesign.dc.html`. Section 14 is the coverage index.

### Shell — every surface
Demo banner (`demo-banner`): full-width `--accent`, `#1d2a06` ink, a `DEMO DATA` outlined pill
plus "Every figure here is invented. Nothing is a measured Ruaha result." Always visible, top
of every page, prominent.
Header: white, 1px `--rule` base, the **Ruaha Energy SVG lockup** at 26px (23px on field
surfaces) followed by a 1px divider and `360` at 15/600 `--ink-2`; then user name, language
select, sign out at 40px.
Ops nav (`nav-sidebar`): 216px white column, 44px items, active item `--primary-tint` +
`--primary-ink` + a 3px primary rule + `gauge` icon. Field nav (`nav-tabs`): a **floating solid pill**
(paper, 1px `--rule-2` border, `--radius-frame`; no blur, no shadow — `depth.test.ts`), 60px
items, **22px Lucide icon above a 12px label**, active item is a `--primary-tint` fill; the
officer's verify tab carries a count badge. It floats `--tab-bar-bottom` above the screen edge
(`max(12px, safe-area-inset-bottom − 10px)`), which needs `viewport-fit=cover` in `index.html`
or the inset reads 0 and the bar sits under the iOS home indicator / Android gesture bar.

### Control Tower — the demo's destination (§01)
`tower`, tiles `tile-<id>`, each drill `tile-drill`. **The tiles are deliberately not uniform.**
A wrapping flex row: **Production** (`1 1 430px`) leads with the display figure
`12,000.00 kg` and names the excluded superseded 3,200 kg estimate, then a three-column
crop/window table and the planted-area row with its "not land area" note. **Energy**
(`1 1 350px`) shows `500.000 kW` with a hatched `Basis: planned` pill, then
**prospective and approved in two framed cells with a rule between them** — the layout cannot
express their sum — then the never-summed sentence at 13/500, a headroom bar labelled with
both segments, and simultaneity. **Market** (`1 1 340px`) is the coverage bar. **Pipeline**
(`1 1 280px`) is four status rows with indicative values. **Data quality** (`1 1 230px`) is the
smallest, sunken, three ratio meters, and has no drill link because it counts records rather
than reporting a figure. Loading tiles keep their header and footprint.

### Tower drills (§02, §13)
**Energy**: the two grouped `<tbody>` blocks become **two separate tables in two cards**, the
approved one keyed to `--primary-tint`, each with its own footer arithmetic
(`Sum of estimated peaks 18.000 kW × 0.6 simultaneity = 10.800 kW`). `energy-excluded` states
the draft/rejected/withdrawn requests that feed neither figure.
**Production**: chips for the tile totals, then a table ending in a `cycle` drill per row and a
footer restating the 12,000.00 kg headline.
**Market**: buyer/crop rows with an inline 72px coverage bar; the seeded coffee demand keeps
its row with an empty hatched track — an honest zero, stated.

### Officer (§03, §04, §08, §09)
**`/officer/register`** is the most important screen in the build: outdoors, one-handed,
someone waiting. It stays **one page, one submit, one transaction** — but gains a **six-chip
completion rail** (using the same mark shapes as verification), numbered section cards, 48px
fields at 17px with tabular numerics, "optional" said in the label, the crop-measure branch
labelled (`Mahindi is measured by area`), rounding notes carrying the hatch mark, confidence as
a **three-target radio row** instead of a select, and a **sticky footer** holding the draft
state next to the submit.
**`/officer/verify`**: 48px rows, the verification mark leading, a type chip, one line of
provenance words, and a 48px Verify button with a check icon.
**`/officer/people`**: the DataTable treatment plus the full lozenge in the provenance column.
**`/officer`**: register as a 52px primary action, the outstanding-verification prompt as a
left-ruled card, village cards with three figures.
**Person / farm / cycle detail**: six nested levels — person → household → farm → plot → cycle
→ harvest — each a 3px rail tinted by verification, the mark leading every row. On cycle
detail the harvest **series** shows current outlined in blue and superseded hatched with the
figure struck through: the 3,200 kg the Tower excludes is visible and obviously not counted.

### Farmer (§05, §10) — lowest literacy assumptions, worst hardware
`/farm`, `/farm/my-farm`, `/farm/equipment`, `/farm/equipment/$id`, `/farm/requests`,
`/farm/requests/$id`, `/farm/opportunities`. Larger type, 48–52px controls, full-width buttons.
Prices carry an **`Indicative` pill**, not a grey parenthesis. The not-a-sale sentence is 14px
body on a bordered panel — it must never shrink into fine print. On `/farm/opportunities` the
farmer's own share is the tinted card and the opportunity total is context; no buyer name
appears, and the screen says the officer holds it rather than rendering a blank.

### Ops (§06, §11, §12)
`/ops` three linked queue cards; `/ops/requests` pipeline; `/ops/requests/$id` review with the
snapshotted inputs on a hatched panel and the headroom split cells; `/ops/catalogue` (null
renders `—`, never `0.000`); `/ops/buyers` (AFM takes the hatched pill — a label about origin,
not a live connection); `/ops/villages` (the basis pill sits **inside the figure's own cell**,
so it can never be sorted away from the number); `/ops/demand` order book + eight-field create
form that wraps; `/ops/demand/$id` coverage per village; `/ops/opportunities/$id` where the
buyer's demand and the offered total are **two separate cards**, only one tinted, and the
supply table restates the offered total in its own footer.

### Auth (§07)
`/login` (idle, submitting, invalid credentials, network error), `/select-role` (64px choices),
`/no-access` (a real screen with a next step, never blank, never an error).

---

## Interactions & behaviour

- **No animations and no transitions anywhere** — see the performance section of the
  implementation prompt. Loading is a flat skeleton sized to what replaces it.
- Hover: `--primary-tint` on rows and secondary buttons, `--primary-ink` on primary buttons,
  a solid underline on drill links. Focus: 2px primary outline, 2px offset.
- Controls disable in flight; **one "Saving…" indicator per write**, never one per button.
- Inline validation: one message per reason, field-level, with the bang mark.
- One `role="alertdialog"` for the irreversible action (releasing an opportunity's supply).
- Not found is a real empty state, including for a malformed id in the URL.

## State management

Unchanged. Filter state stays in the URL as validated search params; the register draft stays
in IndexedDB keyed by the URL-held `client_ref`; TanStack Query owns server state; no client
aggregation — every figure is read from a database view.

## Assets

- `assets/ruaha-logo.svg` — the supplied lockup, for white and sand grounds. 26px in the
  desktop header, 23px on field surfaces, 34px on the login card; never below 20px.
- `assets/ruaha-logo-dark-bg.svg` — for deep navy and saturated grounds.
- `assets/ruaha-logo-white.svg` — **carries no fill in the supplied file** and renders black
  as an `<img>`; it is a one-colour silhouette that must be inlined and given a colour.
- `assets/fonts/Poppins-{Regular,Medium,SemiBold,Bold}.ttf` — from the brand book. The repo
  already self-hosts these weights via `@fontsource/poppins/latin-*`; keep that import rather
  than adding files.
- Icons: **Lucide**, already installed. sprout · wrench · file-text · handshake · user-plus ·
  users · circle-check-big · clipboard-list · package · book-open · briefcase · map-pin ·
  gauge · zap · battery-charging · banknote · weight · land-plot · arrow-right / arrow-left.
  Unit marks: weight→kg, land-plot→ha, zap→kW, battery-charging→kWh, banknote→TZS,
  users/map-pin/sprout→record counts. Icons never appear alone in navigation, and never
  inside table cells or status pills — those already carry a shape system.

## Files in this bundle

```
IMPLEMENTATION_PROMPT.md   paste into Claude Code
README.md                  this specification
tokens.css                 merge into src/styles/globals.css
design/                    the three HTML design references + support.js
assets/                    brand lockups and Poppins weights
```
