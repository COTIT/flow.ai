# Staging demo seed

`genesy-360-api seed-demo` seeds every part of the product end to end, through the API's own routes in-process, then exits. CLI only: no route ever seeds. Code: `src/demo.rs` (it reuses `src/farm_demo.rs`, the RUAHA farm story).

- **Refuses** unless `GENESY_ALLOW_DEMO_SEED=1` (or `true`). Only the staging compose sets it; production never does.
- **Runs once.** If both `afm-demo` and `ruaha-demo` exist and are demo-flagged, it does nothing. If only one of them exists, it refuses ("half-seeded"). A workspace with a demo name that is not demo-flagged is refused too. To reset, wipe the staging volume and seed again; there is no in-CLI truncate.
- **Secrets.** Every password, PIN and key is random per run and printed once, as one table on stdout (`app | role | login | secret | path`). After the table come the storefront app key the shop container needs, the coffee passport path its QR code encodes, more sample paths (quarantine and recalled passports, shop orders awaiting payment, paid, refunded), and `seeded in N.N s`. Nothing is written to a file or a log line.
- **Runtime.** About 80 s against Postgres in Docker Desktop on a laptop. Only about 6 s of that is CPU; the rest is database round trips. The AFM catalog and the RUAHA records are built side by side. The run prints its own time on the last line.
- **Sizes are fixed.** Names come from fixed tables (`src/demo/names.rs`) and index arithmetic, never from a random generator. Dates are relative to the run.
- **Through the API.** Every row is made by the API's own routes, except:
  - Staff join a workspace by direct insert, so no invite mail goes out.
  - TZS is inserted directly.
  - Invoices are numbered by calling `invoices::send::number`, the send route's own numbering transaction, without its PDF and mail step. Staging has no PDF renderer or file storage.
  - Two vouchers get an expiry date in the past.
  - Waiting web orders keep their hold for 30 days, so the pay page stays live.
  - One date pass (`src/demo/dates.rs`) spreads mutable timestamps over the last 90 days.
- **Dates and ledgers.** Append-only ledgers keep their real times: passbook, sale attributions, payouts, farm cash, the till drawer's `cash_ledger`, voucher events and invoice payments. So everything that writes to them happens in the last 7 days. That covers sales of farm coffee, till sales (`sold_at` up to 7 days back) and payouts. Invoice payments carry their own `date`.

## What it seeds

