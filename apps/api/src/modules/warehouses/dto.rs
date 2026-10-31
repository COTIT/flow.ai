//! Warehouse request/response bodies.
// Keys follow Laravel WarehouseResource, WarehouseCompactedResource and
// Store/Update/ListWarehouseRequest (vimdesk-web warehousePayloadTransformer.js).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::modules::stock::resource::utilization;
use crate::{
    http::json::{Rules, Validate, filled, human, present},
    modules::{
        relations::address::{self, Address},
        users::{dto::UserEmailResource, repo::UserRef},
        warehouses::repo::{ChildRow, CompactRow, WarehouseRow},
    },
    money,
};

const NAME_MAX: usize = 150;
const ALIAS_MAX: usize = 55;
const STATUSES: [&str; 4] = ["active", "construction", "inactive", "maintenance"];
const TYPES: [&str; 6] = [
    "cross-dock",
    "distribution",
    "fulfillment",
    "processing",
    "storage",
    "store",
];
/// Laravel `rules.ValidateId` for domain `warehouse`.
pub const BAD_PARENT: &str = "The parent id is an invalid id for the warehouse domain.";

// Laravel WarehouseResource.
#[derive(Serialize, ToSchema)]
#[schema(description = "A warehouse, with its child warehouses and responsible people.")]
pub struct WarehouseResource {
    id: Uuid,
    name: String,
    #[schema(required = true)]
    description: Option<String>,
    slug: String,
    #[schema(required = true)]
    alias: Option<String>,
    #[schema(required = true)]
    location: Option<LocationView>,
    /// One-line address; left out when there is no location.
    // Laravel `$this->when`.
    #[serde(skip_serializing_if = "Option::is_none")]
    formatted_location: Option<String>,
    default_warehouse: bool,
    stock_count_enabled: bool,
    #[schema(required = true)]
    is_store: Option<bool>,
    pickup_enabled: bool,
    auto_generate_transfer_list: bool,
    total_sum_enabled: bool,
    #[schema(required = true)]
    manager_id: Option<Uuid>,
    #[schema(required = true)]
    manager_assistant_id: Option<Uuid>,
    #[schema(required = true)]
    status: Option<String>,
    #[serde(rename = "type")]
    #[schema(required = true)]
    kind: Option<String>,
    /// `total_area` and `storage_height`.
    #[schema(value_type = Option<Object>, required = true)]
    dimensions: Option<Value>,
    #[schema(required = true)]
    loading_docks: Option<i32>,
    #[schema(required = true)]
    sections: Option<i32>,
    #[schema(required = true)]
    rows_per_sections: Option<i32>,
    #[schema(required = true)]
    shelves_per_row: Option<i32>,
    #[schema(required = true)]
    bins_per_shelf: Option<i32>,
    /// Occupied bins of the grid, in %; null without a grid.
    #[schema(value_type = Option<f64>, required = true)]
    utilization: Option<Value>,
    #[schema(value_type = Option<Object>, required = true)]
    management_contact: Option<Value>,
    #[schema(value_type = Option<Object>, required = true)]
    operating_hours: Option<Value>,
    #[schema(value_type = Option<Object>, required = true)]
    additional_settings: Option<Value>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(inline)]
    relationships: Relationships,
}

#[derive(Serialize, ToSchema)]
#[schema(as = WarehouseRelationships)]
struct Relationships {
    /// Always empty.
    // Custom fields aren't ported.
    #[schema(value_type = Vec<Object>, max_items = 0)]
    custom_sections: [(); 0],
    children: Vec<WarehouseCompacted>,
    #[schema(required = true)]
    user: Option<UserEmailResource>,
    #[schema(required = true)]
    manager: Option<UserEmailResource>,
    #[schema(required = true)]
    manager_assistant: Option<UserEmailResource>,
}

// Laravel WarehouseCompactedResource (children of a warehouse).
#[derive(Serialize, Clone, ToSchema)]
#[schema(description = "Short form of a warehouse.")]
pub struct WarehouseCompacted {
    id: Uuid,
    name: String,
    slug: String,
    #[schema(required = true)]
    alias: Option<String>,
    #[schema(required = true)]
    is_store: Option<bool>,
    pickup_enabled: bool,
    #[schema(required = true)]
    parent_name: Option<String>,
    #[schema(required = true)]
    status: Option<String>,
    #[serde(rename = "type")]
    #[schema(required = true)]
    kind: Option<String>,
}

