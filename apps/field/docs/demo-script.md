# Demo script — Ruaha 360

There are two ways to run the demo, and they can be combined.

- **Part A — the tour-led run.** Sign in as each role, press the **Tour**
  button, and let the app explain itself, module by module. It only reads, so
  it is safe to run again and again on the shared demo database.
- **Part B — the live chain.** The acceptance journey, done by hand: register a
  farmer, verify, sign in as that farmer, answer a survey, redeem the voucher,
  read the trail. It writes real records, once.

The demo build needs `VITE_FEATURE_RESOURCES=on` (Netlify deploy previews and
branch deploys, and `.env` locally; production leaves it unset). Without it, equipment, loans, requests and the ops catalogue are
hidden and so are their tour chapters — that is the first production release.

Every figure below is asserted by `e2e/journey.spec.ts`, so if the demo shows
something else, the demo is wrong rather than the number.

**Open with the caveats.** They take twenty seconds and they prevent every
misunderstanding that follows.

> Everything on screen is invented demo data — the banner says so on every
> page. Nothing here is a measured Ruaha result. The farmer and officer
> screens have a draft Kiswahili translation that a native reviewer has not yet
> read; ops and the Tower are English.

---

## Sign-ins

Every account uses the password `demo1234`. Switching accounts means signing
out first — a signed-in visitor is sent away from `/login`.

| role | email | lands on | notes |
| --- | --- | --- | --- |
| Field officer, Ilundo | `officer.ilundo@demo.ruaha360.test` | `/officer` | Salima. Registered every seeded Ilundo household, so she may **not** verify those households or redeem their vouchers. |
| Second officer, Ilundo | `officer2.ilundo@demo.ruaha360.test` | `/officer` | Juma. May verify households and redeem vouchers. |
| Field officer, Mgama | `officer.mgama@demo.ruaha360.test` | `/officer` | Peter. |
| Farmer | `neema@demo.ruaha360.test` | `/farm` | Neema Mwakalinga. Two surveys waiting; one voucher already collected, one not. |
| Farmer | `joseph@demo.ruaha360.test` | `/farm` | Joseph Kimaro. |
| Ops | `ops@demo.ruaha360.test` | `/ops` | Asha. Reads everything; cannot author surveys. |
| Admin | `admin@demo.ruaha360.test` | `/ops` | The only account that can write and publish surveys. Same screens as ops, plus the authoring chapter. |

---

## How the tour works

The **Tour** button is in the header (on a phone it is inside the person-icon
menu). The first time someone signs in, a short **Welcome** chapter opens by
itself. After that the button opens a menu:

- **Play the whole tour** — every chapter of that role, in order.
- **One chapter** — jump straight to the module in front of you.

Each stop dims the page, lights up the part being explained, and says what it is
and how to use it. **Next** and **Back** move along (Back returns to the screen
the stop before was on), **Skip the tour** leaves at any time.

A stop tagged **Try it** invites you to use the lit-up part yourself — type in a
search box, filter a list, pick a village, click a voucher. Nothing you try is
kept. Where a stop asks you to do something first, **Next** stays greyed out
with a hint until you have.

**The tour never performs a one-way action.** Verify, Approve, Reject, Redeem,
Publish, Void, Submit and Register are explained and never pressed. It does open
rows (a request, a person, a survey) to show you what is inside, and nothing
else. If a list is empty the tour skips what it would have opened.

### Chapters

| Role | Chapters |
| --- | --- |
| Farmer | Welcome · Your home screen · My farm · Equipment · My requests · Opportunities · Surveys and vouchers · Your voucher · What happens next |
| Officer | Welcome · Your home screen · Registering a farmer · People and their records · Verifying records · Redeeming a voucher · What happens next |
| Ops | Welcome · What is waiting · Equipment requests · Buyer demand and opportunities · Resource catalogue, buyers and villages · Surveys and vouchers · Redemptions and cash counts · Control Tower · What happens next |
| Admin | The ops chapters, plus **Writing a survey** |

*What happens next* only appears on the demo build. It names the next account to
sign in as.

---

## Part A — the tour-led run

Read-only, roughly twenty minutes for all four roles. Run the roles in this
order, because it follows the money: the farmer asks, the officer records, ops
decides, the Tower shows it.

### A1 · Farmer — Neema

Sign in, let the Welcome chapter run, then open the menu and play **Play the
whole tour**, or pick one chapter.

- **Equipment** — open the mill and take the *Try it* stop: change the hours
  and watch the estimate move. Type 99 hours per day and the estimate
  disappears instead of confidently reporting an impossible number. **Say
  why:** an impossible input should not produce a plausible-looking answer. The
  estimate is always labelled an estimate and the price always indicative.
