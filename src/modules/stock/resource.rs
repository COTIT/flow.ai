//! Stock JSON in Laravel's resource shapes (ListProductStockWithBins, ListProductConfiguredBins,
//! StockMovementResource). Quantities print as MySQL's `decimal(19,2)` strings.

use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::modules::stock::repo::{BinsRow, ConfiguredBin, MovementRow};

/// A `decimal(19,2)` column as PDO printed it: always 2 places.
pub fn qty(v: Decimal) -> String {
    let mut v = v;
    v.rescale(2);
    v.to_string()
}

/// PHP `$a ?: $b`: `""` and `"0"` count as empty.
fn or<'a>(a: &'a Option<String>, b: &'a Option<String>) -> Option<&'a str> {
    a.as_deref()
        .filter(|s| !s.is_empty() && *s != "0")
        .or(b.as_deref())
}

/// One `stock-with-bins` row.
pub fn with_bins(r: &BinsRow) -> Value {
    json!({
        "id": r.id,
        "stock_lvl": qty(r.stock_lvl),
        "reserved": qty(r.reserved),
        "minimal_stock_level": r.minimal_stock_level,
        "maximum_stock_level": r.maximum_stock_level,
        "warehouse": r.w_id.map(|id| json!({ "id": id, "name": r.w_name })),
        "shelf": r.sh_id.map(|id| json!({
            "id": id, "shelf_code": r.sh_code, "shelf_name": r.sh_name,
            "row": r.r_id.map(|rid| json!({
                "id": rid, "row_code": r.r_code, "row_name": r.r_name,
                "section": r.sec_id.map(|sid| json!({
                    "id": sid, "section_code": r.sec_code, "section_name": r.sec_name,
                })),
            })),
        })),
        "bin": r.b_id.map(|id| json!({ "id": id, "bin_code": r.b_code, "bin_name": r.b_name })),
    })
}

/// `configured-bins`: bins grouped by warehouse in first-seen order. genesy has no int ids,
/// so `warehouse_id` carries the uuid too (clients key on `warehouse_uuid || warehouse_id`).
pub fn configured(bins: &[ConfiguredBin]) -> Vec<Value> {
    let mut groups: Vec<(Uuid, Option<Uuid>, Vec<Value>)> = Vec::new();
    for b in bins {
        let bin = json!({
            "id": b.id,
            "bin_code": b.bin_code,
            "bin_name": b.bin_name,
            "section": b.sec_id.map(|id| json!({
                "id": id, "section_code": b.sec_code, "section_name": b.sec_name,
            })),
            "row": b.r_id.map(|id| json!({ "id": id, "row_code": b.r_code, "row_name": b.r_name })),
            "shelf": b.sh_id.map(|id| json!({
                "id": id, "shelf_code": b.sh_code, "shelf_name": b.sh_name,
            })),
        });
        match groups.iter_mut().find(|g| g.0 == b.warehouse_id) {
            Some(g) => g.2.push(bin),
            None => groups.push((b.warehouse_id, b.w_id, vec![bin])),
        }
    }
    groups
        .into_iter()
        .map(|(id, uuid, bins)| json!({ "warehouse_id": id, "warehouse_uuid": uuid, "bins": bins }))
        .collect()
}

/// Laravel StockMovementResource `formatLocation`: section.row.shelf, names before codes.
fn location(m: &MovementRow) -> Option<String> {
    m.sh_id?;
    let parts = [
        or(&m.sec_name, &m.sec_code),
        or(&m.r_name, &m.r_code),
        or(&m.sh_name, &m.sh_code),
    ];
    Some(parts.into_iter().flatten().collect::<Vec<_>>().join("."))
}

/// One movement. `updated_at` repeats `created_at`: the ledger is append-only.
pub fn movement(m: &MovementRow) -> Value {
    let name = |first: &Option<String>, middle: &Option<String>, last: &Option<String>| {
        let middle = middle
            .as_deref()
            .filter(|s| !s.is_empty())
            .map_or(" ".to_owned(), |s| format!(" {s} "));
        format!(
            "{}{middle}{}",
            first.as_deref().unwrap_or_default(),
            last.as_deref().unwrap_or_default()
        )
    };
    json!({
        "id": m.id,
        "movement_type": m.movement_type,
        "quantity_change": m.quantity_change.map(qty),
        "quantity_before": m.quantity_before.map(qty),
        "quantity_after": m.quantity_after.map(qty),
        "minimal_stock_level_before": m.minimal_stock_level_before,
        "minimal_stock_level_after": m.minimal_stock_level_after,
        "maximum_stock_level_before": m.maximum_stock_level_before,
        "maximum_stock_level_after": m.maximum_stock_level_after,
        "reason": m.reason,
        "is_correction": m.is_correction,
        "related_domain": m.related_domain,
        "related_id": m.related_id,
        "meta": m.meta,
        "created_at": m.created_at,
        "updated_at": m.created_at,
        "location": location(m),
        "relationships": {
            // shortcut: Picqer isn't ported, so no sync attempt exists.
            "picqer_sync": null,
            "warehouse": m.w_id.map(|id| json!({ "id": id, "name": m.w_name })),
            "shelf": m.sh_id.map(|id| json!({
                "id": id, "shelf_code": m.sh_code, "shelf_name": m.sh_name,
            })),
            "bin": m.b_id.map(|id| json!({
                "id": id, "bin_code": m.b_code, "bin_name": m.b_name,
                "name": or(&m.b_name, &m.b_code), "code": m.b_code,
            })),
            "user": m.u_id.map(|id| json!({
                "id": id, "name": name(&m.u_first, &m.u_middle, &m.u_last), "email": m.u_email,
            })),
            "stock": m.st_id.map(|id| json!({
                "id": id,
                "product_id": m.st_product,
                "warehouse_id": m.st_warehouse,
                "shelf_id": m.st_shelf,
                "bin_id": m.st_bin,
                "stock_lvl": m.st_lvl.map(qty),
                "reserved": m.st_reserved.map(qty),
                "maximum_stock_level": m.st_max,
                "minimal_stock_level": m.st_min,
                "meta": m.st_meta,
                "created_at": m.st_created,
                "updated_at": m.st_updated,
                "deleted_at": null,
            })),
        },
    })
}
