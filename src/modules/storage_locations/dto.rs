//! Storage location body (Laravel StoreStorageLocationRequest) and resource.

use serde::Deserialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled},
    modules::{
        stock::dto::{text, uuid_of},
        storage_locations::repo::PlaceRow,
    },
};

/// Parent domains clients send (Laravel accepted any `Domains` value). Only `product` is
/// ported; the others answer 404 until their batches land.
pub const DOMAINS: [&str; 5] = [
    "product",
    "order",
    "transfer_list",
    "work_order",
    "warehouse",
];

/// Validated POST body.
pub struct NewPlace {
    pub parent_domain: String,
    pub parent_id: String,
    pub warehouse_location_id: Option<Uuid>,
    pub bin_id: Option<Uuid>,
    pub description: Option<String>,
    pub replace: bool,
    pub meta: Option<Value>,
}

/// POST /erp/storage-locations.
#[derive(Deserialize)]
pub struct PlaceBody(Map<String, Value>);

impl Validate for PlaceBody {
    type Valid = NewPlace;

    fn validate(self, r: &mut Rules) -> Option<NewPlace> {
        let m = &self.0;
        let domain = match filled(m.get("parent_domain").cloned()) {
            None => {
                r.missing("parent_domain");
                None
            }
            Some(v) => {
                let raw = v.as_str().map_or_else(|| v.to_string(), str::to_owned);
                let lower = raw.to_lowercase();
                if DOMAINS.contains(&lower.as_str()) {
                    Some(lower)
                } else {
                    r.add(
                        "parent_domain",
                        format!("The selected parent domain: {raw} is invalid."),
                    );
                    None
                }
            }
        };
        let parent_id = match filled(m.get("parent_id").cloned()) {
            None => {
                r.missing("parent_id");
                None
            }
            Some(v) => Some(v.as_str().map_or_else(|| v.to_string(), str::to_owned)),
        };
        let location = uuid_of(
            r,
            m,
            "warehouse_location_id",
            "The warehouse location id must be a valid UUID.",
            false,
        );
        let bin = uuid_of(r, m, "bin_id", "The bin id must be a valid UUID.", false);
        let sent = |k: &str| filled(m.get(k).cloned()).is_some();
        if !sent("warehouse_location_id") && !sent("bin_id") {
            r.add(
                "warehouse_location_id",
                "The warehouse location id field is required when bin id is not present.",
            );
            r.add(
                "bin_id",
                "The bin id field is required when warehouse location id is not present.",
            );
        }
        let description = text(r, m, "description", 1500);
        let replace = filled(m.get("replace").cloned())
            .and_then(|v| r.boolean("replace", &v))
            .unwrap_or_default();
        let meta = r.meta(m.get("meta").cloned()).flatten();
        Some(NewPlace {
            parent_domain: domain?,
            parent_id: parent_id?,
            warehouse_location_id: location,
            bin_id: bin,
            description,
            replace,
            meta,
        })
    }
}

/// Laravel StorageLocationResource: the warehouse location (WarehouseLocationResource with a
/// compact warehouse), or `{"warehouse":{"name":"N/A"}}` when it is gone.
pub fn render(p: &PlaceRow) -> Value {
    let location = match p.l_id {
        Some(id) => json!({
            "id": id,
            "location": p.l_location,
            "warehouse": p.w_id.map(|wid| json!({
                "id": wid, "name": p.w_name, "slug": p.w_slug, "alias": p.w_alias,
                "is_store": p.w_is_store, "pickup_enabled": p.w_pickup,
                "parent_name": p.w_parent_name, "status": p.w_status, "type": p.w_type,
            })),
            "meta": p.l_meta,
            "created_at": p.l_created,
            "last_updated_at": p.l_updated,
        }),
        None => json!({ "warehouse": { "name": "N/A" } }),
    };
    json!({
        "id": p.id,
        "warehouse_location": location,
        "description": p.description,
        "meta": p.meta,
    })
}
