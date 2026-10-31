//! Bin bodies.
// Laravel WarehouseSectionRowBinResource, WarehouseBinHierarchyResource and
// Store/UpdateWarehouseSectionRowBinRequest (vimdesk-web binPayloadTransformer.ts).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::{
        warehouse_bins::repo::{BinRow, HierarchyRow},
        warehouse_sections::dto::{PRIORITIES, WarehouseRef, count},
        warehouse_shelves::dto::SIDES,
    },
};

/// Laravel validation.unique on `bin_code`.
pub const CODE_TAKEN: &str = "The bin code has already been taken.";
const BIN_TYPES: [&str; 5] = ["standard", "oversize", "fragile", "hazmat", "cold_storage"];
const ACCESS_TYPES: [&str; 3] = ["manual_access", "conveyor", "robotic_pick"];

// Laravel WarehouseSectionRowBinResource (parent ids as uuids, Laravel printed integers).
#[derive(Serialize, ToSchema)]
#[schema(description = "A storage bin on a shelf.")]
pub struct BinResource {
    id: Uuid,
    bin_id: Uuid,
    workspace_id: Uuid,
    warehouse_id: Uuid,
    section_id: Uuid,
    row_id: Uuid,
    shelf_id: Uuid,
    uuid: Uuid,
    bin_code: String,
    #[schema(required = true)]
    bin_name: Option<String>,
    bin_type: String,
    #[schema(required = true)]
    length_cm: Option<i32>,
    #[schema(required = true)]
    width_cm: Option<i32>,
    #[schema(required = true)]
    height_cm: Option<i32>,
    #[schema(required = true)]
    max_weight_kg: Option<i32>,
    #[schema(required = true)]
    load_rating: Option<String>,
    access_type: String,
    priority_level: String,
    #[schema(value_type = Object)]
    /// `side`, `position`, `bin_type`.
    basic_information: Value,
    #[schema(value_type = Object)]
    /// `length_cm`, `width_cm`, `height_cm`, `volume_m3`, `max_weight_kg`, `load_rating`.
    physical_specifications: Value,
    #[schema(value_type = Object)]
    /// `access_type`, `priority_level`, `special_features` (`temperature_monitoring`,
    /// `barcode_scanner`, `rfid_enabled`, `security_lock`), `notes`.
    access_configuration: Value,
    // Laravel loaded `stocks` there.
    /// Single-bin GET only; always empty.
    // shortcut: always empty until stock (6F) exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<Object>>)]
    stocks: Option<[(); 0]>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl BinResource {
    /// `with_stocks`: the single-bin GET.
    pub fn new(b: BinRow, with_stocks: bool) -> Self {
        Self {
            id: b.id,
            bin_id: b.id,
            uuid: b.id,
            workspace_id: b.workspace_id,
            warehouse_id: b.warehouse_id,
            section_id: b.section_id,
            row_id: b.row_id,
            shelf_id: b.shelf_id,
            basic_information: json!({
                "side": b.side, "position": b.position, "bin_type": b.bin_type,
            }),
            physical_specifications: json!({
                "length_cm": b.length_cm, "width_cm": b.width_cm, "height_cm": b.height_cm,
                "volume_m3": b.volume_m3, "max_weight_kg": b.max_weight_kg,
                "load_rating": b.load_rating,
            }),
            access_configuration: json!({
                "access_type": b.access_type, "priority_level": b.priority_level,
                "special_features": {
                    "temperature_monitoring": b.temperature_monitoring,
                    "barcode_scanner": b.barcode_scanner,
                    "rfid_enabled": b.rfid_enabled,
                    "security_lock": b.security_lock,
                },
                "notes": b.notes,
            }),
            stocks: with_stocks.then_some([]),
            bin_code: b.bin_code,
            bin_name: b.bin_name,
            bin_type: b.bin_type,
            length_cm: b.length_cm,
            width_cm: b.width_cm,
            height_cm: b.height_cm,
            max_weight_kg: b.max_weight_kg,
            load_rating: b.load_rating,
            access_type: b.access_type,
            priority_level: b.priority_level,
            created_at: b.created_at,
            updated_at: b.updated_at,
        }
    }
}

// Laravel WarehouseBinHierarchyResource: a bin with its whole path, for bin pickers.
#[derive(Serialize, ToSchema)]
#[schema(description = "A bin with its whole warehouse path, for bin pickers.")]
pub struct BinHierarchyResource {
    id: Uuid,
    bin_code: String,
    #[schema(required = true)]
    bin_name: Option<String>,
    warehouse: WarehouseRef,
    #[schema(value_type = Object)]
    /// `id`, `section_code`, `section_name`.
    section: Value,
    #[schema(value_type = Object)]
    /// `id`, `row_code`, `row_name`.
    row: Value,
    #[schema(value_type = Object)]
    /// `id`, `shelf_code`, `shelf_name`.
    shelf: Value,
}

impl From<HierarchyRow> for BinHierarchyResource {
    fn from(h: HierarchyRow) -> Self {
        Self {
            id: h.id,
            bin_code: h.bin_code,
            bin_name: h.bin_name,
            warehouse: WarehouseRef {
                id: h.warehouse_id,
                name: h.warehouse_name,
            },
            section: json!({
                "id": h.section_id, "section_code": h.section_code, "section_name": h.section_name,
            }),
            row: json!({ "id": h.row_id, "row_code": h.row_code, "row_name": h.row_name }),
            shelf: json!({
                "id": h.shelf_id, "shelf_code": h.shelf_code, "shelf_name": h.shelf_name,
            }),
        }
    }
}

/// Create response.
#[derive(Serialize, ToSchema)]
pub struct BinCreated {
    pub bin_id: Uuid,
}

/// Columns create writes and update patches. `None` = keep (update).
#[derive(Default)]
pub struct Fields {
    pub bin_code: Option<String>,
    pub bin_name: Option<Option<String>>,
    pub side: Option<Option<String>>,
    pub position: Option<Option<String>>,
    pub bin_type: Option<String>,
    pub length_cm: Option<Option<i32>>,
    pub width_cm: Option<Option<i32>>,
    pub height_cm: Option<Option<i32>>,
    pub volume_m3: Option<Option<f64>>,
    pub max_weight_kg: Option<Option<i32>>,
    pub load_rating: Option<Option<String>>,
    pub access_type: Option<String>,
    pub priority_level: Option<String>,
    pub temperature_monitoring: Option<bool>,
    pub barcode_scanner: Option<bool>,
    pub rfid_enabled: Option<bool>,
    pub security_lock: Option<bool>,
    pub notes: Option<Option<String>>,
}

/// POST body for create and update (path ids and `uuid` in the body are ignored).
#[derive(Deserialize, ToSchema)]
#[schema(as = WarehouseBinRequest, description = "Create needs `bin_code`; update keeps fields left out.")]
pub struct BinBody {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    bin_code: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    bin_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    side: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    position: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    bin_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    length_cm: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    width_cm: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    height_cm: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<f64>)]
    volume_m3: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    max_weight_kg: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    load_rating: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    access_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    priority_level: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    temperature_monitoring: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    barcode_scanner: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    rfid_enabled: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<bool>)]
    security_lock: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    notes: Option<Option<String>>,
}

