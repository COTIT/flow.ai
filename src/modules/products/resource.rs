//! Product JSON in Laravel ProductResource shape (detail) and ProductIndexResource shape (list).
//! One builder for both: list rows carry the detail-only keys too.
// shortcut: list rows carry detail-only keys; split once offers/images add per-row queries.

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    modules::{
        products::{
            dto::PHYSICAL,
            pricing::{column, computed, incl, purchase_price},
            repo::ProductRow,
        },
        stock::repo::StockLine,
        vat_groups::{dto::VatGroupResource, repo::VatGroupRow},
        warehouses::repo::StockRef,
    },
    money::php_number,
};

/// Everything a page of products embeds, loaded once per page.
#[derive(Default)]
pub struct Embeds {
    pub vat_groups: HashMap<Uuid, VatGroupRow>,
    pub currencies: HashMap<Uuid, Value>,
    pub sales_groups: HashMap<Uuid, Value>,
    pub suppliers: HashMap<Uuid, Value>,
    /// `default_warehouse`: full WarehouseResource on detail, compact on the list.
    pub warehouses: HashMap<Uuid, Value>,
    /// Fallback stock warehouses and the first store.
    pub stock_refs: HashMap<Uuid, StockRef>,
    pub store: Option<Uuid>,
    /// Bundle children per parent, already rendered, with their quantity.
    pub set_children: HashMap<Uuid, Vec<(Value, Decimal)>>,
    /// Live stock rows per product.
    pub stocks: HashMap<Uuid, Vec<StockLine>>,
    /// `?warehouse_id=` for the `pos_*` pick (Laravel read it from the request).
    pub pos_warehouse: Option<Uuid>,
}

