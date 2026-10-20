# Products API

This covers the product catalog: brands, categories, products with prices and price history, and variants with option types. It is ported from vimdesk-api `src/Domain/ERP/{ProductBrand,ProductCategory,Product}`.

Shared rules (guard order, 404s, patch updates, list `meta`, money printing) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}`.

## Brands `/erp/product-brands` and categories `/erp/product-categories`

| Route | Permission |
|---|---|
| list / get / create / update / delete | `productbrand.{list,get,create,update,delete}` (Laravel checked `.view` for one; the catalog has `get`) |
| same for categories | `productcategory.*` |

- **Laravel never saved these writes.** No listener handled them: create answered 500, and update and delete answered 200 without changing anything. genesy stores them. No screen writes them today; special offers and discounts read them through `/searches`.
- **Brand:** `{id, product_brand_id, name, description, logo, website, meta, created_at, updated_at}`.
  - `name` is required, ≤255.
  - `logo` and `website` must be valid URLs, ≤2048. Laravel validated 2048 but stored 255.
  - `meta` is an object.
- **Category:** `{id, product_category_id, name, description, meta, created_at, updated_at, relationships}`.
  - On GET one, `relationships` holds `parent` and `children`. Lists print `[]`, because Laravel loaded nothing there.
  - `parent_id` must be a live category of this workspace, and never the category itself or one of its descendants. Laravel had no cycle guard. The error text is "The parent id is an invalid id for the product_category domain."
- **List:**
  - Both lists take `q`, `page` and `page_size`.
  - Categories also take `parent_id` (uuid) or `root_only=1`.
  - `/searches?domain=product_brand|product_category` uses the same list shape.

## Products `/products`

| Route | Permission | Answer |
|---|---|---|
| `GET /` | `product.list` | `{data:[…], meta}` |
| `POST /search` `{query}` | `product.list` (Laravel had no check) | `{data: product}` or 404 |
| `GET /{id}` | `product.view` | `{data: product}` |
| `POST /` | `product.create` | `201 {data:{product_id}}` |
| `POST /{id}` | `product.update` | `{"message":"ok"}` |
| `DELETE /{id}` | `product.delete` | `{"data":{"message":"The Product has been deleted"}}` |
| `POST /{id}/duplicate` | `product.duplicate` | `201 {data:{product_id}}` |
| `GET /{id}/price-history` | `product.view` | `{data:[…], meta}`, `per_page` default 25 |
| `GET /{id}/sale-statics` | `product.view` | zeros (no invoices yet) |
| `GET /{id}/linked-products` | `product.view` | `{data:{products:[]}}` (links can't be created yet) |
| `POST /{id}/sync-product-stock-locations-from-external-service` | `product.view` | `{"message":"ok"}`: Picqer isn't ported, which is Laravel's "not configured" answer |

The last three answer every product page's automatic calls. Without them the page shows error toasts, and the sales block spins forever.

### Resource

The product resource follows Laravel `ProductResource`; see `src/modules/products/resource.rs` for every key.

- **One shape for list and detail:** list rows also carry the detail-only keys.
- **`default_warehouse`:** the full warehouse resource on GET one and search, the compact form in lists.
- **Prices are strings, printed by `money::format_decimals` the way Laravel's `MoneyHelper` did:**

  | Fields | Print |
  |---|---|
  | `price`, `original_price`, `price_without_correction`, `business_price` | 4 dp, zero `"0.00"` (computed in Laravel) |
  | `*_incl` (incl VAT), `gross_price_incl` | 2 dp, zero `"0.00"` |
  | `gross_price`, `wholesale_price`, `supplier_wholesale_price`, `impact_on_price`, `impact_on_wholesale_price`, `purchase_price`, `supplier_purchase_price_from_discount` | 4 dp, zero `"0"` (raw column), null `"0.00"` |
  | `weight` | 2 dp; null prints `"0.1"` (Laravel default) |
  | `order_unit`, `sale_unit` | raw 4 places (`"1.0000"`) |

- **VAT:** incl VAT is `net × (100 + vat_amount) / 100`, with `vat_amount` from the product's VAT group.
- **Not ported yet** (final batch F3): special offers, price corrections and discount groups. Until then:
  - `price == original_price == price_without_correction == business_price`
  - `purchase_price` is wholesale when it has a markup or is > 0, else the supplier's wholesale price
  - `purchase_discount_percentage` is null
- **Stock is not ported yet (6F):**
  - `stocks` holds zero fallback rows (`"id": "fallback-<warehouse>"`) for the default warehouse and the workspace's first store.
  - `total_available_stock` is 0.
  - `pos_stock_status` is `unavailable` when the product should keep stock, otherwise `not_applicable`.
- **Other keys:**
  - `has_stock` and `archived` print 0/1, as Laravel left them uncast.
  - `images`, `special_offers` and `custom_sections` are `[]`.
  - `vat_group_id` is the group's uuid.

### Create and update

- **Required on create:** `name` and `price` (numeric ≥ 0). An invalid value gets only its own error, not an extra "required".
- **Validated fields** (Laravel `Store/UpdateProductRequest`):
  - **Text:**
    - `brand` ≤255
    - `reference` / `reference_vendor{,2,3}` / `ean13` ≤100
    - `hs_code` / `barcode` / `supplier_discount_code` ≤55
    - `barcode_vendor` / `short_description` ≤255
    - `description` ≤1500
  - **Enums:**
    - `type` is one of digital / multi_product_group / multi_product_set / physical / physical_second_chance / service / giftcard, default physical
    - `status` is one of active / disabled / nla, default `active`. Laravel answered 500 without it.
    - Both are lowercased, with the error "The selected status: X is invalid."
    - `unit`, `warranty_*`
  - **Money:**
    - `gross_price`, `wholesale_price`, `supplier_wholesale_price`, `weight`, `order_unit`, `sale_unit` are numbers ≥ 0
    - `impact_on_price` is any number
    - `impact_on_wholesale_price` ≤100
    - `url_vendor*`: URL on create
  - **References:** `supplier_relation_id`, `vat_group_id`, `sales_group_id`, `default_warehouse_id` and `currency_id` must be live rows of this workspace.
  - **Bundles:** `multi_set_product_ids` `[{id, quantity ≥ 1}]` is required for bundle types, with at most 100 children. Each child must be a live product of the workspace other than the product itself.
- **Defaults:**
  - `currency_id`: the default currency, else the oldest. With none at all it is 422 "This currency id does not exist".
  - `vat_group_id`: the **default group**, else the oldest. This is a user decision; Laravel took the oldest only.
  - `wholesale_price` and `supplier_wholesale_price` are 0 on create, as in Laravel.
- **Physical fields:** `dimensions` (`{length,width,height}` numbers), `has_stock` and `is_serialized` only stick on physical types. The type is judged after the request; Laravel needed `type` in the same request.
- **Update is a patch:** absent keys keep their value. Laravel nulled `supplier_relation_id` and `default_warehouse_id` when they were absent. A null `price` keeps the stored price.
- **Normalisation:** with a gross price > 0, price and markup follow each other.
  - A changed price sets `impact = round(price/gross×100 − 100, 4)`, with the price rounded to cents.
  - Otherwise the price follows: `round(gross × (1 + impact/100), 2)`.
  - On create the price always wins, as in Laravel.
- **Numbers:** `PRO-00001`, MAX+1 per workspace under an advisory lock. Deleted products keep their number.
- **Default variant:** create also writes it. It copies the product's codes and prices, with its markup normalised against the product price (0 when the price is > 0).
- **Price history:** create writes a row for each non-null `gross_price` / `price` / `supplier_wholesale_price` / `wholesale_price`, for the product and for its default variant. Update writes only the changed fields. The flow is `manual` from the API and `system` for duplicate.
- **Duplicate** creates the product again from its stored columns, with a new number.
- **Delete** is soft. **There is no "used in items" guard yet:** order items arrive in the final batch, which must add it.

### List and search

- **Filters:**
  - `status` (default `active`)
  - `q`, or `search` (the RN app's name): ILIKE on name, reference, vendor reference, EAN, barcode and product number
  - `supplier_id` / `supplier_relation_id` (uuid)
  - `ean13` (exact)
  - `exclude_ids` (comma-separated)
- **Paging:** `limit` / `per_page` / `page_size`, max 100.
- **Order:** newest first.
- **Laravel differences:** Laravel ignored `ean13`, `search`, `per_page` and `exclude_ids`. The RN app's barcode fallback `?ean13=X` relied on them and got an arbitrary product.
- **Scale:** the `q` search scans the workspace. Measured on Postgres 18 at 100 000 products: about 70 ms for a search, about 30 ms for a plain page. No trigram index was added; add one if a workspace far exceeds this.
- **`POST /search`:**
  - An exact, case-insensitive match on barcode, EAN, vendor barcode or the product id. The literal `Shift` that some scanners type is stripped.
  - Several matches prefer an active product, then the newest. Laravel hard-coded warehouses 7 and 8 here.

## Variants `/products/{product}/variants`

| Route | Permission | Answer |
|---|---|---|
| `GET /` | `product.view` (Laravel: none) | `{data:[…]}`, not paginated, oldest first |
| `GET /{variant}` | `product.view` (Laravel: always 403) | `{data: variant}` |
| `POST /` | `product.create` | `201 {data:{variant_id}}` |
| `PUT /{variant}` | `product.update` | `{data: variant}` |
| `DELETE /{variant}` | `product.delete` | 204 |
| `POST /generate` | `product.create` | `201 {data:[variants]}` |

- **Resource** (Laravel `ProductVariantResource`):
  - `price` and `gross_price` print 4 dp with a raw-column zero. The other prices print 4 dp, or null when null. `weight` prints 2 dp or null.
  - `relationships.product` is the list-shape product.
  - `relationships.option_values[]` holds `{id, workspace_id, option_type_id, value, display_value, sort_order, …, relationships.option_type{id,name,display_name}}`.
  - `relationships.images` is `[]`.
- **Body:**
  - `price` (≥ 0) or `impact_on_price` is required: "The price field is required when impact on price is not present."
  - Optional: the other prices, `weight`, `sku`, `barcode`, `ean13`, `reference_vendor{,2,3}` (≤255), `is_default`, `meta`.
  - Update is a patch with the same rules.
- **Prices:**
  - The markup is normalised against the **product's price** as gross, as in Laravel.
  - `is_default: true` makes the variant the product's only default.
  - Each change writes price-history rows with the variant id.
- **Delete:** a variant is reached only through its own product. Laravel ignored the product in the path. The default variant can't be deleted (422); Laravel answered 500.
- **Generate:**
  - `option_types[{name, sort_order?, values[{value, sort_order?}]}]` reuse existing types and values by lowercased name or value; the typed text is kept as display text.
  - `combinations[{option_value_ids, sku?, price?, stock?}]`. Ids resolve via `"{value}-{type#}-{value#}"`, the plain value, or a trailing index pair. A combination that resolves nothing is skipped.
  - `price` falls back to `default_price`, then the product price. `sku` falls back to `sku_pattern` with `{index}`. `meta.stock` takes `stock`, else `default_stock`.
  - At most 20 types, 100 values per type and 500 combinations, all in one transaction.

## Not ported

- **Product routes:**
  - archive/unarchive, `relations/{id}` customer price
  - imports, export requests, the public `v1/export/*` routes (no auth and string-built SQL in Laravel)
  - Picqer create, matches (`GET /{id}/matches` stays 404; the SPA catches it)
  - labels, second chance, order/store advice, statics
- **Link routes:** linked-products POST/DELETE.
- **Variant routes:** `variants/by-options` (always 403 in Laravel) and option-type CRUD (no callers).
- **Features:** special offers, discounts, discount groups, price corrections, serial numbers, stock (6F), images, custom fields, the activity log and Meilisearch.
