# Implementation prompt — paste this into Claude Code

You are implementing an approved visual redesign of **Ruaha 360**, an existing, finished
React application. Read this whole prompt before writing any code.

---

## What you are doing

The repository is complete and tested: 780 unit tests, 173 end-to-end tests, 24 database
policy assertions, all green. **No behaviour changes.** This is a visual pass only.

The design is delivered as three HTML files in `design/`:

| File | What it is |
|---|---|
| `Ruaha 360 - Foundations.dc.html` | The design system: brand mark, colour, type scale, spacing, provenance, status, controls, states, figures, icons, unit marks, bars. **Read this first and implement it first.** |
| `Ruaha 360 - Redesign.dc.html` | All 23 routes and 9 shared components, redesigned, in numbered sections 01–14. Section 14 is a coverage index mapping every route to its section. |
| `Ruaha 360 - Current UI.dc.html` | The build as it is today, recreated from source. Reference only — this is the "before". |

Open them in a browser (they are self-contained; `support.js` sits beside them). They are
**design references, not production code.** Do not copy their markup. Recreate them in the
repo's own environment: React 19 + Tailwind v4 + shadcn/ui, TanStack Router/Query/Table,
react-hook-form + Zod, react-i18next.

Fidelity required: **1:1**. Every colour, size, weight, radius, border, spacing step,
icon and piece of copy in the design files is deliberate. Where the design and this
document disagree, the design file wins; where the design and the repo's product rules
disagree, stop and ask.

---

## Hard constraints — read twice

1. **Every `data-testid` must survive.** 220 of them; 173 e2e tests select on them.
   Restyle, restructure and re-nest freely, but an attribute must stay on an element that
   still means the same thing. `pnpm e2e` is the check.
2. **No new dependencies.** No chart library, no animation library, no icon package.
   Icons come from `lucide-react`, which is already installed (`components.json` sets
   `"iconLibrary": "lucide"`). Every mark, meter, hatch and bar in the design is CSS.
3. **No new fonts.** Poppins 400/500/600/700, latin subset, already self-hosted.
4. **No dark mode.** The `dark:` variant compiles; nothing defines dark values. Do not
   introduce any.
5. **No animation and no transitions.** See "Performance" below. This is a design
   decision, not an omission.
6. **Product language cannot be softened.** Capacity is `planned` or `nameplate`, never
   measured, and always shown with its basis. Prospective and approved demand are separate
   figures and are never summed. An estimate is always labelled an estimate. Prices are
   always labelled indicative. An opportunity is not a sale, a delivery or a payment.
   `cycle_area_ha` is "planted area across cycles", never "land area". Every record shows
   its provenance. If a layout change makes any of these harder to read, the layout loses.
7. **Every user-facing string is an i18n key** in `src/i18n/en/common.json`. Do not
   hard-code text. If you add or rename a key, run `pnpm i18n:handover`.
8. **Never invent Kiswahili.** 557 of 559 strings await a native reviewer. Design for the
   longer translated strings; do not write them.
9. **No new screens, routes, fields, controls, or changes to what any number means.**

---

## Order of work

Do it in this order and commit after each step.

1. **Tokens.** Apply `tokens.css` to `src/styles/globals.css` — three ink values, three
   surfaces, one hatch, the radius set, the type scale. Nothing else changes yet.
2. **Shared components** in `src/components/`, in this order: `ProvenanceBadge`,
   `StatusPill`, `EmptyState`/`ErrorState`, `DataTable`, `DrillLink`, `CoverageBar`,
   `EnergyEstimatePanel`, `UnsavedDraftBadge`. Six ops screens improve the moment
   `DataTable` lands.
3. **App shell** — brand lockup, header, ops sidebar, field tab bars, demo banner.
4. **Screens**, in the numbered order of the redesign file: Tower (01), energy drill (02),
   register (03, 08), verify + people (04), farmer (05, 10), ops details (06), auth (07),
   officer home and record screens (09), ops tables (11), demand (12), remaining drills (13).
5. Run `pnpm typecheck`, `pnpm test`, `pnpm e2e` after each screen group.

---

## Responsive requirements — non-negotiable

The redesign must be correct at **320px, 375px, 768px, 1024px, 1440px**. Test all five.

- **Farmer and officer surfaces are mobile-first.** Single column, bottom tab bar fixed to
  the viewport, `main` keeps `pb-20` to clear it. Everything is full-width; nothing has a
  fixed pixel width. Primary buttons are `w-full` with `min-height`, never `height`.
