# Storefront

Catalog read for apps holding an app key with `catalog:read` (see `app-keys.md`). Code: `src/modules/storefront/`.

Only sellable web products show: `status = active`, not archived, not deleted, not `store_only` (sold at the till only, see `till.md`). Anything else, an id of another workspace, or a malformed id is `404`.

## `GET /v1/storefront/products?page&page_size&category`

`200 {"data": [product...], "meta": {current_page, from, to, last_page, per_page, total}}`, ordered by name (then id). `page_size` defaults to 50, max 100. `category=<id>` keeps the products of that category; an unknown, deleted, malformed or another workspace's id gives an empty list (`total` 0), never an error.

## `GET /v1/storefront/categories`

`200 {"data": [{"id", "name", "description", "product_count"}]}`, ordered by name. Only live categories holding at least one product this API lists; `product_count` counts those products (store-only, inactive, archived and deleted ones don't count). One statement, at most 500 rows.

## `GET /v1/storefront/products/{product}`

`200 {"data": product}`.

**Images.** A product carries `images` (its own) and each variant `images` (that variant's): `[{url, expires_at}]` in the order the office set (`products.md`, images). Links are signed per request and expire (`expires_at`): cache the reply for less than that, or read again. A deleted file drops out; a server without file storage gives `[]`. Two statements per page, not per product.

## Product

```json
{
  "id": "…", "name": "Ilundo honey", "brand": null, "type": "physical",
  "short_description": null, "description": null, "ean13": null, "barcode": null,
  "price_gross": 1250, "price_net": 1033, "currency": "EUR", "currency_decimals": 2,
  "available_stock": "7.50", "stock_status": "in_stock",
  "labels": ["Raw", "Unheated"],
  "category": { "id": "…", "name": "Honey" },
  "passport": { "public_code": "7K3M9Q2XAB" },
  "images": [],
  "variants": [
    { "id": "…", "sku": "H-500", "barcode": null, "ean13": null, "is_default": true,
      "price_gross": 1250, "price_net": 1033, "options": [{ "type": "Size", "value": "500 g" }] }
  ]
}
```

- **Prices** are integers in minor units of `currency`. `price_net` is the catalog price excl VAT (`round(price × 10^currency_decimals)`, half away from zero; a variant's own price for a variant). `price_gross` is one unit of that price incl the product's VAT group (0 % without one), priced like a one-line checkout, so it equals what `POST /storefront/orders` charges for quantity 1. The office's `gross_price` column (a list price excl VAT) plays no part. Base prices only: special offers and price corrections are not applied yet.
- **`available_stock`** is on hand minus reserved over every live stock row, as a decimal string, or `null` when the product isn't stock-tracked. Stock is per product (variants have none).
- **`stock_status`:** `sold_out` (available ≤ 0), `low` (at most the workspace setting `product.low_stock_threshold` units, default 5, set through `POST /settings`) or `in_stock`; `null` when not stock-tracked. The rule lives here, so the shop shows a "few left" badge without its own threshold.
- **`labels`:** the product's short public labels in order (`products.md`), `[]` without.
- **`category`:** the product's main category `{id, name}` (`products.md`, `category_id`), `null` without one or when the category was deleted.
- **`passport`:** `{public_code}` of the lot the shop sells next: the product's published, released, unexpired lot with free stock (level − reserved > 0) and the earliest of expiry and best-before (undated last), over every warehouse. Read the page with `GET /storefront/lots/{public_code}`. `null` when no lot qualifies (no lots, none published, recalled, expired or empty).
- **Never shown:** purchase, wholesale or cost prices, supplier data, internal references or notes, stock locations. A test pins the exact key set.

- **`variants`:** default first. A product sold in sizes (any live variant carries an option) lists only those sizes: the option-less variant made with the product is left out, as it is no size the shop can offer. A product without sizes lists its default.

One SQL statement per call (variants, options, stock and currency joined in), so no N+1.

Errors: `401` missing/unknown/revoked key, `403` key lacks `catalog:read`, `404` as above.