/// POST /bins: code required, Laravel defaults for type/access/priority.
#[derive(Deserialize)]
pub struct CreateBinRequest(BinBody);

/// Validated create: the code as a plain value, the rest in `fields`.
pub struct NewBin {
    pub code: String,
    pub fields: Fields,
}

/// POST /bins/{bin}: absent keys keep the stored value.
#[derive(Deserialize)]
pub struct UpdateBinRequest(BinBody);

impl Validate for CreateBinRequest {
    type Valid = NewBin;

    fn validate(self, r: &mut Rules) -> Option<NewBin> {
        let mut f = fields(r, self.0, true);
        // A missing code already recorded its error; the 422 discards the default.
        Some(NewBin {
            code: f.bin_code.take().unwrap_or_default(),
            fields: f,
        })
    }
}

impl Validate for UpdateBinRequest {
    type Valid = Fields;

    fn validate(self, r: &mut Rules) -> Option<Fields> {
        Some(fields(r, self.0, false))
    }
}

fn fields(r: &mut Rules, b: BinBody, create: bool) -> Fields {
    let bin_code = if create {
        Some(r.text("bin_code", b.bin_code.flatten(), 150))
    } else {
        r.patch_text("bin_code", b.bin_code, 150)
    };
    let pick =
        |r: &mut Rules, key: &str, v: Option<Option<String>>, allowed: &[&str], dflt: &str| {
            let v = if create { Some(v.flatten()) } else { v };
            v.map(|v| {
                r.one_of(key, v, allowed, None)
                    .unwrap_or_else(|| dflt.into())
            })
        };
    let flag = |r: &mut Rules, key: &str, v: Option<Option<Value>>| {
        let v = if create { Some(v.flatten()) } else { v };
        v.map(|v| {
            filled(v)
                .and_then(|v| r.boolean(key, &v))
                .unwrap_or_default()
        })
    };
    Fields {
        bin_code,
        bin_name: b.bin_name.map(|v| r.opt_text("bin_name", v, 255)),
        side: b.side.map(|v| r.one_of("side", v, &SIDES, None)),
        position: b.position.map(|v| r.opt_text("position", v, 100)),
        bin_type: pick(r, "bin_type", b.bin_type, &BIN_TYPES, "standard"),
        length_cm: b.length_cm.map(|v| count(r, "length_cm", v)),
        width_cm: b.width_cm.map(|v| count(r, "width_cm", v)),
        height_cm: b.height_cm.map(|v| count(r, "height_cm", v)),
        volume_m3: b
            .volume_m3
            .map(|v| filled(v).and_then(|v| r.number("volume_m3", &v, Decimal::ZERO))),
        max_weight_kg: b.max_weight_kg.map(|v| count(r, "max_weight_kg", v)),
        load_rating: b.load_rating.map(|v| r.opt_text("load_rating", v, 100)),
        access_type: pick(
            r,
            "access_type",
            b.access_type,
            &ACCESS_TYPES,
            "manual_access",
        ),
        priority_level: pick(
            r,
            "priority_level",
            b.priority_level,
            &PRIORITIES,
            "standard",
        ),
        temperature_monitoring: flag(r, "temperature_monitoring", b.temperature_monitoring),
        barcode_scanner: flag(r, "barcode_scanner", b.barcode_scanner),
        rfid_enabled: flag(r, "rfid_enabled", b.rfid_enabled),
        security_lock: flag(r, "security_lock", b.security_lock),
        notes: b.notes.map(|v| r.opt_text("notes", v, usize::MAX)),
    }
}