impl From<&CompactRow> for WarehouseCompacted {
    fn from(w: &CompactRow) -> Self {
        Self {
            id: w.id,
            name: w.name.clone(),
            slug: w.slug.clone(),
            alias: w.alias.clone(),
            is_store: w.is_store,
            pickup_enabled: w.pickup_enabled,
            parent_name: w.parent_name.clone(),
            status: w.status.clone(),
            kind: w.kind.clone(),
        }
    }
}

// Laravel Address::toSingleArray: zipcode uppercased, `""` when missing.
#[derive(Serialize, ToSchema)]
#[schema(description = "Warehouse address. `zipcode` is upper case, `\"\"` when missing.")]
struct LocationView {
    #[schema(required = true)]
    address: Option<String>,
    #[schema(required = true)]
    house_number: Option<String>,
    #[schema(required = true)]
    house_number_addition: Option<String>,
    #[schema(required = true)]
    address2: Option<String>,
    zipcode: String,
    #[schema(required = true)]
    city: Option<String>,
    #[schema(required = true)]
    country: Option<String>,
    #[schema(required = true)]
    state: Option<String>,
    #[serde(rename = "type")]
    #[schema(required = true)]
    kind: Option<String>,
    #[schema(required = true)]
    attn: Option<String>,
    #[schema(required = true)]
    phone_number: Option<String>,
}

impl From<&Address> for LocationView {
    fn from(a: &Address) -> Self {
        Self {
            address: a.address.clone(),
            house_number: a.house_number.clone(),
            house_number_addition: a.house_number_addition.clone(),
            address2: a.address2.clone(),
            zipcode: a.zipcode.as_deref().unwrap_or_default().to_uppercase(),
            city: a.city.clone(),
            country: a.country.clone(),
            state: a.state.clone(),
            kind: a.kind.clone(),
            attn: a.attn.clone(),
            phone_number: a.phone_number.clone(),
        }
    }
}

impl WarehouseResource {
    /// Rows of one page or a single GET: children grouped by parent, people by id.
    pub fn build(
        rows: Vec<WarehouseRow>,
        children: Vec<ChildRow>,
        people: &[UserRef],
        occupied: &HashMap<Uuid, i64>,
    ) -> Vec<Self> {
        let people: HashMap<Uuid, &UserRef> = people.iter().map(|u| (u.id, u)).collect();
        let person = |id: Option<Uuid>| {
            id.and_then(|id| people.get(&id))
                .map(|u| UserEmailResource::from(*u))
        };
        let mut by_parent: HashMap<Uuid, Vec<ChildRow>> = HashMap::new();
        for c in children {
            by_parent.entry(c.parent_id).or_default().push(c);
        }
        rows.into_iter()
            .map(|w| {
                let location: Option<Address> =
                    w.location.and_then(|v| serde_json::from_value(v).ok());
                let children = by_parent
                    .remove(&w.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| WarehouseCompacted {
                        id: c.id,
                        name: c.name,
                        slug: c.slug,
                        alias: c.alias,
                        is_store: c.is_store,
                        pickup_enabled: c.pickup_enabled,
                        parent_name: Some(w.name.clone()),
                        status: c.status,
                        kind: c.kind,
                    })
                    .collect();
                Self {
                    id: w.id,
                    description: w.description,
                    slug: w.slug,
                    alias: w.alias,
                    formatted_location: location.as_ref().map(Address::formatted),
                    location: location.as_ref().map(LocationView::from),
                    default_warehouse: w.default_warehouse,
                    stock_count_enabled: w.stock_count_enabled,
                    is_store: w.is_store,
                    pickup_enabled: w.pickup_enabled,
                    auto_generate_transfer_list: w.auto_generate_transfer_list,
                    total_sum_enabled: w.total_sum_enabled,
                    manager_id: w.manager_id,
                    manager_assistant_id: w.manager_assistant_id,
                    status: w.status,
                    kind: w.kind,
                    dimensions: w.dimensions,
                    loading_docks: w.loading_docks,
                    sections: w.sections,
                    rows_per_sections: w.rows_per_sections,
                    shelves_per_row: w.shelves_per_row,
                    bins_per_shelf: w.bins_per_shelf,
                    utilization: utilization(
                        occupied.get(&w.id).copied().unwrap_or_default(),
                        [
                            w.sections,
                            w.rows_per_sections,
                            w.shelves_per_row,
                            w.bins_per_shelf,
                        ]
                        .iter()
                        // Unchecked, four large i32 columns overflow i64.
                        .try_fold(1_i64, |acc, n| {
                            acc.checked_mul(n.unwrap_or_default().into())
                        })
                        .unwrap_or(i64::MAX),
                    ),
                    management_contact: w.management_contact,
                    operating_hours: w.operating_hours,
                    additional_settings: w.additional_settings,
                    meta: w.meta,
                    created_at: w.created_at,
                    last_updated_at: w.updated_at,
                    relationships: Relationships {
                        custom_sections: [],
                        children,
                        user: person(w.user_id),
                        manager: person(w.manager_id),
                        manager_assistant: person(w.manager_assistant_id),
                    },
                    name: w.name,
                }
            })
            .collect()
    }
}

