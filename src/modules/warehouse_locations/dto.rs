//! Warehouse location bodies: Laravel WarehouseLocationResource and
//! Store/Update/DeleteWarehouseLocationsRequest (vimdesk-web warehouse-locations-table.vue).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, Validate, present},
    modules::{
        warehouse_locations::repo::LocationRow,
        warehouses::{dto::WarehouseCompacted, repo::CompactRow},
    },
};

const LOCATION_MAX: usize = 500;
/// Bulk delete cap; Laravel had none.
const DELETE_MAX: usize = 100;

/// Laravel WarehouseLocationResource.
#[derive(Serialize)]
pub struct LocationResource {
    id: Uuid,
    location: String,
    warehouse: WarehouseCompacted,
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
}

impl LocationResource {
    /// `warehouse` is the same for every row of one request.
    pub fn new(l: LocationRow, warehouse: &CompactRow) -> Self {
        Self {
            id: l.id,
            location: l.location,
            warehouse: WarehouseCompacted::from(warehouse),
            meta: l.meta,
            created_at: l.created_at,
            last_updated_at: l.updated_at,
        }
    }
}

/// Create response.
#[derive(Serialize)]
pub struct LocationCreated {
    pub warehouse_location_id: Uuid,
}

/// GET list query. `limit` is Laravel's page size (default 50); unknown params ignored.
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub page: Option<String>,
    pub limit: Option<String>,
}

/// DELETE bulk query: `locations=uuid1,uuid2`.
#[derive(Deserialize)]
pub struct DeleteQuery {
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
#[derive(Deserialize)]
pub struct LocationBody {
    location: Option<String>,
    #[serde(default, deserialize_with = "present")]
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