- **Ops is desktop-first with a mobile fallback.** The sidebar sits beside the content at
  `lg` and above. Below `lg` it becomes a horizontally scrollable strip above the content —
  this was a bug fix (QA #8): at 375px the fixed sidebar squeezed the content column until
  figures were cut mid-number. Collapsing is not hiding; every destination stays reachable.
- **Tower tiles**: a wrapping flex row, not a fixed grid. Production `flex: 1 1 430px`,
  Energy `1 1 350px`, Market `1 1 340px`, Pipeline `1 1 280px`, Quality `1 1 230px`, all
  with `min-width: 0`. They land two-up on desktop and stack cleanly on a phone without a
  media query. The energy tile's prospective/approved cells wrap to stacked rows on narrow
  widths **and keep the rule between them** — they must never merge into one block.
- **Tables** live in `overflow-x-auto` wrappers and scroll inside themselves. The page must
  never scroll sideways. Numeric columns stay right-aligned and `tabular-nums` at every
  width.
- **Label/figure rows are grid tracks** (`minmax(0,1fr) auto`), never `justify-between` on
  a flex row: the label must wrap to two lines while the figure stays aligned to the right
  edge. This is what makes the Kiswahili build survive.
- **Content widths** stay as they are: `max-w-xl` forms, `max-w-lg` farmer detail,
  `max-w-3xl` opportunity, `max-w-2xl` demand create. The Tower is two-column at `lg`.
- **Nothing may depend on a string being short.** Kiswahili runs 10–30% longer. Buttons
  wrap (`text-wrap: balance`), chips wrap, no `nowrap`, no fixed heights on anything
  holding text, no truncation with ellipsis on a figure or a status.
- **Touch targets**: 48px minimum on field surfaces, 44px on ops. Tab bar items are 60px.
  Verify buttons in a list row are 48px.

---

## Performance — this is a field constraint, not a preference

Users are on low- to mid-spec Android phones over 3G, and the PWA precaches the shell.

- **No animation, no transition, anywhere.** No fades, slides, shimmer skeletons, spinner
  sweeps, or scroll effects. A state change is a repaint, not a performance. Loading
  skeletons are flat blocks sized to match what replaces them, so a tile landing never
  reflows the grid.
- **No `box-shadow`, no `filter`, no `backdrop-filter`, no blur.** Depth is borders plus
  white-on-sand contrast. (The one exception in the design is an `inset` box-shadow used as
  a 3px active-nav rule; a `border-left` is an equally acceptable implementation.)
- **No images.** Icons are inline `lucide-react` components; hatching, marks and meters are
  CSS gradients and borders. The only raster-free asset is the brand SVG.
- **No new web fonts, no additional weights.**
- Prefer static CSS over JS-driven layout. Nothing should measure the DOM to lay itself out.

---

## Accessibility to fix while you are in there

- Contrast: `text-deep/60` on the sand ground measures **4.17:1** and is used for every
  secondary string in the app. Replace it per the token table — `--ink-2` (6.6:1) for
  secondary, `--ink-3` (4.8:1) for notes. Nothing renders below 12px.
- Keyboard focus: every interactive element gets a visible focus ring
  (`outline: 2px solid` the primary, `outline-offset: 1–2px`). The browser default is not
  enough on this ground.
- Keep `role="alert"` on the 7 error surfaces, `role="alertdialog"` on the release
  confirmation, `role="meter"` with `aria-valuenow/min/max` on coverage bars, `aria-sort`
  on sortable headers, `aria-busy` on loading tiles, `aria-invalid` + `aria-describedby` on
  invalid fields.
- Verification state must never be conveyed by colour alone — that is why each state has
  its own mark shape. Preserve the shapes.
- Add a skip link to the shell.

---

## Definition of done

- `pnpm typecheck`, `pnpm test`, `pnpm e2e`, `pnpm db:rls` all green.
- Every route in section 14's coverage index matches its design section at 375px and 1440px.
- No `data-testid` deleted or moved to a different value.
- No new dependency in `package.json`; no new font file; no `@keyframes` and no
  `transition:` in `src/`.
- The seeded demo figures still read exactly: 12,000.00 kg expected · 9,000 demand /
  5,600 available / 62.2% coverage · 500.000 kW planned, basis Planned · prospective
  7.200 kW · approved 10.800 kW · headroom 489.200 kW · simultaneity 0.6 · 3/6 verified,
  3/4 with GPS, 5/7 with an estimate.

If anything in the design cannot be built without breaking one of the constraints above,
**stop and say so** rather than adding a dependency or weakening a product-language rule.
