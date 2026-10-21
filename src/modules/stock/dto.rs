//! Stock request bodies (Laravel MutateStockRequest, ...).

use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::Deserialize;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, human},
    money,
};

/// Laravel WarehouseExist rule message.
pub const NO_WAREHOUSE: &str = "This warehouse id does not exist";

/// Validated `POST /products/{id}/stocks`.
pub struct Mutate {
    pub warehouse_id: Uuid,
    pub bin_id: Option<Uuid>,
    pub mutation_value: Decimal,
    pub is_correction: bool,
    pub reason: Option<String>,
    pub meta: Option<Value>,
}

/// POST /products/{id}/stocks. `reserved` isn't read: reservations arrive with orders.
#[derive(Deserialize)]
pub struct MutateBody(Map<String, Value>);

impl Validate for MutateBody {
    type Valid = Mutate;

    fn validate(self, r: &mut Rules) -> Option<Mutate> {
        let m = &self.0;
        let warehouse_id = uuid_of(r, m, "warehouse_id", NO_WAREHOUSE, true);
        let bin_id = uuid_of(r, m, "bin_id", "The selected bin id is invalid.", false);
        let mutation_value = r
            .required_value("mutation_value", m.get("mutation_value").cloned())
            .and_then(|v| r.decimal("mutation_value", &v, -money::MAX, money::MAX));
        let is_correction = filled(m.get("is_correction").cloned())
            .and_then(|v| r.boolean("is_correction", &v))
            .unwrap_or_default();
        let reason = text(r, m, "reason", 255);
        if is_correction && reason.is_none() {
            r.add(
                "reason",
                "The reason field is required when is correction is true.",
            );
        }
        let meta = r.meta(m.get("meta").cloned()).flatten();
        Some(Mutate {
            warehouse_id: warehouse_id?,
            bin_id,
            mutation_value: mutation_value?,
            is_correction,
            reason,
            meta,
        })
    }
}

/// A uuid field: missing gives "required" when `required`, anything but a uuid `bad`.
pub fn uuid_of(
    r: &mut Rules,
    m: &Map<String, Value>,
    field: &str,
    bad: &str,
    required: bool,
) -> Option<Uuid> {
    match filled(m.get(field).cloned()) {
        None => {
            if required {
                r.missing(field);
            }
            None
        }
        Some(v) => {
            let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
            if id.is_none() {
                r.add(field, bad);
            }
            id
        }
    }
}

/// Optional trimmed string, at most `max` chars.
pub fn text(r: &mut Rules, m: &Map<String, Value>, field: &str, max: usize) -> Option<String> {
    match filled(m.get(field).cloned()) {
        None => None,
        Some(Value::String(s)) => r.opt_text(field, Some(s), max),
        Some(_) => {
            r.add(field, format!("The {} must be a string.", human(field)));
            None
        }
    }
}

/// Entries one set-levels/requirements call may carry.
const MAX_ENTRIES: usize = 100;

/// One `warehouses.N` entry of set-stock-levels / set-stock-requirements.
pub struct Entry {
    pub warehouse_id: Uuid,
    pub bin_id: Option<Uuid>,
    pub stock_lvl: Option<Decimal>,
    pub reason: Option<String>,
    pub minimal: Option<i32>,
    pub maximum: Option<i32>,
}

/// POST /products/{id}/set-stock-levels (Laravel SetStockLevelsRequest).
#[derive(Deserialize)]
pub struct SetLevelsBody(Map<String, Value>);

impl Validate for SetLevelsBody {
    type Valid = Vec<Entry>;

    fn validate(self, r: &mut Rules) -> Option<Vec<Entry>> {
        entries(r, &self.0, true)
    }
}

/// Validated set-stock-requirements.
pub struct SetRequirements {
    pub entries: Vec<Entry>,
    pub default_warehouse_id: Option<Uuid>,
}

/// POST /products/{id}/set-stock-requirements (Laravel SetStockRequirementRequest).
#[derive(Deserialize)]
pub struct SetRequirementsBody(Map<String, Value>);

