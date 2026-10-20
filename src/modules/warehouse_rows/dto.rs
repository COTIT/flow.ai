//! Row bodies: Laravel WarehouseSectionRowResource and Store/UpdateWarehouseSectionRowRequest
//! (vimdesk-web rowPayloadTransformer.ts).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::{
        warehouse_rows::repo::RowRow,
        warehouse_sections::{
            dto::{PRIORITIES, TEMPERATURES, WarehouseRef, count},
            repo::SectionRow,
        },
    },
};

/// Laravel validation.unique on `row_code`.
pub const CODE_TAKEN: &str = "The row code has already been taken.";

/// Laravel WarehouseSectionRowResource.
#[derive(Serialize)]
pub struct RowResource {
    id: Uuid,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    section_id: Uuid,
    row_code: String,
    row_name: String,
    length_meters: f64,
    width_meters: f64,
    height_meters: f64,
    basic_information: Value,
    physical_dimensions: Value,
    shelf_configuration: Value,
    bins_per_shelf: i32,
    total_bins: i64,
    // shortcut: 0.0 until stock (6F) can count occupied bins.
    utilization: f64,
    location_access: Value,
    additional_settings: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    relationships: Relationships,
}

#[derive(Serialize)]
struct Relationships {
    warehouse: WarehouseRef,
    /// Laravel printed a column rows don't have.
    assigned_manager: Option<Uuid>,
}

impl RowResource {
    /// `section` supplies the warehouse name and `bins_per_shelf`.
    pub fn new(r: RowRow, workspace_id: Uuid, section: &SectionRow) -> Self {
        let bins_per_shelf = section.bins_per_shelf.unwrap_or_default();
        Self {
            id: r.id,
            workspace_id,
            warehouse_id: section.warehouse_id,
            section_id: section.id,
            basic_information: serde_json::json!({
                "orientation": r.orientation, "row_type": r.row_type,
            }),
            physical_dimensions: serde_json::json!({
                "length_meters": r.length_meters, "width_meters": r.width_meters,
                "height_meters": r.height_meters,
                "calculated_volume_cubic_meters": r.calculated_volume_cubic_meters,
            }),
            shelf_configuration: serde_json::json!({
                "number_of_shelves": r.number_of_shelves, "shelf_spacing_cm": r.shelf_spacing_cm,
            }),
            bins_per_shelf,
            total_bins: i64::from(r.number_of_shelves.unwrap_or_default())
                * i64::from(bins_per_shelf),
            utilization: 0.0,
            location_access: serde_json::json!({
                "starting_position": { "x": r.position_x, "y": r.position_y },
                "access_side": r.access_side, "aisle_width_cm": r.aisle_width_cm,
                "equipment_access": r.equipment_access,
            }),
            additional_settings: serde_json::json!({
                "priority_level": r.priority_level, "temperature_zone": r.temperature_zone,
                "notes": r.notes,
                "enable_automatic_bin_labeling": r.enable_automatic_bin_labeling,
                "generate_qr_codes_for_shelves": r.generate_qr_codes_for_shelves,
            }),
            row_code: r.row_code,
            row_name: r.row_name,
            length_meters: r.length_meters,
            width_meters: r.width_meters,
            height_meters: r.height_meters,
            created_at: r.created_at,
            updated_at: r.updated_at,
            relationships: Relationships {
                warehouse: WarehouseRef {
                    id: section.warehouse_id,
                    name: section.warehouse_name.clone(),
                },
                assigned_manager: None,
            },
        }
    }
}

/// Create response: Laravel sent the same id under both keys.
#[derive(Serialize)]
pub struct RowCreated {
    pub row_id: Uuid,
    pub warehouse_section_row_id: Uuid,
}

/// Columns create writes and update patches. `None` = keep (update) / default (create).
#[derive(Default)]
pub struct Fields {
    pub row_code: Option<String>,
    pub row_name: Option<String>,
    pub length_meters: Option<f64>,
    pub width_meters: Option<f64>,
    pub height_meters: Option<f64>,
    pub orientation: Option<Option<String>>,
    pub row_type: Option<Option<String>>,
    pub calculated_volume_cubic_meters: Option<Option<f64>>,
    pub number_of_shelves: Option<Option<i32>>,
    pub shelf_spacing_cm: Option<Option<i32>>,
    pub position_x: Option<Option<f64>>,
    pub position_y: Option<Option<f64>>,
    pub access_side: Option<Option<String>>,
    pub aisle_width_cm: Option<Option<i32>>,
    pub equipment_access: Option<Option<String>>,
    pub priority_level: Option<String>,
    pub temperature_zone: Option<String>,
    pub notes: Option<Option<String>>,
    pub enable_automatic_bin_labeling: Option<bool>,
    pub generate_qr_codes_for_shelves: Option<bool>,
}

