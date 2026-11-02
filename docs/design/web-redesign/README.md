# Web redesign — office, shop, till

Approved design (2026-11-03): glass look **A · Frosted mist**, office **sidebar** navigation, till sale **A · tiles + basket**.
Field and landing are out of scope and must look exactly as before.

## Two rules for every app session (from the user)

1. **A 1:1 copy of the approved design.** Every mocked screen matches its mockup exactly: spacing, radius, glass tints, type sizes, tile layout and copy placement. No reinterpretation. When in doubt, open the artboard's `.dc.html` in [`canvas/`](canvas/) and copy its values.
2. **Pages without a mockup follow the closest approved template, never the old look.** The [route table](#6-every-route--its-mockup-or-template) below says which template each route inherits.

Mockups: Claude Design canvas https://claude.ai/artifact/GEEtyDyy6ujsDoDJ9tACPa (private; ask the owner for access).
Source of every artboard is in [`canvas/`](canvas/) — open a `.dc.html` file to read the exact markup, spacing and copy.
[`canvas/glass.css`](canvas/glass.css) is the mockup stylesheet; the real tokens below follow it.

## 1. Switch the look on

Put `theme-glass` on `<body>` in the app's root layout:

```tsx
<body className="theme-glass">
```

That one class (defined in `packages/ui/src/styles/globals.css`) gives:

| What | How to use |
|---|---|
| Soft "mist" ground behind the page (static, light + dark) | automatic on `body.theme-glass` |
| Rounder corners (`--radius: 0.75rem`) | all `rounded-*` utilities follow |
| Glass card | `glass` utility, or just use shadcn `Card` (it is glass under `theme-glass`) |
| Stronger glass for bars, sidebar, sheets | `glass-strong` (dialogs, sheets, menus, selects get it automatically) |
| Tinted tile inside a card, table header, list row (never blurred) | `glass-subtle` |
| Tokens | `--glass`, `--glass-strong`, `--glass-subtle`, `--glass-solid`, `--glass-border`, `--glass-highlight`, `--glass-shadow`, `--glass-blur` (12px), `--ground`, `--ground-base`, `--tone-info/success/warning/error` |

Rules:
- Blur turns into a solid fill by itself where the browser can't blur or the user asks for less transparency. Text contrast is tested on that solid fill (`tokens.test.ts`).
- Keep at most ~3 blurred layers on screen. Never put `glass` on rows or tiles inside a scrolling list; use `glass-subtle`.
- Gold is never text on a light ground (use it as fill, rule, or on navy). Dark mode: gold is the primary button.
- `Notice` and `EmptyState` keep their API; under `theme-glass` they get the new shape and coloured icon chip.

## 2. Components in `@genesy/ui/components/*`

Existing: `button`, `input`, `label`, `textarea`, `table`, `badge`, `dialog`, `alert-dialog`, `confirm-dialog`, `notice`, `empty-state`, `api-error`, `pager`, `logo`, `theme-toggle`, `passport-view`, `login-form`.

New (shadcn base-nova on `@base-ui/react`, no new packages): `card`, `tabs`, `select`, `sheet`, `skeleton`, `separator`, `dropdown-menu`, `checkbox`, `switch`, `toggle`, `toggle-group`, `tooltip`, `avatar`, `scroll-area`, `sidebar` (+ hook `@genesy/ui/hooks/use-mobile`).
- `tooltip` needs a `TooltipProvider` in the layout; a tooltip only labels, it is never the only way to an action.
- `sidebar` needs `SidebarProvider`; on phones it opens as a sheet.
- Loading = `Skeleton` blocks in the final shape (no layout shift), not a "Loading…" line.

## 3. Animations: `island` (`@genesy/ui/components/island`)

```tsx
"use client"
import { island } from "@genesy/ui/components/island"

// module level, once:
const CountUp = island(() => import("../rb/CountUp"))

<CountUp to={12} plain={<>12</>} />
```

- The animated component is fetched in its own chunk, on the client only, and only when motion is allowed. Server render, reduced motion and load errors all show `plain`.
- Make `plain` the same size as the animated result (no layout shift).
- The animated piece sits in `[data-motion]`, which e2e uses to prove nothing animates under reduced motion.
- **`motion` 12.43.0 is approved (exact pin, same as landing) for office and shop only.** Each of those two apps adds it to its own `package.json` when it copies in a React Bits piece that needs it (CountUp, BlurText). `packages/ui` and the till never depend on it; `island` works with any component. Copy React Bits files into the app (e.g. `apps/office/components/rb/`) with the licence note, like `apps/landing/components/rb/README.md`.
- Where: office = CountUp on overview numbers (once). Shop = BlurText headline, GlareHover on product photos (pointer: fine only), passport tiles fade in. **Till = none, ever**; no animation code in its bundle, and no transitions on sale actions.

## 4. E2E (`@genesy/e2e`)

- Projects: every app has `<app>-desktop` (1280×800) and `<app>-mobile` (375×812). The till also has `till-ipad-landscape` (1024×768) and `till-ipad-portrait` (834×1194), both touch (`projectsFor(app, { tablets: true })`).
- From `@genesy/e2e/test`, next to `expectNoHorizontalScroll`:
  - `expectTapTargets(page, { min = 44, within? })` — every visible control is at least 44×44 px.
  - `expectNoLayoutShift(page, max = 0.01)` — call when the page has settled.
  - `expectNoMotion(page)` — with `test.use({ reducedMotion: "reduce" })`.
- The checks have their own tests: `tests/helpers/checks.spec.ts` (project `helpers`).

## 5. Per app: where to look in the mockups

| App | Artboards in `canvas/` | Notes |
|---|---|---|
| Office | `OfficeOverview` (bento + sidebar), `OfficeProducts` (list template; rows become cards at 375), `OfficeProduct`, `OfficeLot`, `OfficeOrder` (bento detail), `OfficePassport` (editor + live preview; Edit/Preview tabs at 375), `OfficeSettings` (list + "copy it now" dialog), `OfficeLogin`, `OfficeStates` | Overview at `/` uses list endpoints with `per_page=1` and reads `meta.total`; no client sums. "Sales this week" tile is not built (needs an API summary). Dialogs become bottom sheets at 375. |
| Shop | `ShopCatalogue`, `ShopProduct` (bento), `ShopCart`, `ShopCheckout`, `ShopOrder` (`status` = awaiting / paid / cancelled), `ShopPassport` (bento; `status="recalled"` puts the recall banner first), `ShopStates` | Cart shows unit prices only; totals come from the server at checkout. |
| Till | `TillSale` (`layout="a"`; `sheet` = cash / card / receipt), `TillLogin` (user tiles + PIN pad; `step="setup"` pairs the device), `TillDrawer` (`step` = count / result), `TillQueue` | iPad first. Keys 72–84 px, every target ≥ 44 px, no hover-only actions. Portrait: basket docks at the bottom. Cash notes come from `register.accepted_cash`. |

Wrapper artboards (`*Dark`, `*Phone`, `*Portrait`, …) only re-mount these with other props.

## 6. Every route → its mockup or template

**Mock** = copy that artboard 1:1. **Template** = no own mockup; build it from the named template artboard (same shell, card, spacing, table, states), changing only the content.

Templates: **List** = `OfficeProducts` (page head + one card holding filters, table, pager; rows become cards at 375). **Detail tiles** = `OfficeProduct` / `OfficeLot` / `OfficeOrder` (12-column bento of cards). **Form card** = the "Details" card of `OfficeProduct`. **Dialog** = the dialog in `OfficeSettings` (bottom sheet at 375). **Auth** = `OfficeLogin` / `TillLogin`. **States** = `OfficeStates` / `ShopStates`. **POS panel** = `TillSale` sheet / `TillDrawer` card.

### Office (sidebar shell from `OfficeOverview` on every signed-in page)

| Route | Mock / template | Notes |
|---|---|---|
| `/` | Mock `OfficeOverview` | Replaces today's redirect. KPI counts from list endpoints with `per_page=1` → `meta.total`. Sidebar item "Overview". Skip the "Sales this week" tile. |
| `/login` | Mock `OfficeLogin` | Session-ended info Notice as drawn; errors as error Notice above the fields. |
| `/products` | Mock `OfficeProducts` | |
| `/products/[id]` | Mock `OfficeProduct` | "Change stock" opens a Dialog; images card = today's image editor. |
| `/stock` | Template List | Segmented "All / Below zero" in the card header (as in the system board), switching to `/stock/negative`. |
| `/stock/negative` | Template List | Same page, "Below zero" selected; quantities in the red badge tone. |
| `/orders` | Template List | Status filter = shadcn Select in the card header; status as Badge. Columns like `OfficeOverview` "Recent orders". |
| `/orders/[id]` | Mock `OfficeOrder` | Pick, returns, refund = Dialog; complete, cancel = confirm dialog (see `OfficeStates`). "Next step" card shows the main action big. |
| `/invoices` | Template List | Search in card header; status Badge. |
| `/invoices/[id]` | Template Detail tiles | `s-8` lines card with totals at its foot (as `OfficeOrder`), `s-4` cards for status + PDF actions and customer; payments as full-width table card. |
| `/lots` | Template List | Search + status Select + product Select in card header; "New lot" primary button in page head opens Dialog with the `OfficeLot` "Lot details" fields. |
| `/lots/[id]` | Mock `OfficeLot` | Recall → confirm dialog (`OfficeStates`). Pack into jars → Dialog. |
| `/lots/[id]/passport` | Mock `OfficePassport` | Certificates and materials = more cards in the left column, same list-row style as "Story blocks". |
| `/supply` | — | Redirect to `/supply/links` (unchanged). |
| `/supply/links` | Template List | Supply sub-tabs (Links / Transfers) = `tabs` style in the page head. Accept / Revoke = confirm dialog. |
| `/supply/transfers` | Template List | Same tabs. "Receive" → Dialog. |
| `/till` | — | Redirect to `/till/registers` (unchanged). |
| `/till/registers` | Template List | Till sub-tabs (Registers / Users / Vaults / Shifts / Sales) in the page head. New register → Dialog; pairing code shown like the `OfficeSettings` secret box. Revoke → confirm dialog. |
| `/till/users` | Template List | Add user / reset PIN → Dialog; deactivate → confirm dialog. |
| `/till/vaults` | Template List | New vault → Dialog. |
| `/till/shifts` | Template List | Difference column in gold (short/over) or teal (balanced) badge tone. |
| `/till/sales` | Template List | "Only mismatches" = Switch in the card header. |
| `/settings/app-keys` | Mock `OfficeSettings` | |
| `/settings/webhooks` | Template List (as `OfficeSettings`) | New / edit endpoint → Dialog; secret shown once like app keys; delete → confirm dialog. |
| `error.tsx` | Template States | Error Notice with "Try again" inside a single card, in the shell when signed in. |
| `not-found.tsx` | Template States | EmptyState card with a link back to Overview. |

### Shop (header + footer from `ShopCatalogue` on every page)

| Route | Mock / template | Notes |
|---|---|---|
| `/` | Mock `ShopCatalogue` | Pager as drawn. Empty / past-end = EmptyState card. |
| `/products/[id]` | Mock `ShopProduct` | Variants as the size buttons; sold out → warning Notice in the buy box, Add disabled. |
| `/cart` | Mock `ShopCart` | Empty cart = `ShopStates` "Empty cart". |
| `/checkout` | Mock `ShopCheckout` | Empty cart → same EmptyState. |
| `/orders/[id]` | Mock `ShopOrder` (`status` awaiting / paid / cancelled) | Other statuses: same card with a neutral info Notice. Slow payment = `ShopStates` warning with "Check again". |
| `/orders/[id]` error | Template States | `ShopStates` "Shop unavailable" error Notice in the order card. |
| `/b/[code]` | Mock `ShopPassport` (+ `ShopPassportRecalled`) | Each section hidden when empty; quarantine → warning Notice in the recall-banner slot. |
| `/b/[code]` not found | Mock `ShopStates` "Passport not found" | |
| `error.tsx` | Mock `ShopStates` "Shop unavailable" | |
| `not-found.tsx` | Template States | EmptyState card, "Go to the shop". |

### Till (iPad; top bar from `TillSale` on signed-in pages)

| Route | Mock / template | Notes |
|---|---|---|
| `/` | — | Redirect to `/sale` (unchanged). |
| `/setup` | Mock `TillSetup` (`TillLogin step="setup"`) | Errors as error Notice above the code field; success Notice then go to login. |
| `/login` | Mock `TillLogin` | No users → EmptyState in the users card. Session ended → info Notice in the PIN card. |
| `/sale` | Mock `TillSale` (A), `TillPayCash`, `TillPayCard`, `TillReceipt` | Portrait = `TillSalePortrait`. No open shift → warning Notice above the tiles with a link to Drawer. Offline → warning Notice like `TillQueue`, sale saved → warning in the receipt sheet. Catalog loading = Skeleton tiles. |
| `/drawer` | Mock `TillDrawer` (+ `TillDrawerResult`) | No shift → same card with "Open shift" and the count rows. Balanced → success Notice instead of warning. |
| `/queue` | Mock `TillQueue` | Empty → EmptyState "All offline sales are booked". |
| `error.tsx` | Template POS panel | Centered `glass-strong` card (as `TillDrawerResult`) with error Notice, "Try again", "Back to sale". |
| `not-found.tsx` | Template POS panel | Same card with info Notice and "Back to sale". |
[`parts/`](parts/) holds the office/shop page bodies the canvas files were assembled from.