- **Surveys and vouchers**, then **Your voucher** — a survey pays a fixed cash
  amount, once per household, collected at the office. The voucher is a QR and
  a code to show there; it is a claim slip, not money in the app.

### A2 · Officer — Salima

- **Registering a farmer** — the *Try it* stop lets you type into the first
  fields; it is only a draft on the phone (reload and it is still there).
  Phone and farm GPS are now required. Submitting creates all six records at
  once and shows the farmer's login **once**.
- **People and their records** — search and filter (*Try it*), open a person.
- **Verifying records** — explained, not pressed. Verifying cannot be undone,
  and a household you registered needs a *second* staff member.
- **Redeeming a voucher** — scan or type a code, check the ID, hand over the
  cash. You cannot redeem for a household you registered or verified; some
  vouchers are held for ops.

### A3 · Ops — Asha, then Admin

- **Equipment requests** and **Buyer demand and opportunities** — the request
  pipeline, the review screen with planned capacity and headroom (prospective
  and approved demand are never added together), coverage per village, and the
  opportunity. An opportunity is not a sale, a delivery or a payment.
- **Surveys and vouchers** — pick a voucher (*Try it*) to see the full,
  **named** trail behind it. **Redemptions and cash counts** totals what was
  handed over per officer per day.
- **Control Tower** — pick a village (*Try it*), read the headline figures,
  drill from a tile to the records underneath. Capacity is planned, never
  measured.
- **Writing a survey** (admin only) — the editor, the question kinds, and what
  Publish does.

---

## Part B — the live chain

Do this **once**, at the stakeholder demo. It writes real records, and the
demo database has no reset (see the box below).

Order matters: the farmer's household must exist *before* the survey is
published, or it cannot answer it, and it must be verified by someone other
than the officer who registered it.

### 1 · The officer registers a farmer

`/officer/register`, as Salima. *Tour: Registering a farmer.*

One page, one submit, one RPC — not five chained inserts. **Say why:** a phone
that loses signal halfway through five inserts leaves orphan rows across five
tables. This is one transaction, and it is idempotent on a client reference,
so a retry after a timeout cannot create a second farmer.

Worth showing: type three spaces into First name and submit. It is refused
inline. The database would have accepted `'   '` as a name.

On success the screen shows the farmer's **phone and a temporary password,
once**. Note them — they are not shown again.

### 2 · The officer verifies; a second staff member verifies the household

`/officer/verify`, as Salima. *Tour: Verifying records.*

Each record summary is a keyboard-accessible link. The Verify button is a
separate control, so tapping it never navigates. It first asks for
confirmation, then calls `app_verify`; cancelling does nothing. There is no
unverify button because verification is one-way.

On the household row Salima sees "another staff member must verify" instead of
a button. Sign in as **Juma** (or ops) and verify the household there.
**Say why:** four eyes. The person who registered a household cannot be the
person who confirms it.

### 3 · The farmer signs in with the login card

Sign out. On `/login` type the farmer's **phone** and the **temporary
password**. The first sign-in forces a new password of their own.

`/farm/my-farm` shows the same records, each with a provenance badge: where it
came from, who captured it, whether it is verified. **Say why:** a figure with
no provenance is a claim nobody can check.

### 4 · The farmer requests equipment

`/farm/equipment`, open the mill. The estimate moves as the hours change.
*Tour: Equipment.*

Worth showing: type 99 hours per day. The estimate disappears rather than
confidently reporting 1,485 kWh.

### 5 · Ops reviews and approves

Sign in as ops. `/ops/requests` → the request → start review → approve with a
note. *Tour: Equipment requests.*

**Say why:** the status machine lives in a database trigger, not in this
screen. The screen only decides which buttons to draw.

Approval is final. Use the request the farmer just made, **not** the seeded
dryer that is under review — deciding it moves the seeded headroom figures.

### 6 · Ops opens the maize demand

`/ops/demand` → Iringa Grain Traders. *Tour: Buyer demand and opportunities.*

```
Ilundo:  5,600 kg available   62.2% coverage
```

**Say why:** 12,000 kg expected, 6,400 already committed. The superseded 3,200
kg estimate is excluded — a revised figure never reaches a total, and the old
one stays auditable.

### 7 · Ops creates an opportunity and attaches supply

From the coverage row. Then attach a harvest figure.

Worth showing: attach a quantity already committed elsewhere. The refusal
names the actual kilograms. **Say why:** that message is written to be read,
and it is shown exactly as the database wrote it.

**Say clearly:** an opportunity is not a sale, not a delivery, not a payment.
"Accepted" means both sides agreed to keep talking.