/// POST body for create and update (path ids in the body are ignored).
#[derive(Deserialize)]
pub struct RowBody {
    #[serde(default, deserialize_with = "present")]
    row_code: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    row_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    orientation: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    row_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    length_meters: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    width_meters: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    height_meters: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    calculated_volume_cubic_meters: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    number_of_shelves: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    shelf_spacing_cm: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    position_x: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    position_y: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    access_side: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    aisle_width_cm: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    equipment_access: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    priority_level: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    temperature_zone: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    notes: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    enable_automatic_bin_labeling: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    generate_qr_codes_for_shelves: Option<Option<Value>>,
}

/// POST /rows: code, name and the three sizes required.
#[derive(Deserialize)]
pub struct CreateRowRequest(RowBody);

/// POST /rows/{row}: everything optional (absent keeps the stored value).
#[derive(Deserialize)]
pub struct UpdateRowRequest(RowBody);

impl Validate for CreateRowRequest {
    type Valid = Fields;

    fn validate(self, r: &mut Rules) -> Option<Fields> {
        let b = self.0;
        let mut f = fields(r, b, true);
        f.priority_level.get_or_insert_with(|| "standard".into());
        f.temperature_zone.get_or_insert_with(|| "ambient".into());
        Some(f)
    }
}

impl Validate for UpdateRowRequest {
    type Valid = Fields;

    fn validate(self, r: &mut Rules) -> Option<Fields> {
        Some(fields(r, self.0, false))
    }
}

fn fields(r: &mut Rules, b: RowBody, create: bool) -> Fields {
    // Create: missing = required error. Update: absent = keep, null/blank = required.
    let req_text = |r: &mut Rules, key: &str, v: Option<Option<String>>, max: usize| {
        if create {
            Some(r.text(key, v.flatten(), max))
        } else {
            r.patch_text(key, v, max)
        }
    };
    let req_size = |r: &mut Rules, key: &str, v: Option<Option<Value>>| match (create, v) {
        (false, None) => None,
        (_, v) => {
            let v = r.required_value(key, v.flatten())?;
            r.number(key, &v, Decimal::ZERO)
        }
    };
    let opt_num = |r: &mut Rules, key: &str, v: Option<Option<Value>>, min: Decimal| {
        v.map(|v| filled(v).and_then(|v| r.number(key, &v, min)))
    };
    let flag = |r: &mut Rules, key: &str, v: Option<Option<Value>>| {
        v.map(|v| {
            filled(v)
                .and_then(|v| r.boolean(key, &v))
                .unwrap_or_default()
        })
    };
    let any = Decimal::MIN;
    Fields {
        row_code: req_text(r, "row_code", b.row_code, 100),
        row_name: req_text(r, "row_name", b.row_name, 255),
        length_meters: req_size(r, "length_meters", b.length_meters),
        width_meters: req_size(r, "width_meters", b.width_meters),
        height_meters: req_size(r, "height_meters", b.height_meters),
        orientation: b.orientation.map(|v| r.opt_text("orientation", v, 100)),
        row_type: b.row_type.map(|v| r.opt_text("row_type", v, 100)),
        calculated_volume_cubic_meters: opt_num(
            r,
            "calculated_volume_cubic_meters",
            b.calculated_volume_cubic_meters,
            Decimal::ZERO,
        ),
        number_of_shelves: b
            .number_of_shelves
            .map(|v| count(r, "number_of_shelves", v)),
        shelf_spacing_cm: b.shelf_spacing_cm.map(|v| count(r, "shelf_spacing_cm", v)),
        position_x: opt_num(r, "position_x", b.position_x, any),
        position_y: opt_num(r, "position_y", b.position_y, any),
        access_side: b.access_side.map(|v| r.opt_text("access_side", v, 100)),
        aisle_width_cm: b.aisle_width_cm.map(|v| count(r, "aisle_width_cm", v)),
        equipment_access: b
            .equipment_access
            .map(|v| r.opt_text("equipment_access", v, 100)),
        priority_level: b.priority_level.map(|v| {
            r.one_of("priority_level", v, &PRIORITIES, None)
                .unwrap_or_else(|| "standard".into())
        }),
        temperature_zone: b.temperature_zone.map(|v| {
            r.one_of("temperature_zone", v, &TEMPERATURES, None)
                .unwrap_or_else(|| "ambient".into())
        }),
        notes: b.notes.map(|v| r.opt_text("notes", v, usize::MAX)),
        enable_automatic_bin_labeling: flag(
            r,
            "enable_automatic_bin_labeling",
            b.enable_automatic_bin_labeling,
        ),
        generate_qr_codes_for_shelves: flag(
            r,
            "generate_qr_codes_for_shelves",
            b.generate_qr_codes_for_shelves,
        ),
    }
}