/// One product. `detail`: GET one (Laravel loaded more there).
pub fn render(p: &ProductRow, e: &Embeds, detail: bool) -> Value {
    let vat = p.vat_group_id.and_then(|id| e.vat_groups.get(&id));
    let vat_pct = vat.map_or(Decimal::ZERO, |v| v.vat_amount);
    let mutates = PHYSICAL.contains(&p.kind.as_str()) && p.has_stock;
    let lines = e.stocks.get(&p.id).map_or(&[][..], Vec::as_slice);
    let pos = if mutates { pos_pick(p, lines, e) } else { None };
    let price = computed(p.price, 4);
    let price_incl = computed(incl(p.price, vat_pct), 2);
    let purchase = purchase_price(
        p.impact_on_wholesale_price,
        p.wholesale_price,
        p.supplier_wholesale_price,
    );
    let children = e.set_children.get(&p.id).map_or_else(Vec::new, |c| {
        c.iter()
            .map(|(child, qty)| {
                let mut child = child.clone();
                // Laravel printed the raw pivot column: scale 8.
                child["quantity"] = Value::String(raw(*qty, 8));
                child
            })
            .collect()
    });
    json!({
        "id": p.id,
        "purchase_price": column(purchase, 4),
        "name": p.name,
        "brand": p.brand,
        "product_nr": p.product_nr_formatted,
        "reference": p.reference,
        "hs_group": null,
        "reference_vendor": p.reference_vendor,
        "reference_vendor2": p.reference_vendor2,
        "reference_vendor3": p.reference_vendor3,
        "url_vendor": p.url_vendor,
        "url_vendor_override": p.url_vendor_override,
        "ean13": p.ean13,
        "hs_code": p.hs_code,
        "barcode": p.barcode,
        "barcode_vendor": p.barcode_vendor,
        "type": p.kind,
        // Laravel left tinyints uncast here.
        "has_stock": i32::from(p.has_stock),
        "is_serialized": p.is_serialized,
        "warranty_default_mode": p.warranty_default_mode,
        "warranty_duration_value": p.warranty_duration_value,
        "warranty_duration_unit": p.warranty_duration_unit,
        "should_mutate_stock": mutates,
        "vat_group": null,
        "status": p.status,
        // shortcut: no special offers, corrections or business discount settings until the
        // final batch (F3), so every consumer/business variant is the stored net price.
        "business_price": price,
        "business_price_incl": price_incl,
        "impact_on_price": column(p.impact_on_price, 4),
        "impact_on_wholesale_price": column(p.impact_on_wholesale_price, 4),
        "price": price,
        "price_incl": price_incl,
        "price_without_correction": price,
        "price_without_correction_incl": price_incl,
        "import_id": p.import_id,
        "import_source": p.import_source,
        "import_links": [],
        "original_price": price,
        "original_price_incl": price_incl,
        "wholesale_price": column(p.wholesale_price, 4),
        "gross_price": column(Some(p.gross_price), 4),
        "gross_price_incl": computed(incl(p.gross_price, vat_pct), 2),
        "supplier_wholesale_price": column(p.supplier_wholesale_price, 4),
        "purchase_discount_percentage": null,
        "supplier_purchase_price_from_discount": column(p.supplier_wholesale_price, 4),
        "archived": i32::from(p.archived),
        "country_of_origin": p.country_of_origin,
        "weight": p.weight.map_or_else(|| "0.1".to_owned(), |w| column(Some(w), 2)),
        "images": [],
        "stock_level_supplier": p.stock_level_supplier,
        "supplier_discount_code": p.supplier_discount_code,
        "sales_group": null,
        "supplier_discount_group": null,
        "unit": p.unit,
        "order_unit": raw(p.order_unit, 4),
        "sale_unit": raw(p.sale_unit, 4),
        "dimensions": p.dimensions,
        "short_description": p.short_description,
        "description": p.description,
        "second_chance_reason": null,
        "exclude_price_corrections": p.exclude_price_corrections,
        "total_available_stock": php_number(lines.iter().map(|l| l.stock_lvl).sum()),
        // shortcut: no orders or transfer lists yet (final batch), so nothing is reserved.
        "total_reserved_stock": 0,
        "pos_available_stock": php_number(pos.map_or(Decimal::ZERO, |l| l.stock_lvl)),
        "pos_stock_status": match pos {
            _ if !mutates => "not_applicable",
            Some(l) if l.stock_lvl > Decimal::ZERO => "available",
            _ => "unavailable",
        },
        "pos_stock_warehouse_id": pos.map(|l| l.warehouse_id),
        "pos_stock_id": pos.map(|l| l.id),
        "default_warehouse": p.default_warehouse_id.and_then(|id| e.warehouses.get(&id)),
        "stocks": stocks(p, lines, e),
        "supplier_relation": p.supplier_relation_id.and_then(|id| e.suppliers.get(&id)),
        "attributes": null,
        "meta": p.meta,
        "association_data": [],
        "created_at": p.created_at,
        "last_updated_at": p.updated_at,
        "relationships": {
            "currency": e.currencies.get(&p.currency_id),
            "active_special_offer": null,
            "special_offers": [],
            "sales_group": p.sales_group_id.and_then(|id| e.sales_groups.get(&id)),
            "vat_group": vat.map(|v| VatGroupResource::from(v.clone())),
            "custom_sections": [],
            "child_products": [],
            "parent_product": null,
            "return_item": null,
            "child_products_from_set": if detail { Value::from(children) } else { json!([]) },
            "storage_locations": [],
        },
        "vat_group_id": p.vat_group_id,
        "export_enabled": p.export_enabled,
        "export_price_sync": p.export_price_sync,
        "export_product_data": p.export_product_data,
        "export_data": p.export_data.clone().unwrap_or_else(|| json!({
            "title": null, "description_short": null, "description": null,
            "meta_title": null, "meta_description": null, "keywords": null,
        })),
    })
}

/// An uncast Laravel decimal column as MySQL printed it: always `dp` places (`"1.0000"`).
fn raw(v: Decimal, dp: u32) -> String {
    let mut v = crate::money::round(v, dp);
    v.rescale(dp);
    v.to_string()
}

