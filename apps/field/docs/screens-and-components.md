# Ruaha 360 — screens and components

Status: **specification, nothing built.**
Authority: Ruaha 360 Overview Plan v2 (S07, S08, S11, S12, S15).
Companion to `schema.md` — every data reference here is a real table, view or
column from the migrations. Nothing in this document invents a field.
Target: MVP demo, 30 September 2026.

---

## 0 · How to use this document

Written to be handed to an implementing agent alongside `schema.md` and the
`supabase/` migrations. Rules of engagement:

1. **Do not invent tables, columns, views or enum values.** If a screen seems
   to need one that does not exist, stop and raise it. The schema is
   deliberate; the gap is probably the point.
2. **RLS is the security boundary.** Route guards are convenience only. Never
   add a client-side check as a substitute for a policy, and never work around
   a policy that returns zero rows — that result is the specification.
3. **Do not add libraries** beyond the stack listed in §2. No chart library
   until the Tower renders real numbers. No state manager beyond TanStack
   Query. No ORM.
4. **Build tier by tier.** T1 first, all of it, before any T2. The T1 set is
   exactly the acceptance journey and nothing else.
5. **Validated against the cloud dev project.** The migrations are applied
   there and recorded; `pnpm db:rls` must pass before UI work that depends on
   a policy. There is no local Supabase (see `supabase/README.md`).

---

## 1 · Tiers

| tier | meaning |
|---|---|
| **T1** | Required for the acceptance journey to pass end to end. Build all of it first. |
| **T2** | Needed for the demo to feel complete. Build only after every T1 screen works. |
| **T3** | Post-demo. Do not build. |
| **cut** | Explicitly out of MVP. Do not build even if a reference screen shows it. |

**Working order: make it work → make it right → make it fast.**
T1 is "make it work". Validation depth, empty/error polish and accessibility
are "make it right". Visual design is deliberately last, with one exception
in §9.

---

## 2 · Stack (fixed)

```
Vite + React + TypeScript          SPA, no SSR
TanStack Router                    typed routes, search-param state, guards
TanStack Query                     all server state
Tailwind + shadcn/ui               UI primitives
TanStack Table                     ops tables and Tower drill-downs
react-hook-form + Zod              forms; Zod schemas shared with Edge Functions
react-i18next                      Swahili + English, JSON in repo
vite-plugin-pwa                    cached shell, opens offline
@supabase/supabase-js              the only data client
```

Generated database types are committed and imported. No hand-written row types.

---

## 3 · Route map

```
/login                         public
/select-role                   multi-membership picker
/no-access                     zero memberships

/farm                          _farmer  · mobile-first · Swahili
  /farm/my-farm
  /farm/equipment
  /farm/equipment/$equipmentId
  /farm/requests
  /farm/requests/$requestId
  /farm/opportunities

/officer                       _officer · mobile-first · bilingual
  /officer/register
  /officer/people
  /officer/people/$personId
  /officer/farms/$farmId
  /officer/cycles/$cycleId
  /officer/verify

/ops                           _ops     · desktop-first · English
  /ops/requests
  /ops/requests/$requestId
  /ops/catalogue
  /ops/buyers
  /ops/demand
  /ops/demand/$demandId
  /ops/opportunities/$opportunityId
  /ops/villages
  /ops/tower
  /ops/tower/production
  /ops/tower/energy
  /ops/tower/market
```

One layout route per surface. `beforeLoad` reads the session's memberships and
redirects if the role does not match. The Tower lives **inside** `_ops` —
executive is an Ops view in MVP, not a fourth role.

---

## 4 · Shell

### 4.1 App shell — T1
- Role-aware nav. Farmer and Officer get a bottom tab bar; Ops gets a sidebar.
- Language switch, persisted to `app_user.locale`.
- **Demo banner**, always visible, driven by `VITE_DATA_MODE=demo`. Never by a
  database column — the demo instance and the live instance are different
  databases.
- Sign out.

