# Storefront

Catalog read for apps holding an app key with `catalog:read` (see `app-keys.md`). Code: `src/modules/storefront/`.

Only sellable web products show: `status = active`, not archived, not deleted, not `store_only` (sold at the till only, see `till.md`). Anything else, an id of another workspace, or a malformed id is `404`.

## `GET /v1/storefront/products?page&page_size`

`200 {"data": [product...], "meta": {current_page, from, to, last_page, per_page, total}}`, ordered by id. `page_size` defaults to 50, max 100.

## `GET /v1/storefront/products/{product}`

`200 {"data": product}`.

## Product

```json
{
  "id": "…", "name": "Ilundo honey", "brand": null, "type": "physical",
  "short_description": null, "description": null, "ean13": null, "barcode": null,
  "price_gross": 1250, "price_net": 1033, "currency": "EUR", "currency_decimals": 2,
  "available_stock": "7.50",
  "variants": [
    { "id": "…", "sku": "H-500", "barcode": null, "ean13": null, "is_default": true,
      "price_gross": 1250, "price_net": 1033, "options": [{ "type": "Size", "value": "500 g" }] }
  ]
}
```

- **Prices** are integers in minor units of `currency` (`round(price × 10^currency_decimals)`, half away from zero). `price_gross` includes VAT. Base prices only: special offers and price corrections are not applied yet.
- **`available_stock`** is on hand minus reserved over every live stock row, as a decimal string, or `null` when the product isn't stock-tracked. Stock is per product (variants have none).
- **Never shown:** purchase, wholesale or cost prices, supplier data, internal references or notes, stock locations. A test pins the exact key set.

One SQL statement per call (variants, options, stock and currency joined in), so no N+1.

Errors: `401` missing/unknown/revoked key, `403` key lacks `catalog:read`, `404` as above.