| Part | Contents |
|---|---|
| AFM catalog | AFM's real store (africanfarmersmarketonline.com, with AFM's consent): 75 products and 96 variants (print, sticker and T-shirt options) from `src/demo/afm_catalogue.json`, made by `scripts/afm-catalogue.mjs` with AFM's titles, descriptions and USD prices. Every product sits in one of 10 categories (Honey, Coffee, Tea, Spices, Baskets, Toys & games, Art, Books, Apparel, Stickers), grouped as AFM's category pages group them. In-store EAN-13s, 4 till-only products (two baskets, two spices). With `--images` (staging), AFM's photos too. AFM's whole-bean coffee gets stock only from the farm's lots. Sales tax 7 % as the default VAT group. Three warehouses: "AFM store" (default store), "Market stall" (store), and "Asheville warehouse" with 2 sections and 48 bins. Stock levels with minimum and maximum. Some products are below their minimum in the store, 5 are sold out, and 2 original paintings go below zero through till sales. |
| AFM customers | 15 businesses (two contacts each) and 25 households (one contact each), all with US addresses and `.test` emails. |
| AFM orders | Office orders in every status: draft, open, completed, cancelled, partially returned and returned. Web orders that are paid and shipped, still waiting for payment, failed, refunded and cancelled, or partly refunded. About 120 till orders. Catalogue orders spread over 90 days; orders with farm coffee stay in the last week. |
| AFM invoices | From completed orders: drafts, sent, partly paid, overdue (due 30 days after sending) and paid. Three paid invoices are credited by a credit note. Plus three direct invoices: one draft and two cancelled. |
| AFM pricing | A purchase discount group with two discounts. Three special offers: one running, one starting in three weeks, and one always on. |
| AFM lots | Lots of AFM's food in every passport state: Mary's Tanzanian Wildflower Honey (13 oz and 7 oz, from the Illundo beekeepers), Roselle and Kizazi tea, Rift Valley coffee, cardamom. 4 published, 2 with a draft passport, 1 honey lot in quarantine and 1 recalled (both published, so the public page shows the banner), and 1 expired. Passport text comes from AFM's own descriptions. Plus the farm's received coffee lots (`AFM-CF-…`) on AFM's Africa's Wildest Whole Bean Coffee, published. |
| AFM tills | "AFM counter" and "Market stall", four cashiers (Riley is inactive), a back-office safe. Shifts over the last 7 days: 8 closed, two short and one over, plus one open on the counter now. The stall has no open shift. Cash and card sales, three of them at an old price (price mismatch). |
| AFM settings | US notes and coins for drawer counts. A revoked app key. Two webhook endpoints on `example.com`, one inactive, made last so the seed fires no delivery. |
| RUAHA farm | The farm demo (`farm_demo.md`), plus three villages (Kitowo, Lugoda, Mwangeza), an officer for each, a second Mgama officer, and Hamisi, who holds two roles (ops and Ilundo officer). 60 people in 47 households with farms and plots, about 110 crop cycles over four seasons in every status, expected and actual harvests with revisions. About 70 % are verified by ops. |
| RUAHA surveys | 8 more surveys: 2 drafts, 4 live and 2 closed, with yes/no, choice and number questions. Six farmers can sign in and answer them. Their vouchers are redeemed, voided, expired or still open. |
| RUAHA energy and market | 6 equipment types, 3 loan listings, PUE requests in every status. 5 buyers, 6 demands, opportunities that are proposed, shared, accepted, declined and lapsed, with supply attached. |
| Supply chain | A revoked RUAHA → AFM link, the live one, and a pending AFM → RUAHA link. Three coffee batches bought from verified harvests in Ilundo, Kitowo and Lugoda, packed into 120 bags each and transferred. Two are received into published AFM lots; one is still in transit, so the office shows Receive. Ilundo's batch leaves its last 4 verified coffee harvests unbought, and Mgama's verified maize is never bought. Both are verified by the RUAHA admin, so either ops user (four eyes) finds them in Field's Buy harvest. |
| Money to farmers | Web, office and till sales of the coffee credit about a dozen farmers. Hamisi pays five of them out at the desk; one payout is voided. His cash float and count are recorded. |

## Logins

| App | Role | Login | Path |
|---|---|---|---|
| office (AFM) | owner | `afm.owner@afm-demo.test` | `POST /v1/login` |
| office (RUAHA) | admin | `amina.admin@ruaha-demo.test` | `POST /v1/login` |
| field (RUAHA) | ops, field officers | `asha.ops@`, `salima@`, `juma@`, `peter@`, `rose@`, `daudi@`, `zuhura@`, `upendo@ruaha-demo.test` | `POST /v1/login` |
| field (RUAHA) | ops plus field officer | `hamisi.ops@ruaha-demo.test` | `POST /v1/login` |
| field (RUAHA) | farmer (one password for all) | `0712000001`, `0712000003`, `0712000004`, `0714001001`, `0714002001`, `0714003001` | `POST /v1/farm/login` |
| till (AFM) | device keys, 3 cashier PINs | registers "AFM counter" and "Market stall"; Zawadi, Baraka, Jordan | `POST /v1/workspace/{ws}/till/login` |

## Not seeded (no route or no service on staging)

- Product images only with `seed-demo --images` and file storage (staging's image store, `deploy/staging/README.md` "Image store"). Passport banners, certificate files, farm plot documents and bug reports: never.
- The invoice PDF: staging has no renderer.
- A till pairing code: it is one-time and lasts 10 minutes. Make one from the office.
- Demand that is matched, closed or cancelled, an inactive buyer, a disputed record: no route sets these.
- A village without capacity: capacity is required when a village is created.