### 4.2 `/login` — T1
- Email + password via `supabase.auth.signInWithPassword`.
- On success: query `membership` for `auth.uid()` where `revoked_at is null`.
  - 0 rows → `/no-access`
  - 1 row → that role's home
  - 2+ rows → `/select-role`
- **States:** idle · submitting · invalid credentials · network error.
- **Acceptance:** each of the six seeded accounts lands on the correct home.

### 4.3 `/select-role` — T1
- Lists memberships as role + project + village. Selection stored in memory
  and in the URL, not in a token.

### 4.3a `/set-password` — surveys feature (29 Sep 2026)
A farmer signing in with the temporary password from their login card lands
here first — every surface guard sends them. New password twice → the auth
server stores it → `app_password_changed` clears the flag. Until then the
database refuses their survey answers. Password rules are the auth server's;
its message is shown as it says it. `/login` accepts a phone number or an
email.

### 4.4 `/no-access` — T1
- A real screen with a next step ("ask your programme manager to add you"),
  never a blank page or a crash.

---

## 5 · Officer surface

The origin of nearly all data. Mobile-first, bilingual, interrupted-save
tolerant.

### 5.1 `/officer` — home — T1
- **Data:** `village` (via `app_villages()`), counts from `person`, `farm`,
  `pue_request`.
- Assigned villages, unverified record count, primary action **Register**.
- **States:** loading · zero villages assigned · error.

### 5.2 `/officer/register` — T1 — the most important screen in the build
One page, one submit. Creates person + household + farm + plot + crop_cycle +
expected harvest_report together.

- **Writes via a single RPC** — `app_register_farmer` (specified in the
  business-rules document). **Not** five chained client inserts: a phone that
  loses signal mid-chain leaves orphan rows across five tables.
- **Sections**, all on one scrollable page:
  1. Person — `given_name`, `family_name`, `phone` (optional)
  2. Household — `label`, is this person the head
  3. Farm — `label`, GPS point (optional), link to household
  4. Plot — `label`, `area_ha`, and title documents (photos or PDFs of the
     title deed, staff only, added 5 Oct 2026). Picked files wait in their
     own IndexedDB draft and are filed against the plot once the RPC returns
     its `plot_id`; a file that fails is named and offered again. Person
     detail shows each plot's documents for the verifier.
  5. Crops — tick every crop grown on the plot (added 5 Oct 2026). Each
     crop gets its own card: measure (branches on its `crop.measured_by`),
     `harvest_start`, `harvest_end`. One `crop_cycle` per crop.
  6. Expected harvest — `quantity_kg` per crop, one `confidence`
- Every section writes `source = 'field_verified'` and `captured_by` = current
  user. Provenance is never optional and never a user-facing choice here.
- **Draft persistence:** the whole form autosaves to IndexedDB keyed by a
  client-generated draft id. Restores on mount. Cleared only after the RPC
  returns success.
- **States:** empty · restoring draft · saving · **saved-locally-not-submitted**
  · submit failed with retry · success.
- **Acceptance:** kill the network mid-form, reload, the form comes back with
  every field intact and a visible "not yet submitted" badge.

### 5.3 `/officer/people` — T2
- `DataTable` over `person` scoped by RLS. Search by name, filter by
  `verification`.

### 5.4 `/officer/people/$personId` — T1
- Person detail: household members, farms, plots, cycles, provenance on each.
- **Verify** action → sets `verification = 'verified'`, `verified_by`,
  `verified_at`. Officer only.
- **Acceptance:** verifying flips the `ProvenanceBadge` on this screen and on
  the farmer's My Farm without a manual refresh (invalidate the query key).

### 5.5 `/officer/farms/$farmId` — T2
Plots list, add plot, GPS capture, farm-level provenance.

### 5.6 `/officer/cycles/$cycleId` — T2
Cycle detail plus harvest reports. Adding a new expected figure **supersedes**
the previous one: set the old row `is_current = false`, insert the new row
`is_current = true`. The partial unique index enforces this — handle the
constraint error, do not pre-check.

