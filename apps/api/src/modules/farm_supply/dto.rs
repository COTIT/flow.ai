//! Supply batch request/response bodies.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{
        farm_supply::repo::{BatchRow, LineRow},
        lots::dto::code,
    },
};

const LINES_MAX: usize = 500;
/// numeric(12,2).
// 999_999_999_999 = 0xE8_D4A5_0FFF, scale 2 (`Decimal::new` is not const).
const KG_MAX: Decimal = Decimal::from_parts(0xD4A5_0FFF, 0xE8, 0, false, 2);

/// A supply batch: what was bought, at what price, and each farmer's kg.
#[derive(Serialize, ToSchema)]
#[schema(description = "Verified harvests bought at one price per kg, held as one raw lot.")]
pub struct BatchResource {
    id: Uuid,
    client_ref: Uuid,
    village_id: Uuid,
    crop_id: Uuid,
    /// Minor units of the currency per kg.
    #[schema(example = 800000)]
    price_per_kg: i64,
    currency_id: Uuid,
    /// Kilograms as decimal text (2 decimals).
    #[schema(example = "370.50")]
    kg_total: String,
    lot: BatchLot,
    /// One per harvest, by harvest id.
    lines: Vec<BatchLine>,
    created_by: Uuid,
    created_at: DateTime<Utc>,
}

/// The raw lot a batch became.
#[derive(Serialize, ToSchema)]
pub struct BatchLot {
    id: Uuid,
    code: String,
    public_code: String,
}

/// One farmer's kg from one harvest.
#[derive(Serialize, ToSchema)]
pub struct BatchLine {
    /// The share line; a correction names it.
    share_id: Uuid,
    harvest_report_id: Uuid,
    /// Credited farmer: the farm's primary manager when the batch was made.
    person_id: Uuid,
    #[schema(example = "250.00")]
    kg: String,
}

impl BatchResource {
    /// From the stored batch and its lines.
    pub fn new(b: BatchRow, lines: Vec<LineRow>) -> Self {
        Self {
            id: b.id,
            client_ref: b.client_ref,
            village_id: b.village_id,
            crop_id: b.crop_id,
            price_per_kg: b.price_per_kg,
            currency_id: b.currency_id,
            kg_total: format!("{:.2}", b.kg_total),
            lot: BatchLot {
                id: b.lot_id,
                code: b.lot_code,
                public_code: b.public_code,
            },
            lines: lines
                .into_iter()
                .map(|l| BatchLine {
                    share_id: l.id,
                    harvest_report_id: l.harvest_report_id,
                    person_id: l.person_id,
                    kg: format!("{:.2}", l.kg),
                })
                .collect(),
            created_by: b.created_by,
            created_at: b.created_at,
        }
    }
}

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// GET /farm/quiet-farmers query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QuietQuery {
    /// Tanzania day (`YYYY-MM-DD`): farmers with no bought kg on or after it. Default: 30
    /// days before today.
    #[param(value_type = Option<chrono::NaiveDate>)]
    pub since: Option<String>,
    /// Only farmers living in this village.
    #[param(value_type = Option<Uuid>)]
    pub village_id: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// A farmer with no delivery since the asked day.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A registered person whose last bought kg (a supply batch line) is before the asked day, or who never delivered."
)]
pub struct QuietFarmer {
    pub person_id: Uuid,
    /// Given and family name.
    pub name: String,
    pub village_id: Uuid,
    /// Village name.
    pub village: String,
    /// When their kg was last bought; null = never.
    #[schema(required = true)]
    pub last_delivery_at: Option<DateTime<Utc>>,
}

/// POST /farm/supply-batches.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Buy verified actual harvests of one village and crop into a raw lot.")]
pub struct CreateBatchRequest {
    /// Retry key: the same key and body replay; another body under it is a 409.
    #[schema(value_type = Uuid)]
    client_ref: Option<String>,
    #[schema(value_type = Uuid)]
    village_id: Option<String>,
    #[schema(value_type = Uuid)]
    crop_id: Option<String>,
    /// The raw lot's code (letters, digits, `. _ / -`, unique per workspace).
    #[schema(value_type = String, example = "ILU-H-2026-01")]
    code: Option<String>,
    /// Stock-tracked product of the raw lot.
    #[schema(value_type = Uuid)]
    raw_product_id: Option<String>,
    /// Agreed price, minor units of `currency_id` per kg; frozen with the batch.
    #[schema(value_type = i64, minimum = 1, example = 800000)]
    price_per_kg: Option<Value>,
    #[schema(value_type = Uuid)]
    currency_id: Option<String>,
    /// Where the bought kg go in (buying is the receipt). Default: the workspace's default
    /// warehouse; with neither, the raw lot is made empty.
    #[schema(value_type = Option<Uuid>)]
    warehouse_id: Option<String>,
    /// 1-500 lines, each harvest once.
    #[schema(value_type = Vec<LineInput>)]
    lines: Option<Value>,
}