/// Create response.
#[derive(Serialize, ToSchema)]
pub struct WarehouseCreated {
    pub warehouse_id: Uuid,
}

/// GET list query. Unknown params ignored.
// Laravel ListWarehouseRequest.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// Only children of this warehouse.
    #[param(value_type = Option<Uuid>)]
    pub parent_id: Option<String>,
    /// `1` = only warehouses with pickup enabled.
    #[param(value_type = Option<bool>)]
    pub pickup_enabled: Option<String>,
    /// Default on: only top-level warehouses. `0` lists every level.
    #[param(value_type = Option<bool>)]
    pub roots_only: Option<String>,
}

/// Columns both create and update write. Update leaves `None` fields as stored.
#[derive(Default)]
pub struct Fields {
    pub alias: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub stock_count_enabled: Option<bool>,
    pub is_store: Option<Option<bool>>,
    pub pickup_enabled: Option<bool>,
    pub auto_generate_transfer_list: Option<bool>,
    pub total_sum_enabled: Option<bool>,
    pub user_id: Option<Option<Uuid>>,
    pub manager_id: Option<Option<Uuid>>,
    pub manager_assistant_id: Option<Option<Uuid>>,
    pub location: Option<Option<Value>>,
    pub status: Option<Option<String>>,
    pub kind: Option<Option<String>>,
    pub dimensions: Option<Option<Value>>,
    pub loading_docks: Option<Option<i32>>,
    pub sections: Option<Option<i32>>,
    pub rows_per_sections: Option<Option<i32>>,
    pub shelves_per_row: Option<Option<i32>>,
    pub bins_per_shelf: Option<Option<i32>>,
    pub management_contact: Option<Option<Value>>,
    pub operating_hours: Option<Option<Value>>,
    pub additional_settings: Option<Option<Value>>,
    pub meta: Option<Option<Value>>,
}

impl Fields {
    /// Ids that must belong to members of the workspace, with their field names.
    pub fn people(&self) -> Vec<(&'static str, Uuid)> {
        [
            ("user_id", self.user_id),
            ("manager_id", self.manager_id),
            ("manager_assistant_id", self.manager_assistant_id),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.flatten().map(|id| (k, id)))
        .collect()
    }
}

/// Validated create input.
pub struct NewWarehouse {
    pub name: String,
    pub parent_id: Option<Uuid>,
    /// None = Laravel's "not set": becomes default when the workspace has none.
    pub default_warehouse: Option<bool>,
    pub fields: Fields,
}

/// Validated update: `name` always set (Laravel update rules require it), the rest patches.
pub struct WarehousePatch {
    pub name: String,
    pub default_warehouse: Option<bool>,
    pub fields: Fields,
}

/// Both request bodies. Every key optional so the validators decide what is required.
#[derive(Deserialize, ToSchema)]
#[schema(
    as = WarehouseRequest,
    description = "Create or update a warehouse. Update keeps fields left out; null clears them."
)]
pub struct WarehouseBody {
    #[schema(value_type = String, max_length = 150)]
    name: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    parent_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 55)]
    alias: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    description: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    default_warehouse: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    stock_count_enabled: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    is_store: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    pickup_enabled: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    auto_generate_transfer_list: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    total_sum_enabled: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    user_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    manager_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    manager_assistant_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    location: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, example = "active")]
    status: Option<Option<String>>,
    #[serde(default, rename = "type", deserialize_with = "present")]
    #[schema(value_type = Option<String>, example = "storage")]
    kind: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    dimensions: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    loading_docks: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    sections: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    rows_per_sections: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    shelves_per_row: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    bins_per_shelf: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    management_contact: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    operating_hours: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    additional_settings: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
    /// Custom fields are not supported: an item naming a `section_id` is rejected.
    #[schema(value_type = Option<Vec<Object>>)]
    custom_sections: Option<Value>,
    /// Same rule as `custom_sections`.
    #[schema(value_type = Option<Vec<Object>>)]
    custom_section_forms: Option<Value>,
}

