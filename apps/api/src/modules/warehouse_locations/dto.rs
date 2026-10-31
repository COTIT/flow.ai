//! Warehouse location bodies.
// Laravel WarehouseLocationResource and Store/Update/DeleteWarehouseLocationsRequest
// (vimdesk-web warehouse-locations-table.vue).

use std::borrow::Cow;

use crate::{
    error::AppError,
    http::json::{Rules, Validate, present},
    modules::{warehouse_locations::repo::LocationRow, warehouses::dto::WarehouseCompacted},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

const LOCATION_MAX: usize = 500;
/// Bulk delete cap; Laravel had none.
const DELETE_MAX: usize = 100;

// Laravel WarehouseLocationResource. A page of one warehouse borrows its compact warehouse
// instead of copying it per row.
#[derive(Serialize, ToSchema)]
#[schema(description = "A free-text scan label inside a warehouse.")]
pub struct LocationResource<'a> {
    id: Uuid,
    location: String,
    #[schema(value_type = WarehouseCompacted)]
    warehouse: Cow<'a, WarehouseCompacted>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
}

impl<'a> LocationResource<'a> {
    /// `warehouse` from `WarehouseCompacted::from(&CompactRow)`.
    pub fn new(l: LocationRow, warehouse: Cow<'a, WarehouseCompacted>) -> Self {
        Self {
            id: l.id,
            location: l.location,
            warehouse,
            meta: l.meta,
            created_at: l.created_at,
            last_updated_at: l.updated_at,
        }
    }
}

/// Create response.
#[derive(Serialize, ToSchema)]
pub struct LocationCreated {
    pub warehouse_location_id: Uuid,
}

/// GET list query. Unknown params ignored.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search on the label.
    pub q: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Page size: default 50, max 5000.
    #[param(value_type = Option<i64>)]
    pub limit: Option<String>,
}

/// DELETE bulk query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeleteQuery {
    /// Comma-separated location ids, at most 100.
    #[param(required = true)]
    pub locations: Option<String>,
}

impl DeleteQuery {
    /// Laravel `explode(',')` then `locations.*` required|uuid; at most 100.
    pub fn ids(&self) -> Result<Vec<Uuid>, AppError> {
        let raw = self.locations.as_deref().unwrap_or_default();
        let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
        if parts.len() > DELETE_MAX {
            return Err(AppError::field(
                "locations",
                format!("The locations must not have more than {DELETE_MAX} items."),
            ));
        }
        let mut r = Rules::default();
        let mut ids = Vec::with_capacity(parts.len());
        for (i, p) in parts.iter().enumerate() {
            let key = format!("locations.{i}");
            if p.is_empty() {
                r.add(&key, format!("The {key} field is required."));
            } else if let Ok(id) = Uuid::parse_str(p) {
                ids.push(id);
            } else {
                r.add(&key, format!("The {key} must be a valid UUID."));
            }
        }
        r.into_result().map(|()| ids)
    }
}

/// POST body (create and update): `location` required, `meta` optional.
#[derive(Deserialize, ToSchema)]
#[schema(as = WarehouseLocationRequest)]
pub struct LocationBody {
    #[schema(value_type = String, max_length = 500)]
    location: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
}

/// Validated body. `meta`: None = not sent (update keeps it).
pub struct LocationInput {
    pub location: String,
    pub meta: Option<Option<Value>>,
}

impl Validate for LocationBody {
    type Valid = LocationInput;

    fn validate(self, r: &mut Rules) -> Option<LocationInput> {
        Some(LocationInput {
            location: r.text("location", self.location, LOCATION_MAX),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}