/// One line of a new batch.
// Doc-only: `lines` item schema; the lines are read from the raw JSON, never constructed.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct LineInput {
    #[schema(value_type = Uuid)]
    harvest_report_id: Option<String>,
    /// Kilograms, more than 0, at most 2 decimals.
    #[schema(value_type = String, example = "250.00")]
    kg: Option<Value>,
}

/// Validated batch input.
pub struct NewBatch {
    pub client_ref: Uuid,
    pub village_id: Uuid,
    pub crop_id: Uuid,
    pub code: String,
    pub raw_product_id: Uuid,
    pub price_per_kg: i64,
    pub currency_id: Uuid,
    /// Where the bought kg go in, as sent.
    pub warehouse_id: Option<Uuid>,
    /// (harvest report, kg), in the order sent (error keys name these indexes).
    pub lines: Vec<(Uuid, Decimal)>,
}

impl NewBatch {
    /// sha256 of the canonical body: kg at 2 decimals, lines by harvest id. A replay must match.
    pub fn hash(&self) -> [u8; 32] {
        let mut lines: Vec<Value> = self
            .lines
            .iter()
            .map(|(h, kg)| json!([h, format!("{kg:.2}")]))
            .collect();
        lines.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
        let mut canonical = json!([
            self.village_id,
            self.crop_id,
            self.code,
            self.raw_product_id,
            self.price_per_kg,
            self.currency_id,
            lines
        ]);
        // Only when sent, so bodies without it hash as before.
        if let (Some(w), Some(all)) = (self.warehouse_id, canonical.as_array_mut()) {
            all.push(json!(w));
        }
        Sha256::digest(canonical.to_string()).into()
    }
}

fn uuid(r: &mut Rules, field: &str, v: Option<String>) -> Uuid {
    let Some(v) = r.required(field, v) else {
        return Uuid::nil();
    };
    let id = Uuid::parse_str(v.trim()).ok();
    r.check(
        field,
        id.is_some(),
        format!("The {field} must be a valid UUID."),
    );
    id.unwrap_or_default()
}

fn kg(r: &mut Rules, field: &str, v: Option<Value>) -> Decimal {
    let Some(v) = r.required_value(field, v) else {
        return Decimal::ZERO;
    };
    let Some(kg) = r.decimal(field, &v, Decimal::new(1, 2), KG_MAX) else {
        return Decimal::ZERO;
    };
    r.check(
        field,
        kg.scale() <= 2,
        "The kg must have at most 2 decimal places.",
    );
    kg
}

fn lines(r: &mut Rules, v: Option<Value>) -> Vec<(Uuid, Decimal)> {
    let Some(v) = r.required_value("lines", v) else {
        return Vec::new();
    };
    let Some(list) = v.as_array().filter(|l| (1..=LINES_MAX).contains(&l.len())) else {
        r.add("lines", "The lines field must list 1 to 500 lines.");
        return Vec::new();
    };
    let mut out: Vec<(Uuid, Decimal)> = Vec::with_capacity(list.len());
    for (n, line) in list.iter().enumerate() {
        let text = |k: &str| line.get(k).and_then(Value::as_str).map(str::to_owned);
        let field = format!("lines.{n}.harvest_report_id");
        let h = uuid(r, &field, text("harvest_report_id"));
        if !h.is_nil() && out.iter().any(|(seen, _)| *seen == h) {
            r.add(&field, "The harvest report is listed twice.");
        }
        let kg = kg(r, &format!("lines.{n}.kg"), line.get("kg").cloned());
        out.push((h, kg));
    }
    let total = out
        .iter()
        .try_fold(Decimal::ZERO, |t, (_, kg)| t.checked_add(*kg))
        .filter(|t| *t <= KG_MAX);
    if total.is_none() {
        r.add(
            "lines",
            "The kg of all lines must not be greater than 9999999999.99.",
        );
    }
    out
}

