# Genesy

One shared backend and four apps, in one repo.

| Folder | What it is | Who uses it |
|---|---|---|
| `apps/api` | Rust backend: all business rules and the database | every app |
| `apps/shop` | Public web shop, product pages, QR batch pages | shoppers |
| `apps/office` | Back office: products, stock, batches, invoices, orders | staff |
| `apps/field` | Farmer registration, harvests, passbooks | field officers, farmers |
| `apps/till` | Point of sale and cash payouts | shop and office counters |
| `packages/ui` | Shared shadcn components and theme | all four apps |
| `packages/api-client` | Backend connection (axios) and TanStack Query provider | all four apps |
| `packages/eslint-config`, `packages/typescript-config` | Shared lint and TypeScript settings | every package |

Ports: shop 3000, office 3001, field 3002, till 3003, api 8080.
Each app reads the backend address from `NEXT_PUBLIC_API_URL`: copy its `.env.example` to `.env.local`.

Add a shadcn component once for every app: `pnpm dlx shadcn@4.23.0 add <name> -c apps/office` (it lands in `packages/ui`).

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