### 5.7 `/officer/verify` — T2
Queue of records where `verification in ('unverified','pending')` in the
officer's villages.

### 5.7a `/officer/redeem` — surveys feature
Scan the farmer's voucher QR (`qr-scanner`, typed code always available) →
`app_voucher_lookup` (logs the scan) → preview: household, head, respondent,
survey, incentive, status. If the database says this caller may not redeem —
they registered or verified the household, or the voucher is held for an
ops audit — the reason is shown and there is no confirm. Otherwise: ID type
seen + name matches → `app_voucher_redeem` → receipt with the audit trail.
Ops and admin reach this screen too.

Registration's success state shows the **farmer login card** once: phone and
temporary password. Person detail shows the login history and "Reset app
password". Neither screen offers Verify on a household the viewer
registered (business-rules §5).

### 5.8 Visits — **T3**. Not in MVP.

---

## 6 · Farmer surface

Mobile-first, Swahili-first, low-end Android. Read-heavy. Keep this bundle
lean — it runs on the weakest hardware in the system.

### 6.1 `/farm` — home — T2
Status summary, latest request status, any opportunity the farmer's supply is
inside.

### 6.2 `/farm/my-farm` — T1
- **Data:** `farm` → `plot` → `crop_cycle` → `harvest_report` (current
  expected), all via `app_farms()` scoping.
- Read-only. Household, plots with area, crops with expected harvest.
- Every figure carries a `ProvenanceBadge`.
- **States:** loading · **no farm yet** (a real message, not an empty list) ·
  error.
- **Acceptance:** the farmer sees exactly her own farm. Seeded farmer Neema
  sees one farm and cannot reach Joseph's.

### 6.3 `/farm/equipment` — T1 (Equipment and loans since 5 Oct 2026)
- **Nav label:** "Resources" / "Rasilimali" (`nav.equipment`, the word ops
  uses for the same catalogue); the screen title is "Equipment and loans".
  Likewise the My farm tab is "Shamba" in Swahili while its heading stays
  "Shamba langu". Every farmer and officer tab label is held to one line on a
  375px phone, in both languages, by `e2e/shell.spec.ts`.
- **Data:** `equipment` joined to `equipment_category`, and `loan_product`,
  both scoped by `app_projects()`. Tabs Equipment | Loans, held in the URL
  (`?kind=loan`).
- Equipment cards: name in the active language, `rated_power_kw`, category,
  how it is offered (rent / buy), `indicative_price` when it can be bought and
  `indicative_rent_per_day` when it can be rented.
- Loan cards: name, description, indicative range, "not an offer", "ask at
  the Ruaha office". No request control: a loan is not requested in the app.
- The word **indicative** appears next to every price, rent and amount. It is
  not a quotation and not an offer.

### 6.4 `/farm/equipment/$equipmentId` — T1
- Detail plus **Request this** form: `quantity`, `hours_per_day`,
  `days_per_week` (prefilled from `typical_*`), `purpose`.
- Live `EnergyEstimatePanel` recalculating as the inputs change, labelled as an
  estimate.
- **Rent or buy:** the modes the machine is offered in. One offered: shown,
  nothing to choose. Both: the farmer must choose. Recorded as
  `pue_request.acquisition`; `pue_request_acquisition_check` refuses a mode
  not offered. The estimate is the same either way.
- Submit inserts `pue_request` with `status = 'submitted'`.
- **Acceptance:** changing hours from 6 to 8 visibly changes the estimate
  before submit, and the stored `energy_estimate` row after submit matches.

### 6.5 `/farm/requests` and `/farm/requests/$requestId` — T1 / T2
- List with `StatusPill`. Detail shows the equipment, the assumptions, the
  stored estimate, and the decision note when decided.
- A submitted request is **not editable**. The trigger enforces it; the UI
  must not offer the control.

### 6.6 `/farm/opportunities` — T2
Opportunities where this farmer's supply is in `opportunity_supply`. Must state
plainly that an opportunity is not a sale.