impl Validate for CreateBatchRequest {
    type Valid = NewBatch;

    fn validate(self, r: &mut Rules) -> Option<NewBatch> {
        let price_per_kg = r
            .required_value("price_per_kg", self.price_per_kg)
            .and_then(|v| r.integer("price_per_kg", &v, 1, i64::MAX))
            .unwrap_or_default();
        let lines = lines(r, self.lines);
        let kg: Decimal = lines
            .iter()
            .map(|l| l.1)
            .fold(Decimal::ZERO, |t, k| t.saturating_add(k));
        // Credits are price × kg in bigint minor units: the whole batch must fit.
        let fits = Decimal::from(price_per_kg)
            .checked_mul(kg)
            .is_some_and(|t| t <= Decimal::from(i64::MAX));
        if !fits {
            r.add(
                "price_per_kg",
                "The price per kg times the kg is too large.",
            );
        }
        Some(NewBatch {
            client_ref: uuid(r, "client_ref", self.client_ref),
            village_id: uuid(r, "village_id", self.village_id),
            crop_id: uuid(r, "crop_id", self.crop_id),
            code: code(r, self.code),
            raw_product_id: uuid(r, "raw_product_id", self.raw_product_id),
            price_per_kg,
            currency_id: uuid(r, "currency_id", self.currency_id),
            warehouse_id: self
                .warehouse_id
                .filter(|v| !v.trim().is_empty())
                .map(|v| uuid(r, "warehouse_id", Some(v))),
            lines,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid(body: Value) -> Result<NewBatch, Vec<String>> {
        let req: CreateBatchRequest = serde_json::from_value(body).unwrap();
        let mut r = Rules::default();
        let v = req.validate(&mut r);
        match r.into_result() {
            Ok(()) => Ok(v.unwrap()),
            Err(crate::error::AppError::Validation(e)) => Err(e.into_keys().collect()),
            Err(e) => panic!("{e}"),
        }
    }

    fn body(lines: Value) -> Value {
        let id = "01900000-0000-7000-8000-000000000001";
        json!({ "client_ref": id, "village_id": id, "crop_id": id, "code": "A1",
                "raw_product_id": id, "price_per_kg": 800000, "currency_id": id, "lines": lines })
    }

    #[test]
    fn hash_ignores_line_order_and_kg_spelling() {
        let (a, b) = (
            "01900000-0000-7000-8000-00000000000a",
            "01900000-0000-7000-8000-00000000000b",
        );
        let one = valid(body(json!([{ "harvest_report_id": a, "kg": "1.5" },
                                    { "harvest_report_id": b, "kg": 2 }])))
        .unwrap();
        let two = valid(body(json!([{ "harvest_report_id": b, "kg": "2.00" },
                                    { "harvest_report_id": a, "kg": "1.50" }])))
        .unwrap();
        assert_eq!(one.hash(), two.hash());
        let three = valid(body(json!([{ "harvest_report_id": a, "kg": "1.51" },
                                      { "harvest_report_id": b, "kg": 2 }])))
        .unwrap();
        assert_ne!(one.hash(), three.hash());
    }

    #[test]
    fn kg_bounds_and_lines_count() {
        let a = "01900000-0000-7000-8000-00000000000a";
        assert!(valid(body(json!([{ "harvest_report_id": a, "kg": "0.01" }]))).is_ok());
        assert!(
            valid(body(
                json!([{ "harvest_report_id": a, "kg": "9999999999.99" }])
            ))
            .is_ok()
        );
        for kg in [
            json!("0"),
            json!("-1"),
            json!("0.001"),
            json!("10000000000"),
            json!("x"),
        ] {
            assert_eq!(
                valid(body(json!([{ "harvest_report_id": a, "kg": kg }]))).err(),
                Some(vec!["lines.0.kg".to_owned()]),
                "{kg}"
            );
        }
        let many: Vec<Value> = (0..501)
            .map(|i| json!({ "harvest_report_id": Uuid::from_u128(i + 1), "kg": "1" }))
            .collect();
        assert_eq!(
            valid(body(json!(many))).err(),
            Some(vec!["lines".to_owned()])
        );
    }
}

/// GET options query: the village and crop a new batch is for.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OptionsQuery {
    /// Required.
    #[param(value_type = Option<Uuid>)]
    pub village_id: Option<String>,
    /// Required.
    #[param(value_type = Option<Uuid>)]
    pub crop_id: Option<String>,
}

impl OptionsQuery {
    /// (village, crop); 422 when either is missing or not a uuid.
    pub fn ids(&self) -> Result<(Uuid, Uuid), crate::error::AppError> {
        let mut r = Rules::default();
        let village = uuid(&mut r, "village_id", self.village_id.clone());
        let crop = uuid(&mut r, "crop_id", self.crop_id.clone());
        r.into_result()?;
        Ok((village, crop))
    }
}

/// What a new batch of one village and crop can be made of.
#[derive(Serialize, ToSchema)]
#[schema(description = "Harvests a new batch may buy and the products its raw lot may be.")]
pub struct OptionsResource {
    /// Current, verified, actual harvests of the village and crop with kg left, the caller verified none of.
    pub harvests: Vec<BuyableHarvest>,
    /// Stock-tracked products, suggestions for the crop first, then by name.
    pub raw_products: Vec<RawProduct>,
    /// The caller may add a raw product here (`POST …/supply-batches/raw-product`): ops and admin.
    pub can_create_product: bool,
}

/// One harvest a batch line may buy.
#[derive(Serialize, ToSchema)]
pub struct BuyableHarvest {
    pub harvest_report_id: Uuid,
    /// Primary manager of the harvest's farm (the farmer a batch credits).
    pub farmer_name: String,
    pub plot_label: String,
    #[schema(example = "300.00")]
    pub quantity_kg: String,
    /// Kg already in batches and opportunities.
    #[schema(example = "100.00")]
    pub committed_kg: String,
    #[schema(example = "200.00")]
    pub available_kg: String,
}

/// A product a raw lot may be of.
#[derive(Serialize, ToSchema)]
pub struct RawProduct {
    pub id: Uuid,
    pub name: String,
    #[schema(example = "kg")]
    pub unit: Option<String>,
    /// Hint only: an earlier batch of this crop used it, or its name holds the crop's name.
    /// Batch create accepts any stock-tracked product.
    pub suggested_for_crop: bool,
}

/// POST /farm/supply-batches/raw-product.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "A raw material for supply batches: stock-tracked, kg, net price 0, off the shop."
)]
pub struct RawProductRequest {
    /// Required, at most 255 characters.
    #[schema(value_type = String, example = "Raw coffee")]
    name: Option<Value>,
}

