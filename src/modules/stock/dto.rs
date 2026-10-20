//! Stock request bodies (Laravel MutateStockRequest, ...).

use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled},
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
            r.add(
                field,
                format!("The {} must be a string.", crate::http::json::human(field)),
            );
            None
        }
    }
}