/// POST /warehouses.
#[derive(Deserialize)]
pub struct CreateWarehouseRequest(WarehouseBody);

/// POST /warehouses/{id}.
#[derive(Deserialize)]
pub struct UpdateWarehouseRequest(WarehouseBody);

impl Validate for CreateWarehouseRequest {
    type Valid = NewWarehouse;

    fn validate(self, r: &mut Rules) -> Option<NewWarehouse> {
        let b = self.0;
        let name = r.text("name", b.name.clone(), NAME_MAX);
        let parent_id = b.parent_id.clone().flatten().and_then(|v| parent(r, v));
        let default_warehouse = flag(r, "default_warehouse", b.default_warehouse.clone());
        // Laravel's create rules also check these nested keys; update only checks `array`.
        check_nested(r, &b);
        let fields = fields(r, b, false);
        Some(NewWarehouse {
            name,
            parent_id,
            default_warehouse,
            fields,
        })
    }
}

impl Validate for UpdateWarehouseRequest {
    type Valid = WarehousePatch;

    fn validate(self, r: &mut Rules) -> Option<WarehousePatch> {
        let b = self.0;
        let name = r.text("name", b.name.clone(), NAME_MAX);
        let default_warehouse = flag(r, "default_warehouse", b.default_warehouse.clone());
        Some(WarehousePatch {
            name,
            default_warehouse,
            fields: fields(r, b, true),
        })
    }
}

/// Shared field rules. `patch`: absent keys stay None (keep); on create they are written
/// with the column default.
fn fields(r: &mut Rules, b: WarehouseBody, patch: bool) -> Fields {
    r.custom_sections("custom_sections", b.custom_sections.as_ref());
    r.custom_sections("custom_section_forms", b.custom_section_forms.as_ref());
    let opt = |v: Option<Option<Value>>| if patch { v } else { Some(v.flatten()) };
    let opt_s = |v: Option<Option<String>>| if patch { v } else { Some(v.flatten()) };
    Fields {
        alias: opt_s(b.alias).map(|v| r.opt_text("alias", v, ALIAS_MAX)),
        description: opt_s(b.description).map(|v| r.opt_text("description", v, usize::MAX)),
        stock_count_enabled: not_null_flag(r, "stock_count_enabled", opt(b.stock_count_enabled)),
        is_store: opt(b.is_store).map(|v| filled(v).and_then(|v| r.boolean("is_store", &v))),
        pickup_enabled: not_null_flag(r, "pickup_enabled", opt(b.pickup_enabled)),
        auto_generate_transfer_list: not_null_flag(
            r,
            "auto_generate_transfer_list",
            opt(b.auto_generate_transfer_list),
        ),
        total_sum_enabled: not_null_flag(r, "total_sum_enabled", opt(b.total_sum_enabled)),
        user_id: opt(b.user_id).map(|v| person(r, "user_id", v)),
        manager_id: opt(b.manager_id).map(|v| person(r, "manager_id", v)),
        manager_assistant_id: opt(b.manager_assistant_id)
            .map(|v| person(r, "manager_assistant_id", v)),
        location: opt(b.location).map(|v| {
            filled(v)
                .and_then(|v| address::validate_one(r, "location", &v))
                .and_then(|a| serde_json::to_value(a).ok())
        }),
        status: opt_s(b.status).map(|v| r.one_of("status", v, &STATUSES, None)),
        kind: opt_s(b.kind).map(|v| r.one_of("type", v, &TYPES, None)),
        dimensions: opt(b.dimensions).map(|v| filled(v).and_then(|v| dimensions(r, &v))),
        loading_docks: opt(b.loading_docks).map(|v| int(r, "loading_docks", v)),
        sections: opt(b.sections).map(|v| int(r, "sections", v)),
        rows_per_sections: opt(b.rows_per_sections).map(|v| int(r, "rows_per_sections", v)),
        shelves_per_row: opt(b.shelves_per_row).map(|v| int(r, "shelves_per_row", v)),
        bins_per_shelf: opt(b.bins_per_shelf).map(|v| int(r, "bins_per_shelf", v)),
        management_contact: opt(b.management_contact).map(|v| object(r, "management_contact", v)),
        operating_hours: opt(b.operating_hours).map(|v| object(r, "operating_hours", v)),
        additional_settings: opt(b.additional_settings)
            .map(|v| object(r, "additional_settings", v)),
        meta: opt(b.meta).and_then(|v| r.meta(v)),
    }
}

