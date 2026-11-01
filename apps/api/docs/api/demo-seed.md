# Staging demo seed

`genesy-360-api seed-demo` seeds every part of the product end to end, through the API's own routes in-process, then exits. CLI only: no route ever seeds. Code: `src/demo.rs` (it reuses `src/farm_demo.rs`, the RUAHA farm story).

- **Refuses** unless `GENESY_ALLOW_DEMO_SEED=1` (or `true`). Only the staging compose sets it; production never does.
- **Runs once.** If both `afm-demo` and `ruaha-demo` exist and are demo-flagged, it does nothing. If only one of them exists, it refuses ("half-seeded"). A workspace with a demo name that is not demo-flagged is refused too. To reset, wipe the staging volume and seed again; there is no in-CLI truncate.
- **Secrets.** Every password, PIN and key is random per run and printed once, as one table on stdout (`app | role | login | secret | path`), followed by the storefront app key the shop container needs and the passport path its QR code encodes. Nothing is written to a file or a log line.

## What it seeds

| Part | Contents |
|---|---|
| AFM shop `afm-demo` | Owner. USD as the default currency. A store warehouse. "Ruaha coffee 250 g". A storefront app key (`catalog:read`, `checkout:write`). |
| RUAHA farm `ruaha-demo` | The farm demo: villages with capacity, registrations, surveys, vouchers. Plus the equipment catalogue, a loan listing, PUE requests (Ilundo approved peak 10.800 kW), buyers, demand and an opportunity. |
| Supply chain | A live supply link RUAHA → AFM. Asha's verified coffee harvest is bought into a raw lot, packed into 40 bags, transferred and received at AFM into a lot with a published passport (`/b/{public_code}`). |
| Sales | A paid web order of 2 bags (storefront, Fake provider) that is picked, completed and invoiced. A till cash sale and a till card sale. All of them credit the farmer. |
| Payout | Juma, an officer who neither registered nor verified Asha's household, pays out her balance at the desk. |
| Till | Register "AFM counter" (device key) and two cashiers with PINs; a shift opened with a float. |

The paid web order goes through the seed's own in-process Fake provider, so its `order_payments` row says `fake`. Staging runs the Fake provider as well.

## Logins

| App | Role | Login | Path |
|---|---|---|---|
| office (AFM) | owner | `afm.owner@afm-demo.test` | `POST /v1/login` |
| office (RUAHA) | admin | `amina.admin@ruaha-demo.test` | `POST /v1/login` |
| field (RUAHA) | ops, field officers | `asha.ops@`, `salima@`, `juma@`, `peter@ruaha-demo.test` | `POST /v1/login` |
| field (RUAHA) | farmer | `0712000001` | `POST /v1/farm/login` |
| till (AFM) | device key, 2 cashier PINs | register "AFM counter", Zawadi, Baraka | `POST /v1/workspace/{ws}/till/login` |
