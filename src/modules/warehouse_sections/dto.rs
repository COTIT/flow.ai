//! Section bodies: Laravel SectionResource and Store/Update/ListSectionRequest
//! (vimdesk-web sectionPayloadTransformer.ts).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::warehouse_sections::repo::SectionRow,
};

pub const TEMPERATURES: [&str; 4] = ["ambient", "cold", "freezer", "heated"];
pub const PRIORITIES: [&str; 4] = ["low", "standard", "high", "critical"];
const BAD_TEMPERATURE: &str = "Temperature control must be one of: ambient, cold, freezer, heated.";
const BAD_PRIORITY: &str = "Priority level must be one of: low, standard, high, critical.";
/// Laravel's custom `section_code.unique` text.
pub const CODE_TAKEN: &str = "The section code must be unique within this warehouse.";

/// Laravel SectionResource.
#[derive(Serialize)]
pub struct SectionResource {
    id: Uuid,
    section_id: Uuid,
    warehouse_id: Uuid,
    section_name: String,
    section_code: String,
    category: Option<String>,
    description: Option<String>,
    rows: Option<i32>,
    shelves_per_row: Option<i32>,
    bins_per_shelf: Option<i32>,
    total_bins: Option<i32>,
    layout: Layout,
    zone: Option<String>,
    temperature_control: String,
    restricted_access: bool,
    priority_level: String,
    assigned_manager: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    relationships: Relationships,
}

#[derive(Serialize)]
struct Layout {
    rows: Option<i32>,
    shelves_per_row: Option<i32>,
    bins_per_shelf: Option<i32>,
    total_rows: i64,
    total_shelves: i64,
    total_bins: i64,
    total_capacity: i64,
    location_format: String,
}

/// `{id, name}` of a parent warehouse, as the hierarchy resources embed it.
#[derive(Serialize)]
pub struct WarehouseRef {
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize)]
struct Relationships {
    warehouse: WarehouseRef,
    assigned_manager: Option<Uuid>,
}

impl From<SectionRow> for SectionResource {
    fn from(s: SectionRow) -> Self {
        let n = |v: Option<i32>| i64::from(v.unwrap_or_default());
        let capacity = n(s.rows) * n(s.shelves_per_row) * n(s.bins_per_shelf);
        Self {
            id: s.id,
            section_id: s.id,
            warehouse_id: s.warehouse_id,
            layout: Layout {
                rows: s.rows,
                shelves_per_row: s.shelves_per_row,
                bins_per_shelf: s.bins_per_shelf,
                total_rows: n(s.rows),
                total_shelves: n(s.rows) * n(s.shelves_per_row),
                total_bins: s.total_bins.map_or(capacity, i64::from),
                total_capacity: capacity,
                location_format: format!(
                    "{}-{}",
                    s.section_code,
                    s.zone.as_deref().unwrap_or("GENERAL")
                ),
            },
            section_name: s.section_name,
            section_code: s.section_code,
            category: s.category,
            description: s.description,
            rows: s.rows,
            shelves_per_row: s.shelves_per_row,
            bins_per_shelf: s.bins_per_shelf,
            total_bins: s.total_bins,
            zone: s.zone,
            temperature_control: s.temperature_control,
            restricted_access: s.restricted_access,
            priority_level: s.priority_level,
            assigned_manager: s.assigned_manager,
            created_at: s.created_at,
            updated_at: s.updated_at,
            relationships: Relationships {
                warehouse: WarehouseRef {
                    id: s.warehouse_id,
                    name: s.warehouse_name,
                },
                assigned_manager: s.assigned_manager,
            },
        }
    }
}

/// Create response.
#[derive(Serialize)]
pub struct SectionCreated {
    pub section_id: Uuid,
}

/// GET list query (Laravel ListSectionRequest). Unknown params ignored.
#[derive(Deserialize)]
pub struct ListQuery {
    pub page: Option<String>,
    pub per_page: Option<String>,
    pub category: Option<String>,
    pub zone: Option<String>,
    pub priority_level: Option<String>,
    pub restricted_access: Option<String>,
    pub search: Option<String>,
}

/// Columns create writes and update patches. `None` = keep (update) / default (create).
#[derive(Default)]
pub struct Fields {
    pub category: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub rows: Option<Option<i32>>,
    pub shelves_per_row: Option<Option<i32>>,
    pub bins_per_shelf: Option<Option<i32>>,
    pub total_bins: Option<Option<i32>>,
    pub zone: Option<Option<String>>,
    pub temperature_control: Option<String>,
    pub restricted_access: Option<bool>,
    pub assigned_manager: Option<Option<Uuid>>,
    pub priority_level: Option<String>,
}

/// Validated body. Laravel create and update both require name and code.
pub struct SectionInput {
    pub section_name: String,
    pub section_code: String,
    pub fields: Fields,
}

/// POST body for create and update (`warehouse_id`/`workspace_id` keys are ignored).
#[derive(Deserialize)]
pub struct SectionBody {
    section_name: Option<String>,
    section_code: Option<String>,
    #[serde(default, deserialize_with = "present")]
    category: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    description: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    rows: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    shelves_per_row: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    bins_per_shelf: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    total_bins: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    zone: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    temperature_control: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    restricted_access: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    assigned_manager: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    priority_level: Option<Option<String>>,
}

impl Validate for SectionBody {
    type Valid = SectionInput;

    fn validate(self, r: &mut Rules) -> Option<SectionInput> {
        Some(SectionInput {
            section_name: r.text("section_name", self.section_name, 255),
            section_code: r.text("section_code", self.section_code, 50),
            fields: Fields {
                category: self.category.map(|v| r.opt_text("category", v, 255)),
                description: self
                    .description
                    .map(|v| r.opt_text("description", v, usize::MAX)),
                rows: self.rows.map(|v| count(r, "rows", v)),
                shelves_per_row: self.shelves_per_row.map(|v| count(r, "shelves_per_row", v)),
                bins_per_shelf: self.bins_per_shelf.map(|v| count(r, "bins_per_shelf", v)),
                total_bins: self.total_bins.map(|v| count(r, "total_bins", v)),
                zone: self.zone.map(|v| r.opt_text("zone", v, 255)),
                // null = Laravel's validated() default.
                temperature_control: self.temperature_control.map(|v| {
                    r.one_of(
                        "temperature_control",
                        v,
                        &TEMPERATURES,
                        Some(BAD_TEMPERATURE),
                    )
                    .unwrap_or_else(|| "ambient".into())
                }),
                restricted_access: self.restricted_access.map(|v| {
                    filled(v)
                        .and_then(|v| r.boolean("restricted_access", &v))
                        .unwrap_or_default()
                }),
                assigned_manager: self.assigned_manager.map(|v| manager(r, v)),
                priority_level: self.priority_level.map(|v| {
                    r.one_of("priority_level", v, &PRIORITIES, Some(BAD_PRIORITY))
                        .unwrap_or_else(|| "standard".into())
                }),
            },
        })
    }
}

/// `nullable|integer|min:0`.
pub fn count(r: &mut Rules, key: &str, v: Option<Value>) -> Option<i32> {
    let v = filled(v)?;
    r.integer(key, &v, 0, i32::MAX.into())
        .and_then(|n| i32::try_from(n).ok())
}

/// A workspace member's uuid; membership is checked by the service. Laravel took an integer
/// user id here, which genesy doesn't have.
fn manager(r: &mut Rules, v: Option<Value>) -> Option<Uuid> {
    let v = filled(v)?;
    let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
    if id.is_none() {
        r.add(
            "assigned_manager",
            "The assigned manager must be a valid UUID.",
        );
    }
    id
}