impl Validate for SetRequirementsBody {
    type Valid = SetRequirements;

    fn validate(self, r: &mut Rules) -> Option<SetRequirements> {
        let default_warehouse_id = uuid_of(
            r,
            &self.0,
            "default_warehouse_id",
            "The default warehouse id must be a valid UUID.",
            false,
        );
        Some(SetRequirements {
            entries: entries(r, &self.0, false)?,
            default_warehouse_id,
        })
    }
}

/// The `warehouses` list. Levels: `stock_lvl` + `reason` required, min/max optional.
/// Requirements: min/max required, no level.
fn entries(r: &mut Rules, m: &Map<String, Value>, levels: bool) -> Option<Vec<Entry>> {
    let items = match m.get("warehouses") {
        None | Some(Value::Null) => {
            r.missing("warehouses");
            return None;
        }
        Some(Value::Array(items)) if items.is_empty() && levels => {
            r.add("warehouses", "The warehouses must have at least 1 items.");
            return None;
        }
        Some(Value::Array(items)) if items.len() > MAX_ENTRIES => {
            r.add(
                "warehouses",
                format!("The warehouses must not have more than {MAX_ENTRIES} items."),
            );
            return None;
        }
        Some(Value::Array(items)) => items,
        Some(_) => {
            r.add("warehouses", "The warehouses must be an array.");
            return None;
        }
    };
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let k = |f: &str| format!("warehouses.{i}.{f}");
        let empty = Map::new();
        let e = item.as_object().unwrap_or(&empty);
        let get = |f: &str| filled(e.get(f).cloned());
        let id = |r: &mut Rules, f: &str, bad: String, required: bool| match get(f) {
            None if required => {
                r.missing(&k(f));
                None
            }
            None => None,
            Some(v) => {
                let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
                if id.is_none() {
                    r.add(&k(f), bad);
                }
                id
            }
        };
        let warehouse_id = id(
            r,
            "warehouse_id",
            format!("This {} does not exist", human(&k("warehouse_id"))),
            true,
        );
        let bin_id = id(
            r,
            "bin_id",
            format!("The selected {} is invalid.", human(&k("bin_id"))),
            false,
        );
        let mut reason = None;
        let mut stock_lvl = None;
        if levels {
            stock_lvl = r
                .required_value(&k("stock_lvl"), get("stock_lvl"))
                .and_then(|v| r.decimal(&k("stock_lvl"), &v, Decimal::ZERO, money::MAX));
            reason = match get("reason") {
                Some(Value::String(s)) => r.opt_text(&k("reason"), Some(s), 255),
                Some(_) => {
                    r.add(
                        &k("reason"),
                        format!("The {} must be a string.", human(&k("reason"))),
                    );
                    None
                }
                None => None,
            };
            if reason.is_none() && get("reason").is_none() {
                r.missing(&k("reason"));
            }
        }
        let mut level = |f: &str| {
            let v = get(f);
            if v.is_none() && !levels {
                r.missing(&k(f));
            }
            v.and_then(|v| r.decimal(&k(f), &v, Decimal::ZERO, i32::MAX.into()))
        };
        let minimal = level("minimal_stock_level");
        let maximum = level("maximum_stock_level");
        if let (Some(min), Some(max)) = (minimal, maximum)
            && max < min
        {
            // Laravel `gte:` shows the other field's value as sent.
            let shown = get("minimal_stock_level").map_or_else(String::new, |v| {
                v.as_str().map_or_else(|| v.to_string(), str::to_owned)
            });
            let key = k("maximum_stock_level");
            r.add(
                &key,
                format!("The {} must be greater than or equal {shown}.", human(&key)),
            );
        }
        // MySQL rounded numeric input into the int columns.
        let int = |d: Option<Decimal>| d.and_then(|d| money::round(d, 0).to_i32());
        if let Some(warehouse_id) = warehouse_id {
            out.push(Entry {
                warehouse_id,
                bin_id,
                stock_lvl,
                reason,
                minimal: int(minimal),
                maximum: int(maximum),
            });
        }
    }
    Some(out)
}

