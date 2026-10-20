//! Shelf bodies: Laravel WarehouseSectionRowShelfResource and
//! Store/UpdateWarehouseSectionRowShelfRequest (vimdesk-web shelfPayloadTransformer.ts).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, present},
    modules::{warehouse_sections::dto::count, warehouse_shelves::repo::ShelfRow},
};

/// Laravel validation.unique on `shelf_code`.
pub const CODE_TAKEN: &str = "The shelf code has already been taken.";
pub const SIDES: [&str; 2] = ["left", "right"];

/// Laravel WarehouseSectionRowShelfResource. Laravel printed its parent ids as internal
/// integers; genesy prints the uuids.
#[derive(Serialize)]
pub struct ShelfResource {
    id: Uuid,
    shelf_id: Uuid,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    section_id: Uuid,
    row_id: Uuid,
    shelf_code: String,
    shelf_name: Option<String>,
    shelf_level: i32,
    side: Option<String>,
    bins_per_shelf: Option<i32>,
    total_storage_bins: Option<i32>,
    max_weight_kg: Option<i32>,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ShelfRow> for ShelfResource {
    fn from(s: ShelfRow) -> Self {
        Self {
            id: s.id,
            shelf_id: s.id,
            workspace_id: s.workspace_id,
            warehouse_id: s.warehouse_id,
            section_id: s.section_id,
            row_id: s.row_id,
            shelf_code: s.shelf_code,
            shelf_name: s.shelf_name,
            shelf_level: s.shelf_level,
            side: s.side,
            bins_per_shelf: s.bins_per_shelf,
            total_storage_bins: s.total_storage_bins,
            max_weight_kg: s.max_weight_kg,
            notes: s.notes,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

/// Create response.
#[derive(Serialize)]
pub struct ShelfCreated {
    pub shelf_id: Uuid,
}

/// Columns create writes and update patches. `None` = keep (update).
#[derive(Default)]
pub struct Fields {
    pub shelf_code: Option<String>,
    pub shelf_level: Option<i32>,
    pub shelf_name: Option<Option<String>>,
    pub side: Option<Option<String>>,
    pub bins_per_shelf: Option<Option<i32>>,
    pub total_storage_bins: Option<Option<i32>>,
    pub max_weight_kg: Option<Option<i32>>,
    pub notes: Option<Option<String>>,
}

/// POST body for create and update (path ids in the body are ignored).
#[derive(Deserialize)]
pub struct ShelfBody {
    #[serde(default, deserialize_with = "present")]
    shelf_code: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    shelf_level: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    shelf_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    side: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    bins_per_shelf: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    total_storage_bins: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    max_weight_kg: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    notes: Option<Option<String>>,
}

/// POST /shelves: code and level required.
#[derive(Deserialize)]
pub struct CreateShelfRequest(ShelfBody);

/// POST /shelves/{shelf}: absent keys keep the stored value.
#[derive(Deserialize)]
pub struct UpdateShelfRequest(ShelfBody);

impl Validate for CreateShelfRequest {
    type Valid = Fields;

    fn validate(self, r: &mut Rules) -> Option<Fields> {
        Some(fields(r, self.0, true))
    }
}

impl Validate for UpdateShelfRequest {
    type Valid = Fields;

    fn validate(self, r: &mut Rules) -> Option<Fields> {
        Some(fields(r, self.0, false))
    }
}

fn fields(r: &mut Rules, b: ShelfBody, create: bool) -> Fields {
    let shelf_code = if create {
        Some(r.text("shelf_code", b.shelf_code.flatten(), 100))
    } else {
        r.patch_text("shelf_code", b.shelf_code, 100)
    };
    let shelf_level = match (create, b.shelf_level) {
        (false, None) => None,
        (_, v) => r.required_value("shelf_level", v.flatten()).and_then(|v| {
            r.integer("shelf_level", &v, 1, i32::MAX.into())
                .and_then(|n| i32::try_from(n).ok())
        }),
    };
    Fields {
        shelf_code,
        shelf_level,
        shelf_name: b.shelf_name.map(|v| r.opt_text("shelf_name", v, 255)),
        side: b.side.map(|v| r.one_of("side", v, &SIDES, None)),
        bins_per_shelf: b.bins_per_shelf.map(|v| count(r, "bins_per_shelf", v)),
        total_storage_bins: b
            .total_storage_bins
            .map(|v| count(r, "total_storage_bins", v)),
        max_weight_kg: b.max_weight_kg.map(|v| count(r, "max_weight_kg", v)),
        notes: b.notes.map(|v| r.opt_text("notes", v, usize::MAX)),
    }
}