### 6.6a `/farm/surveys` and `/farm/surveys/$surveyId` — surveys feature
The Surveys tab carries a count of surveys the household may answer now —
the only notification in the MVP. Each card: title, description, incentive
("paid in cash at the Ruaha office"), question count, and New / Answered /
Not available with the database's reason. The survey screen is the answer
form (draft survives a reload; submitted once per household) or, once
answered, the voucher: QR code, printed code, amount, status, expiry, and
who handed it over, with the farmer's view of the audit trail.

### 6.7 Progress · Training · Services — **cut**.

---

## 7 · Ops surface

Desktop-first, English, table-heavy.

### 7.1 `/ops` — home — T2
Queue counts: requests awaiting review, open demands, unverified records.

### 7.2 `/ops/requests` — T1
- `DataTable` over `pue_request` with `equipment`, `person`, `energy_estimate`.
- Filter by `status` and village. **Filter state lives in the URL** as validated
  search params — that is why TanStack Router is in the stack.
- Columns: applicant · village · equipment · est. kW · status · submitted.

### 7.3 `/ops/requests/$requestId` — T1
- Full request, applicant, farm, the estimate with its snapshotted inputs, and
  village headroom from `v_village_energy`.
- **Actions:** Start review (`submitted → under_review`), Approve, Reject —
  both requiring a `decision_note`.
- The status machine is enforced by `pue_request_guard`. The UI offers only
  legal transitions and surfaces the trigger's error message verbatim if one
  is raised.
- **Acceptance:** approving updates the farmer's request view and moves
  `approved_peak_kw` in `v_village_energy`.

### 7.4 `/ops/catalogue` — the resource catalogue (5 Oct 2026)
Tabs Equipment | Loans, held in the URL. Equipment: power, typical hours,
offered as (rent / buy), indicative price, indicative rent per day. Loans:
name, description, indicative range, "not an offer".
- **Action:** add a row — equipment (code, names in both languages, category,
  power, typicals, rent / buy, price, rent per day) or a loan listing (code,
  names, descriptions, smallest and largest amount). Just enough of a form; no
  edit or delete. Loans carry no finance terms (research item C).

### 7.5 `/ops/buyers` — T2
Buyer list and create. `channel = 'afm'` is a label only; no integration.

### 7.6 `/ops/demand` — T1
List of `buyer_demand` plus a create form: buyer, crop, `quantity_kg`, window,
`delivery_point`, `indicative_price_per_kg`, `quality_note`.

### 7.7 `/ops/demand/$demandId` — T1
- **Data:** `v_demand_match` for this demand.
- Table of villages with `available_kg`, `coverable_kg`, `coverage_pct`, and
  whether an opportunity already exists.
- **Action:** create an opportunity for a village.
- **Acceptance:** the seeded maize demand shows Ilundo at 5,600 kg available
  and 62.2% coverage. The seeded coffee demand shows **zero matching supply**,
  and the screen says so rather than hiding the row.

### 7.8 `/ops/opportunities/$opportunityId` — T1
- Opportunity with its `opportunity_supply` lines: crop cycle, farmer, plot,
  contributed kg — each a `DrillLink` to the underlying record.
- **Action:** attach supply from `v_harvest_available` for that village, crop
  and window.
- The over-commitment error from `opportunity_supply_guard` is shown to the
  user as written. Do not pre-validate in the client and do not swallow it.
- **Acceptance:** attaching supply already committed elsewhere is refused with
  a readable message.

### 7.9 `/ops/villages` — T2
Village list, `village_capacity` with `basis` and `simultaneity_factor` shown
explicitly.
- **Action (added 5 Oct 2026):** add a village. An inline panel like Demand's
  takes name, code, optional GPS and the first capacity row (planned kW,
  basis, simultaneity factor, effective from, source note). One call,
  `app_create_village`, writes both: a village with no capacity row has no
  energy figures and is missing from the Tower. Officers see the village only
  once a membership assigns it.

