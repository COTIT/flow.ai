# Genesy

One shared backend and four apps, in one repo.

| Folder | What it is | Who uses it |
|---|---|---|
| `apps/api` | Rust backend: all business rules and the database | every app |
| `apps/shop` | Public web shop, product pages, QR batch pages | shoppers |
| `apps/office` | Back office: products, stock, batches, invoices, orders | staff |
| `apps/field` | Farmer registration, harvests, passbooks | field officers, farmers |
| `apps/till` | Point of sale and cash payouts | shop and office counters |
| `packages/*` | Shared UI, API client and config | the apps |

## Requirements

- Node 22 or newer, pnpm 9 (`corepack enable`)
- Rust (stable) and Postgres for `apps/api`

## Commands

```bash
pnpm install
pnpm dev        # run every app
pnpm build      # build everything
pnpm lint
pnpm typecheck
pnpm test
```

Run one app: `pnpm --filter shop dev`.
