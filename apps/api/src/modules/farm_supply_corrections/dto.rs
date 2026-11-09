//! Supply correction request/response bodies.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::farm_surveys::dto::Money,
};

const LINES_MAX: usize = 50;
// 999_999_999_999 = 0xE8_D4A5_0FFF, scale 2: numeric(12,2).
const KG_MAX: Decimal = Decimal::from_parts(0xD4A5_0FFF, 0xE8, 0, false, 2);

/// POST /farm/supply-batches/{batch}/corrections.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Correct the kg of bought deliveries. Each line gives the kg a farmer should have on a share line; a wrong farmer is two lines on one share (the share's farmer to less, the right farmer to more)."
)]
pub struct CreateSupplyCorrectionRequest {
    /// Retry key: the same key and body replay; another body under it is a 409.
    #[schema(value_type = Uuid)]
    client_ref: Option<String>,
    /// Why, 1-500 chars. The farmer sees it on the passbook line.
    #[schema(value_type = String, example = "Scale read 20 kg high")]
    reason: Option<String>,
    /// 1-50 lines, each (share, farmer) once.
    #[schema(value_type = Vec<SupplyCorrectionLineInput>)]
    lines: Option<Value>,
}

/// One line of a correction request.
// Doc-only: `lines` item schema; the lines are read from the raw JSON, never constructed.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct SupplyCorrectionLineInput {
    #[schema(value_type = Uuid)]
    share_id: Option<String>,
    /// The farmer whose kg change; default the share's farmer. Another farmer of the batch's
    /// village takes kg moved from a wrong farmer.
    #[schema(value_type = Option<Uuid>)]
    person_id: Option<String>,
    /// The kg this farmer should have on this share, 0 or more, at most 2 decimals.
    #[schema(value_type = String, example = "40.00")]
    kg: Option<Value>,
}

/// Validated correction input.
pub struct NewCorrection {
    pub client_ref: Uuid,
    pub reason: String,
    /// (share, person when sent, target kg), in the order sent (error keys name the indexes).
    pub lines: Vec<(Uuid, Option<Uuid>, Decimal)>,
}

impl NewCorrection {
    /// sha256 of the canonical body: kg at 2 decimals, lines sorted. A replay must match.
    pub fn hash(&self, batch_id: Uuid) -> [u8; 32] {
        let mut lines: Vec<String> = self
            .lines
            .iter()
            .map(|(s, p, kg)| json!([s, p, format!("{kg:.2}")]).to_string())
            .collect();
        lines.sort();
        Sha256::digest(json!([batch_id, self.reason, lines]).to_string()).into()
    }
}

fn uuid(r: &mut Rules, field: &str, v: Option<&str>) -> Option<Uuid> {
    let id = v.and_then(|v| Uuid::parse_str(v.trim()).ok());
    r.check(
        field,
        id.is_some(),
        format!(
            "The {} must be a valid UUID.",
            field.rsplit('.').next().unwrap_or(field)
        ),
    );
    id
}

impl Validate for CreateSupplyCorrectionRequest {
    type Valid = NewCorrection;

    fn validate(self, r: &mut Rules) -> Option<NewCorrection> {
        let client_ref = r
            .required("client_ref", self.client_ref)
            .and_then(|v| uuid(r, "client_ref", Some(&v)));
        let reason = r.text("reason", self.reason, 500);
        let mut lines = Vec::new();
        match r.required_value("lines", self.lines) {
            Some(Value::Array(list)) if (1..=LINES_MAX).contains(&list.len()) => {
                for (n, line) in list.iter().enumerate() {
                    let text = |k: &str| line.get(k).and_then(Value::as_str);
                    let share = uuid(r, &format!("lines.{n}.share_id"), text("share_id"));
                    let person = match line.get("person_id") {
                        None | Some(Value::Null) => None,
                        _ => uuid(r, &format!("lines.{n}.person_id"), text("person_id")),
                    };
                    let field = format!("lines.{n}.kg");
                    let kg = match line.get("kg") {
                        None | Some(Value::Null) => {
                            r.missing(&field);
                            None
                        }
                        Some(v) => r.decimal(&field, v, Decimal::ZERO, KG_MAX),
                    };
                    if let Some(kg) = kg {
                        r.check(
                            &field,
                            kg.scale() <= 2,
                            "The kg must have at most 2 decimal places.",
                        );
                    }
                    if let (Some(s), Some(kg)) = (share, kg) {
                        lines.push((s, person, kg));
                    }
                }
            }
            Some(_) => r.add("lines", "The lines field must list 1 to 50 lines."),
            None => {}
        }
        Some(NewCorrection {
            client_ref: client_ref?,
            reason,
            lines,
        })
    }
}

/// A correction: what was asked, its decision, and its effect per farmer.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A kg correction of a bought delivery. Pending until a second ops or admin approves or rejects it. Approval re-splits past credits by the corrected kg (one passbook line per farmer) and moves the raw lot's stock by `net_kg`."
)]
pub struct SupplyCorrectionResource {
    pub id: Uuid,
    pub batch_id: Uuid,
    pub village_id: Uuid,
    /// `pending`, `approved` or `rejected`.
    pub status: String,
    pub reason: String,
    pub requested_by: Uuid,
    pub requested_at: DateTime<Utc>,
    #[schema(required = true)]
    pub decided_by: Option<Uuid>,
    #[schema(required = true)]
    pub decided_at: Option<DateTime<Utc>>,
    /// Change of the batch's kg (and the raw lot's stock): negative is a cut.
    #[schema(example = "-20.00")]
    pub net_kg: String,
    pub lines: Vec<SupplyCorrectionLine>,
    /// Farmers whose money moves: the preview while pending, what was posted once approved,
    /// none when rejected.
    pub farmers: Vec<SupplyFarmerEffect>,
}

/// One kg change.
#[derive(Serialize, ToSchema)]
pub struct SupplyCorrectionLine {
    pub share_id: Uuid,
    pub person_id: Uuid,
    /// Given and family name.
    pub farmer_name: String,
    #[schema(example = "-20.00")]
    pub kg_delta: String,
    /// The farmer's kg on this share now; only while pending.
    #[schema(required = true, example = "120.50")]
    pub kg_before: Option<String>,
    /// The kg after approval; only while pending.
    #[schema(required = true, example = "100.50")]
    pub kg_after: Option<String>,
}

/// A farmer's money effect.
#[derive(Serialize, ToSchema)]
pub struct SupplyFarmerEffect {
    pub person_id: Uuid,
    /// Given and family name.
    pub name: String,
    /// Signed change of their passbook balance.
    pub change: Money,
    /// Their balance after approval (negative: they owe the co-op and payouts are blocked
    /// until later credits cover it); only while pending.
    #[schema(required = true)]
    pub balance_after: Option<Money>,
}

/// GET /farm/supply-corrections query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SupplyCorrectionListQuery {
    /// `pending`, `approved` or `rejected`; blank = all.
    pub status: Option<String>,
    /// Only corrections of this batch.
    #[param(value_type = Option<Uuid>)]
    pub batch_id: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}