/// Laravel `stocksWithFallbackWarehouses`: the real rows (StockWithoutRelationsResource), then
/// one zero row for the default warehouse and the first store when they have none.
fn stocks(p: &ProductRow, lines: &[StockLine], e: &Embeds) -> Vec<Value> {
    let mut rows: Vec<Value> = lines.iter().map(stock_row).collect();
    let mut have: Vec<Uuid> = lines.iter().filter_map(|l| l.w_id).collect();
    for id in p.default_warehouse_id.into_iter().chain(e.store) {
        if have.contains(&id) {
            continue;
        }
        have.push(id);
        if let Some(w) = e.stock_refs.get(&id) {
            rows.push(fallback_row(w));
        }
    }
    rows
}

/// One real stock row. No `product` key: Laravel didn't load it in product resources.
fn stock_row(l: &StockLine) -> Value {
    json!({
        "id": l.id,
        "stock_lvl": raw(l.stock_lvl, 2),
        "reserved": raw(l.reserved, 2),
        "minimal_stock_level": l.minimal_stock_level,
        "maximum_stock_level": l.maximum_stock_level,
        "warehouse": l.w_id.map(|id| json!({
            "id": id, "name": l.w_name, "alias": l.w_alias, "color": null, "icon": null,
            "is_store": l.w_is_store, "pickup_enabled": l.w_pickup, "location": l.w_location,
        })),
        "shelf": l.shelf_id.map(|id| json!({
            "id": id, "shelf_code": l.shelf_code, "shelf_name": l.shelf_name,
        })),
        "bin": l.bin_id.map(|id| json!({ "id": id, "bin_code": l.bin_code, "bin_name": l.bin_name })),
        "updated_at": l.updated_at,
        "meta": l.meta,
    })
}

/// Laravel `makeFallbackStockRow`.
fn fallback_row(w: &StockRef) -> Value {
    json!({
        "id": format!("fallback-{}", w.id),
        "stock_lvl": "0.00",
        "reserved": "0.00",
        "minimal_stock_level": 0,
        "maximum_stock_level": 0,
        "warehouse": {
            "id": w.id, "name": w.name, "alias": w.alias, "color": null, "icon": null,
            "is_store": w.is_store, "pickup_enabled": w.pickup_enabled, "location": w.location,
        },
        "shelf": null,
        "bin": null,
        "product": null,
        "updated_at": null,
        "meta": null,
    })
}

/// Laravel `posStockPayload`: the highest pickup-enabled row in the asked warehouse; without
/// one asked, in the default warehouse, else anywhere. Ties keep the oldest row.
fn pos_pick<'a>(p: &ProductRow, lines: &'a [StockLine], e: &Embeds) -> Option<&'a StockLine> {
    let best = |wh: Option<Uuid>| {
        lines
            .iter()
            .filter(|l| l.w_pickup == Some(true) && wh.is_none_or(|w| l.warehouse_id == w))
            .fold(None, |best: Option<&StockLine>, l| match best {
                Some(b) if b.stock_lvl >= l.stock_lvl => Some(b),
                _ => Some(l),
            })
    };
    match e.pos_warehouse {
        Some(w) => best(Some(w)),
        None => p
            .default_warehouse_id
            .and_then(|w| best(Some(w)))
            .or_else(|| best(None)),
    }
}

/// Price-history row (Laravel ProductPriceHistoryResource).
pub fn history_row(h: &crate::modules::products::repo::HistoryRow) -> Value {
    let mut row = json!({
        "id": h.id,
        "field": h.field,
        "old_value": h.old_value.map(|v| column(Some(v), 4)),
        "new_value": h.new_value.map(|v| column(Some(v), 4)),
        "changed_by_flow": h.changed_by_flow,
        "created_at": h.created_at,
    });
    if let (Some(first), Some(last)) = (&h.first_name, &h.last_name) {
        row["changed_by"] = Value::String(crate::modules::users::dto::full_name(
            first,
            h.middle_name.as_deref(),
            last,
        ));
    }
    if let Some(id) = h.variant_id {
        row["variant"] = json!({ "id": id, "sku": h.variant_sku });
    }
    row
}