impl Validate for RawProductRequest {
    type Valid = crate::modules::products::dto::Fields;

    fn validate(self, r: &mut Rules) -> Option<Self::Valid> {
        crate::modules::products::dto::CreateProductRequest::raw_material(self.name).validate(r)
    }
}

/// The raw product just made; its `id` is a valid `raw_product_id`.
#[derive(Serialize, ToSchema)]
pub struct RawProductCreated {
    pub id: Uuid,
    pub name: String,
    #[schema(example = "kg")]
    pub unit: Option<String>,
}

/// What the Field dispatch form may send: farm-guarded, so ops need no office rights.
#[derive(Serialize, ToSchema)]
pub struct DispatchOptions {
    /// Live supply links this workspace granted.
    pub links: Vec<DispatchLinkResource>,
    /// Raw lots of supply batches, newest first (`POST /lots/{lot}/transfers` sends these).
    pub lots: Vec<DispatchLot>,
    /// Warehouses holding free stock of those lots, by name.
    pub warehouses: Vec<DispatchWarehouse>,
}

/// A live link and the shop it goes to.
#[derive(Serialize, ToSchema)]
pub struct DispatchLinkResource {
    pub id: Uuid,
    pub partner: crate::modules::supply_links::dto::Partner,
}

/// A lot that can be sent.
#[derive(Serialize, ToSchema)]
pub struct DispatchLot {
    pub id: Uuid,
    pub code: String,
}

/// A warehouse to send from.
#[derive(Serialize, ToSchema)]
pub struct DispatchWarehouse {
    pub id: Uuid,
    pub name: String,
}