/// Laravel's nested create rules on management_contact / operating_hours.
fn check_nested(r: &mut Rules, b: &WarehouseBody) {
    if let Some(Some(Value::Object(m))) = &b.management_contact {
        for key in ["primary_phone", "emergency_contact", "email"] {
            nested_text(r, &format!("management_contact.{key}"), m.get(key), 255);
        }
    }
    if let Some(Some(Value::Object(m))) = &b.operating_hours {
        for key in ["is24x7", "weekend_ops"] {
            if let Some(v) = filled(m.get(key).cloned()) {
                r.boolean(&format!("operating_hours.{key}"), &v);
            }
        }
        for key in ["opening_time", "closing_time"] {
            nested_text(r, &format!("operating_hours.{key}"), m.get(key), 255);
        }
    }
}

fn nested_text(r: &mut Rules, key: &str, v: Option<&Value>, max: usize) {
    match v {
        None | Some(Value::Null) => {}
        Some(Value::String(s)) => r.max_chars(key, s, max),
        Some(_) => r.add(key, format!("The {} must be a string.", human(key))),
    }
}

/// `nullable|boolean` with null treated as not sent. None = absent or null.
fn flag(r: &mut Rules, key: &str, v: Option<Option<Value>>) -> Option<bool> {
    filled(v.flatten()).and_then(|v| r.boolean(key, &v))
}

/// `nullable|boolean` on a NOT NULL column: null = false (Laravel stored 0).
fn not_null_flag(r: &mut Rules, key: &str, v: Option<Option<Value>>) -> Option<bool> {
    v.map(|v| {
        filled(v)
            .and_then(|v| r.boolean(key, &v))
            .unwrap_or_default()
    })
}

/// `nullable|uuid|exists:users,uuid`: membership is checked by the service.
fn person(r: &mut Rules, key: &str, v: Option<Value>) -> Option<Uuid> {
    let v = filled(v)?;
    let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
    if id.is_none() {
        let f = human(key);
        r.add(key, format!("The {f} must be a valid UUID."));
        r.add(key, format!("The selected {f} is invalid."));
    }
    id
}

/// `nullable|uuid|ValidateId('warehouse')`: a malformed id fails both rules.
fn parent(r: &mut Rules, v: Value) -> Option<Uuid> {
    let v = filled(Some(v))?;
    let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
    if id.is_none() {
        r.add("parent_id", "The parent id must be a valid UUID.");
        r.add("parent_id", BAD_PARENT);
    }
    id
}

fn int(r: &mut Rules, key: &str, v: Option<Value>) -> Option<i32> {
    let v = filled(v)?;
    r.integer(key, &v, i32::MIN.into(), i32::MAX.into())
        .and_then(|n| i32::try_from(n).ok())
}

/// `nullable|array`: an object, stored as sent.
fn object(r: &mut Rules, key: &str, v: Option<Value>) -> Option<Value> {
    match v? {
        Value::Null => None,
        o @ Value::Object(_) => Some(o),
        _ => {
            r.add(key, format!("The {} must be an array.", human(key)));
            None
        }
    }
}

/// Laravel WarehouseDimensions cast: only `total_area` and `storage_height` survive, as
/// JSON numbers. `total_height` is validated, then dropped (Laravel parity).
fn dimensions(r: &mut Rules, v: &Value) -> Option<Value> {
    let Value::Object(m) = v else {
        r.add("dimensions", "The dimensions must be an array.");
        return None;
    };
    let mut out = Map::new();
    for key in ["total_area", "total_height", "storage_height"] {
        let n = filled(m.get(key).cloned())
            .and_then(|n| r.decimal(&format!("dimensions.{key}"), &n, Decimal::ZERO, money::MAX));
        if key != "total_height" {
            out.insert(key.into(), json!(n.and_then(|n| n.to_f64())));
        }
    }
    Some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_keep_area_and_height_as_numbers() {
        let mut r = Rules::default();
        let v = dimensions(
            &mut r,
            &json!({ "total_area": "120.5", "storage_height": 8, "total_height": 3, "x": 1 }),
        );
        assert!(r.into_result().is_ok());
        assert_eq!(
            v,
            Some(json!({ "total_area": 120.5, "storage_height": 8.0 }))
        );
    }

    #[test]
    fn people_lists_only_sent_ids() {
        let id = Uuid::nil();
        let f = Fields {
            manager_id: Some(Some(id)),
            user_id: Some(None),
            ..Fields::default()
        };
        assert_eq!(f.people(), vec![("manager_id", id)]);
    }
}
