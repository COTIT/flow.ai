# Quick search

## `GET /v1/workspace/{workspace}/quick-search?q&limit`

The office top bar: one call returns a few rows of each kind. Permission `workspace.global-search` to call (403 without; 404 for a non-member or unknown workspace).

- **`q`:** required, 3 to 100 characters after trimming (422 on `q` otherwise). Contains-match, any case; `%` and `_` match themselves.
- **`limit`:** rows per group, default 5, at most 10. Junk falls back to 5.

200:

```json
{ "data": {
  "products":  [{ "id": "…", "name": "Raw honey", "product_nr": "P-00042", "ean13": null }],
  "orders":    [{ "id": "…", "number": "ORD-0007", "reference": "PO 77", "status": "open",
                  "total": 2725, "currency": "EUR", "decimals": 2 }],
  "invoices":  [{ "id": "…", "number": "INV-1000", "status": "sent", "customer_name": "Client" }],
  "lots":      [{ "id": "…", "code": "ILU-26-07", "public_code": "7K3M9Q2XAB", "product_name": "Raw honey" }],
  "relations": [{ "id": "…", "name": "Client" }]
} }
```

| Group | Matches | Shown to |
|---|---|---|
| `products` | name, product number, EAN-13, barcode (live products) | `product.list` |
| `orders` | number, reference | `order.list` |
| `invoices` | the digits of `q` equal the invoice number (`INV-1000` and `1000` both find 1000); unnumbered drafts never match | `invoice.list` |
| `lots` | code, public code | any member (as the lot routes) |
| `relations` | company name, alias, a live contact's first or last name (live relations) | `relation.list` |

- A group the caller may not list is `null`, not `[]`. Each group is newest first.
- `customer_name` and relation `name`: company name, else alias, else the first contact's full name.
- Every statement filters the workspace; another workspace's rows never appear.
- **Speed:** trigram (GIN, `pg_trgm`) indexes from migration `quick_search_trgm` serve every contains-match, which is why `q` needs 3 characters. Invoices use the unique `(workspace_id, invoice_number)` index. Five small statements per call.
- `/searches?domain=` (per-domain dropdowns) is unchanged.