### 7.9a `/ops/surveys`, `/ops/surveys/$surveyId`, `/ops/surveys/redemptions` — surveys feature
The survey list with its figures. **Admin** authors: a draft editor
(English + optional Swahili, incentive, household cap, audit share, closing
date, questions of five kinds) and Publish; once live, nothing changes.
Results for everyone staff: households answered, issued / collected / not
yet collected / expired / cancelled, a per-question tally, and every voucher
with its full audit trail; ops or admin may cancel an unredeemed voucher with
a reason. Redemptions: per officer per day, and each redemption, for
reconciling cash.

### 7.10 Translations admin · user admin — **cut.**
Translations are JSON in the repo. User admin is the Supabase dashboard.

---

## 8 · Control Tower

`/ops/tower`, inside the Ops surface. Built **last**, from connected records.

### 8.1 `/ops/tower` — overview — T1
Village selector, then five tiles:

| tile | source |
|---|---|
| Production — expected vs actual kg by crop | `v_village_production` |
| PUE pipeline — count and indicative value by status | `v_village_pue_pipeline` |
| Energy — capacity, prospective peak, approved peak, headroom | `v_village_energy` |
| Market — open demand vs available supply | `v_demand_match`, `v_village_supply` |
| Data quality — verified share, GPS coverage, cycles with estimates | `v_village_data_quality` |

**Non-negotiable labelling on this screen:**
- Capacity is shown with its `capacity_basis`. Never present a planned figure
  as measured.
- Prospective and approved demand are separate numbers, never summed into one
  "demand" figure. An application is not a load.
- `simultaneity_factor` is displayed next to the peak it was applied to.
- `cycle_area_ha` is labelled "planted area across cycles", never "land area" —
  intercropping means it can exceed the village's hectares.

### 8.2 Drill-downs — T1
`/ops/tower/production`, `/ops/tower/energy`, `/ops/tower/market`. Each is a
table that ends in a link to an actual row: a crop cycle, a request, an
opportunity's supply lines.

**Acceptance:** from any Tower headline, reach a single farmer's record in at
most three clicks. If a number cannot be traced, it does not belong on the
screen.

---

## 9 · Components

### 9.1 These carry the product's claim — build first

**`ProvenanceBadge`**
```ts
{ source: SourceType; verification: VerificationStatus;
  confidence?: ConfidenceLevel; capturedAt: string; capturedBy?: string }
```
One chip rendering all of it, with a tooltip giving the long form. Appears on
every record on every surface. The five source values get five distinct
treatments; verification gets its own state. This component is how the
"reported / verified / measured / estimated" discipline actually reaches users.

**`EnergyEstimatePanel`**
```ts
{ ratedPowerKw: number; quantity: number;
  hoursPerDay: number; daysPerWeek: number; editable?: boolean }
```
Shows the inputs, the arithmetic, and the outputs (`kW`, `kWh/day`,
`kWh/week`). When editable, recalculates live. Always labelled as an estimate.
Mirrors `energy_estimate`'s generated columns exactly — if the two ever
disagree, the component is wrong.

**`DrillLink` / `RecordDrawer`**
Turns any aggregate cell into a path to its rows. Used throughout the Tower.

**`CoverageBar`**
Demand against available supply, with the already-committed slice visible.

### 9.2 Forms

**`CropPicker` + `MeasureInput`** — branches on `crop.measured_by`:
`area` → hectares · `tree_count` → integer trees · `unit_count` → integer
units (hives). Switching crop switches the input. Bilingual labels from
`crop.name_en` / `crop.name_sw`. **This is harder than it looks — budget for it.**

**`AreaInput`** — always stores hectares; displays per
`country.default_area_unit`. Conversion happens at the edge, never in storage.

**`HarvestWindowPicker`** — start and end date, validating `end >= start`.

**`GpsCapture`** — one point. States: unsupported · permission denied ·
acquiring · acquired · manual entry fallback.

**`Field`** — react-hook-form + Zod + i18n label + error. Every input goes
through it.

### 9.3 Shell