### 8 · The Tower reflects all of it

`/ops/tower`, Ilundo. *Tour: Control Tower.*

```
production   12,000.00 kg expected
energy       500.000 kW planned capacity, basis: planned
             prospective peak  7.200 kW
             approved peak    10.800 kW
             headroom        489.200 kW
market       9,000 kg demand · 5,600 kg available · 62.2%
```

**Say why, and this is the point of the whole demo:** prospective and approved
demand are never summed — one is an application, the other is a decision.
Capacity is planned, never measured. And every headline drills to the records
underneath it: production → a crop cycle → the farmer registered in step 1, in
three clicks.

### 9 · The admin publishes a survey; the farmer answers once

Sign in as **admin**. `/ops/surveys` → New survey → add a question or two →
Publish. *Tour: Writing a survey.* (The seeded draft *Seed varieties* can be
published instead of writing a new one.)

Sign in as the farmer. The **Surveys** tab shows a badge. Answer the survey —
it can be answered **once** — and the voucher appears: a QR, a code, and the
fixed cash amount to collect at the office. *Tour: Surveys and vouchers, Your
voucher.*

**Say clearly:** the incentive is a fixed cash amount per household per survey,
paid at the office. It is not earnings, not a balance, not a wallet, and not a
price.

### 10 · The registering officer may not redeem it; a second officer does

`/officer/redeem`, as **Salima** (who registered the household): enter the
voucher code. She is refused — "You cannot redeem this voucher" — because she
registered the household.

Sign in as **Juma**: same code. Choose the ID type seen (never the number),
tick the name check, confirm. *Tour: Redeeming a voucher.* Try the same code
again: blocked, with who handed it over and when.

**Say why:** anti-collusion. A household is verified by someone other than its
registrar; its voucher is redeemed by a third person; a random share of
vouchers is held for ops or admin; and every step is on an append-only trail
with a name on it.

### 11 · Everyone sees the same trail

- The **farmer** opens the voucher: collected, and by whom.
- **Ops / admin**: `/ops/surveys` → the survey → click the voucher → the full
  named trail (registered, verified, login created, published, answered,
  issued, scanned, refused, redeemed with the ID type). Then
  `/ops/surveys/redemptions` for cash handed over per officer per day.
  *Tour: Surveys and vouchers, Redemptions and cash counts.*

---

## What a live run changes

There is no "reset demo". Part A changes nothing. Part B does:

| Step | What it leaves behind |
| --- | --- |
| 1–2 | A new person, household, farm, plot, crop cycle and expected harvest, and a farmer login. |
| 5 | A decided request. **Never** approve or reject the seeded under-review dryer: it moves the asserted headroom figures. |
| 7 | A new opportunity and supply lines. |
| 9 | A published survey, an answer and a voucher. Publishing the seeded *Seed varieties* draft changes its status for good. |
| 10 | A redeemed voucher and audit events, including the refusal. **Do not** redeem the seeded vouchers (Neema's energy voucher, Joseph's maize voucher): `pnpm db:rls` asserts their totals. |

After a live run, `pnpm db:rls` and the seeded-figure e2e specs can fail until
the seeded rows are restored. `e2e/README.md` explains the safety rule the
automated suite follows: it marks everything it creates and never verifies,
decides or redeems a seeded record.

**Seeded shortcuts that write nothing:** Neema's *Irrigation* voucher is already
collected (by Juma, with a NIDA card), so opening it shows a finished trail;
her *Energy* voucher and Joseph's *Maize storage* voucher are issued but
uncollected. Opening any of them is read-only.

---

## Questions worth expecting

**"Can we put real farmers in?"** Not yet. Consent and registration are open
research items, and this instance holds demo data only. The demo instance and
any live instance would be different databases — there is no flag to flip.

**"Is the Swahili right?"** It is a draft. The farmer and officer screens are
fully in Swahili, written from a sourced glossary and cross-checked, but no
native reader has reviewed it yet. `docs/i18n-handover.md` is the reviewer's
file, and it says which strings are still drafts and which carry a doubt. Ops
and Tower are English.

**"What does it cost to run?"** Out of scope for this demo: finance terms,
interest, deposits and repayment are unresolved, and Bank of Tanzania Tier 2
classification is an open research item.

**"Can it work offline?"** Partly, and honestly. An interrupted registration
survives a reload with every field intact and a visible "not yet submitted"
badge. There is no background sync queue — that is deferred until field
testing proves it necessary, rather than built on a guess.

**"Is the survey money in the app?"** No. The incentive is a fixed cash amount
per household per survey, handed over at the office against a single-use
voucher. There are no wallets, balances or payment rails.