/// Validated move-stock-between-bins.
pub struct Move {
    pub warehouse_id: Uuid,
    pub bin_from: Option<Uuid>,
    pub bin_to: Option<Uuid>,
    pub quantity: Decimal,
    pub reason: String,
}

/// POST /products/{id}/move-stock-between-bins (Laravel MoveStockBetweenBinsRequest).
#[derive(Deserialize)]
pub struct MoveBody(Map<String, Value>);

impl Validate for MoveBody {
    type Valid = Move;

    fn validate(self, r: &mut Rules) -> Option<Move> {
        let m = &self.0;
        let warehouse_id = uuid_of(r, m, "warehouse_id", NO_WAREHOUSE, true);
        let bin_from = uuid_of(
            r,
            m,
            "bin_id_from",
            "The bin id from must be a valid UUID.",
            false,
        );
        let bin_to = uuid_of(
            r,
            m,
            "bin_id_to",
            "The bin id to must be a valid UUID.",
            false,
        );
        let quantity = r
            .required_value("quantity", m.get("quantity").cloned())
            .and_then(|v| r.decimal("quantity", &v, Decimal::new(1, 2), money::MAX));
        let reason = text(r, m, "reason", 255);
        if reason.is_none() {
            r.missing("reason");
        }
        Some(Move {
            warehouse_id: warehouse_id?,
            bin_from,
            bin_to,
            quantity: quantity?,
            reason: reason?,
        })
    }
}

/// DELETE /products/{id}/stocks?warehouse_id=&bin_id= (Laravel sends DELETE data as a query).
#[derive(Deserialize)]
pub struct DeleteQuery {
    pub warehouse_id: Option<String>,
    pub bin_id: Option<String>,
}

impl DeleteQuery {
    /// `(warehouse, bin)` or the 422.
    pub fn parse(&self) -> Result<(Uuid, Option<Uuid>), crate::error::AppError> {
        let mut r = Rules::default();
        let m: Map<String, Value> = [
            ("warehouse_id", &self.warehouse_id),
            ("bin_id", &self.bin_id),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.clone().map(|v| (k.to_owned(), Value::String(v))))
        .collect();
        let warehouse = uuid_of(&mut r, &m, "warehouse_id", NO_WAREHOUSE, true);
        let bin = uuid_of(
            &mut r,
            &m,
            "bin_id",
            "The bin id must be a valid UUID.",
            false,
        );
        r.into_result()?;
        warehouse
            .map(|w| (w, bin))
            .ok_or_else(|| crate::error::AppError::field("warehouse_id", NO_WAREHOUSE))
    }
}

/// GET stock-movements query (Laravel ListProductStockMovementsRequest). Ids filter
/// independently; `per_page` defaults to 15.
#[derive(Deserialize)]
pub struct MovementQuery {
    pub warehouse_id: Option<String>,
    pub shelf_id: Option<String>,
    pub bin_id: Option<String>,
    pub page: Option<String>,
    pub per_page: Option<String>,
}

impl MovementQuery {
    /// The filter and its page. A malformed id matches nothing (nil).
    pub fn filter(
        &self,
    ) -> (
        crate::modules::stock::repo::MovementFilter,
        crate::http::list::Page,
    ) {
        let id = |v: &Option<String>| {
            v.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| Uuid::parse_str(s).unwrap_or(Uuid::nil()))
        };
        let page = crate::http::list::Page::parse(
            self.page.as_deref(),
            Some(self.per_page.as_deref().unwrap_or("15")),
        );
        let f = crate::modules::stock::repo::MovementFilter {
            warehouse_id: id(&self.warehouse_id),
            shelf_id: id(&self.shelf_id),
            bin_id: id(&self.bin_id),
            limit: page.size,
            offset: page.offset(),
        };
        (f, page)
    }
}

/// GET /products/stocks[/negative] query.
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}