`DataTable` (TanStack Table wrapper: sorting, URL-synced filters, empty state)
· `StatusPill` (request, demand, opportunity statuses) · `EmptyState` ·
`ErrorState` · `RequireRole` · `LanguageSwitch` · `DemoBanner` ·
`UnsavedDraftBadge` · `ConfirmDialog`.

### 9.4 Hooks

- `useSession()` — auth user, `app_user`, memberships, active role
- `useDraft(key)` — IndexedDB form persistence, returns
  `{ draft, save, clear, status }`
- `withProvenance(payload, source)` — injects `source`, `captured_by`,
  `captured_at` into every observed-table insert. **Write this on day one.**
  Retrofitting provenance into thirty call sites is a bad week.

---

## 10 · Conventions

**Query keys** — `['person', villageId]`, `['farm', farmId]`,
`['requests', { villageId, status }]`, `['tower','energy', villageId]`.
Mutations invalidate the narrowest key that covers the change.

**All writes go through a mutation helper** that applies `withProvenance` and
surfaces Postgres error messages verbatim. Trigger errors
(`pue_request_guard`, `opportunity_supply_guard`) are written to be read by
humans; do not replace them with a generic failure toast.

**Zero rows is a legitimate answer.** RLS returning nothing means "you may not
see this". Render an empty state, never an error, and never retry.

**i18n** — every user-facing string is a key. Farmer and Officer surfaces ship
complete Swahili. Ops and Tower may ship English-only for the demo. Reference
data (crops, equipment, categories) is translated **in the database**, not in
JSON — those rows are created at runtime.

**Numbers** — `tabular-nums` wherever figures align. Weights in kg, power in
kW, energy in kWh, area in ha, money with an explicit currency code.

---

## 11 · The acceptance journey

This is the Playwright test and the demo script. It is the definition of done
for T1.

1. Officer signs in → `/officer/register` → creates person, household, farm,
   plot, crop cycle, expected harvest in one submit
2. Officer opens the person → **verifies** the records
3. Farmer signs in → `/farm/my-farm` → sees the same records with provenance
4. Farmer opens `/farm/equipment` → picks the maize mill → sets hours → sees
   the estimate move → submits
5. Ops signs in → `/ops/requests` → opens it → starts review → approves
6. Ops opens `/ops/demand` → the maize demand → sees Ilundo coverage
7. Ops creates an opportunity → attaches supply lines
8. Ops opens `/ops/tower` → production, energy and market tiles reflect all of
   the above → drills from a headline to that farmer's record

Run against seeded demo data. Also test: permissions per role, and an
interrupted save in step 1.

---

## 12 · Do not build

Training · Services · Progress · translations admin · user admin · photo
upload (one exception, approved 5 Oct 2026: plot title documents, staff only,
see business-rules §19) · farm polygons · offline sync queues · notifications (beyond the
in-app survey badge) · meter screens · crowdfarming · wallets or QR payment
rails (a survey voucher is a single-use cash claim, not a payment — added 29
Sep 2026 by decision) · export or shipment tracking · buyer
self-service accounts · any AI surface · any chart library before the Tower
renders real numbers.

Reference screens from the Control Center and African Farmers Market decks show
several of these. **A screen existing does not put it in scope.**

---

## 13 · The one design decision that cannot wait

Set the brand tokens in the Tailwind theme **before** adding shadcn components:

```
primary   #1d70b7      accent    #93c01f
deep      #0C1F5B      surface   #F5F3E5
font      Poppins
```

shadcn ships slate/zinc defaults and inherits from the theme at add-time.
Twenty minutes now against repainting forty components later. This is not
"make it beautiful" — it is avoiding a repaint.

Everything else visual waits until the journey passes.

---

## 14 · Open

- Swahili copy does not exist yet. Field labels and validation messages need a
  native reviewer before the demo, not a machine translation pass.
- Officer device assumptions untested — no coverage or handset survey has been
  done (research item D).
- Confidence is `low/medium/high` provisionally (S22).
- `season_label` is free text until a season taxonomy is agreed (S22).
