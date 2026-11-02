//! Market request/response bodies. kg are decimal text with 2 decimals; prices are INDICATIVE
//! integer minor units of a workspace currency.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, Validate, filled, optional},
    modules::{
        farm_market::{
            repo::{
                AvailableRow, BuyerRow, DemandRow, LineRow, MatchRow, NewBuyer, NewDemand,
                SupplyRow,
            },
            rules::STATUSES,
            service::{OppFilter, Opportunity},
        },
        farm_records::dto::ActorRef,
        farm_surveys::dto::Money,
    },
};

const NOTE_MAX: usize = 1000;

fn kg(d: Decimal) -> String {
    format!("{d:.2}")
}

fn uuid(r: &mut Rules, field: &str, value: Option<String>) -> Option<Uuid> {
    let v = optional(value)?;
    let id = Uuid::parse_str(&v).ok();
    let f = field.replace('_', " ");
    r.check(
        field,
        id.is_some(),
        format!("The {f} must be a valid UUID."),
    );
    id
}

fn required_uuid(r: &mut Rules, field: &str, value: Option<String>) -> Option<Uuid> {
    if optional(value.clone()).is_none() {
        r.missing(field);
        return None;
    }
    uuid(r, field, value)
}

fn date(r: &mut Rules, field: &str, value: Option<String>) -> Option<NaiveDate> {
    let Some(v) = optional(value) else {
        r.missing(field);
        return None;
    };
    let d = NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok();
    let f = field.replace('_', " ");
    r.check(
        field,
        d.is_some(),
        format!("The {f} must be a date (YYYY-MM-DD)."),
    );
    d
}

/// Comma-separated statuses.
fn statuses(value: Option<&String>) -> Option<Vec<String>> {
    optional(value.cloned()).map(|s| s.split(',').map(|x| x.trim().to_owned()).collect())
}

/// GET /farm/buyers query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BuyerQuery {
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// `true` = active buyers only, `false` = retired only.
    #[param(value_type = Option<bool>)]
    pub active: Option<String>,
}

impl BuyerQuery {
    pub fn active(&self) -> Option<bool> {
        match self.active.as_deref() {
            Some("true" | "1") => Some(true),
            Some("false" | "0") => Some(false),
            _ => None,
        }
    }
}

/// GET /farm/buyer-demands query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DemandQuery {
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// Comma-separated: `open`, `matched`, `closed`, `cancelled`.
    pub status: Option<String>,
}

impl DemandQuery {
    pub fn statuses(&self) -> Option<Vec<String>> {
        statuses(self.status.as_ref())
    }
}

/// GET /farm/opportunities query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OpportunityQuery {
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    #[param(value_type = Option<Uuid>)]
    pub village_id: Option<String>,
    #[param(value_type = Option<Uuid>)]
    pub buyer_demand_id: Option<String>,
    /// Comma-separated statuses.
    pub status: Option<String>,
}

impl OpportunityQuery {
    /// The filters; a malformed id is a 422.
    pub fn filter(&self) -> Result<OppFilter, AppError> {
        let id = |field: &str, v: &Option<String>| {
            optional(v.clone())
                .map(|v| Uuid::parse_str(&v))
                .transpose()
                .map_err(|_| {
                    let f = field.replace('_', " ");
                    AppError::field(field, format!("The {f} must be a valid UUID."))
                })
        };
        Ok(OppFilter {
            id: None,
            village: id("village_id", &self.village_id)?,
            demand: id("buyer_demand_id", &self.buyer_demand_id)?,
            statuses: statuses(self.status.as_ref()),
        })
    }
}

// ---- buyers ----

/// A buyer of the order book. `afm` is a channel label only.
#[derive(Serialize, ToSchema)]
pub struct BuyerResource {
    id: Uuid,
    name: String,
    /// `direct`, `afm` or `other`.
    channel: String,
    #[schema(required = true)]
    contact_note: Option<String>,
    is_active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<BuyerRow> for BuyerResource {
    fn from(b: BuyerRow) -> Self {
        Self {
            id: b.id,
            name: b.name,
            channel: b.channel,
            contact_note: b.contact_note,
            is_active: b.is_active,
            created_at: b.created_at,
            updated_at: b.updated_at,
        }
    }
}

/// POST /farm/buyers.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New buyer (ops or admin). A client `id` makes retries safe.")]
pub struct BuyerRequest {
    #[schema(value_type = Option<Uuid>)]
    id: Option<String>,
    #[schema(value_type = String, max_length = 120)]
    name: Option<String>,
    /// `direct` (default), `afm` or `other`.
    #[schema(value_type = Option<String>)]
    channel: Option<String>,
    #[schema(value_type = Option<String>, max_length = 500)]
    contact_note: Option<String>,
}

impl Validate for BuyerRequest {
    type Valid = NewBuyer;

    fn validate(self, r: &mut Rules) -> Option<NewBuyer> {
        Some(NewBuyer {
            id: uuid(r, "id", self.id),
            name: r.text("name", self.name, 120),
            channel: r
                .one_of("channel", self.channel, &["direct", "afm", "other"], None)
                .unwrap_or_else(|| "direct".into()),
            contact_note: r.opt_text("contact_note", self.contact_note, 500),
        })
    }
}

// ---- demand ----

/// A buyer named on a demand or opportunity.
#[derive(Serialize, ToSchema)]
pub struct MarketBuyerRef {
    id: Uuid,
    name: String,
}

/// A crop named on a market row.
#[derive(Serialize, ToSchema)]
pub struct MarketCropRef {
    id: Uuid,
    code: String,
    name_en: String,
    name_sw: String,
}

/// A buyer's stated requirement, recorded by ops.
#[derive(Serialize, ToSchema)]
pub struct BuyerDemandResource {
    id: Uuid,
    buyer: MarketBuyerRef,
    /// `direct`, `afm` or `other`.
    buyer_channel: String,
    crop: MarketCropRef,
    #[schema(example = "5000.00")]
    quantity_kg: String,
    #[schema(required = true)]
    quality_note: Option<String>,
    window_start: NaiveDate,
    window_end: NaiveDate,
    #[schema(required = true)]
    delivery_point: Option<String>,
    /// INDICATIVE price per kg.
    #[schema(required = true)]
    indicative_price_per_kg: Option<Money>,
    /// `open`, `matched`, `closed` or `cancelled`.
    status: String,
    /// `unverified` on capture; never taken from a request.
    verification: String,
    #[schema(required = true)]
    confidence: Option<String>,
    captured_at: DateTime<Utc>,
    #[schema(required = true, value_type = Option<ActorRef>)]
    captured_by: Option<Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DemandRow> for BuyerDemandResource {
    fn from(d: DemandRow) -> Self {
        let price = match (
            d.indicative_price_per_kg_minor,
            d.currency_id,
            d.currency,
            d.decimals,
        ) {
            (Some(minor), Some(currency_id), Some(currency), Some(decimals)) => Some(Money {
                minor,
                currency,
                decimals,
                currency_id,
            }),
            _ => None,
        };
        Self {
            id: d.id,
            buyer: MarketBuyerRef {
                id: d.buyer_id,
                name: d.buyer_name,
            },
            buyer_channel: d.buyer_channel,
            crop: MarketCropRef {
                id: d.crop_id,
                code: d.crop_code,
                name_en: d.crop_name_en,
                name_sw: d.crop_name_sw,
            },
            quantity_kg: kg(d.quantity_kg),
            quality_note: d.quality_note,
            window_start: d.window_start,
            window_end: d.window_end,
            delivery_point: d.delivery_point,
            indicative_price_per_kg: price,
            status: d.status,
            verification: d.verification,
            confidence: d.confidence,
            captured_at: d.captured_at,
            captured_by: d.captured_by,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

/// POST /farm/buyer-demands.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "New demand (ops or admin). Born open and unverified, captured by the caller; \
    status, verification and capture are never read from the body. A client `id` makes \
    retries safe."
)]
pub struct BuyerDemandRequest {
    #[schema(value_type = Option<Uuid>)]
    id: Option<String>,
    #[schema(value_type = Uuid)]
    buyer_id: Option<String>,
    #[schema(value_type = Uuid)]
    crop_id: Option<String>,
    /// kg > 0, at most 2 decimals.
    #[schema(value_type = String, example = "5000")]
    quantity_kg: Option<Value>,
    #[schema(value_type = Option<String>, max_length = 500)]
    quality_note: Option<String>,
    #[schema(value_type = String, format = Date)]
    window_start: Option<String>,
    /// On or after `window_start`.
    #[schema(value_type = String, format = Date)]
    window_end: Option<String>,
    #[schema(value_type = Option<String>, max_length = 200)]
    delivery_point: Option<String>,
    /// Minor units of `currency_id` per kg; needs `currency_id`.
    #[schema(value_type = Option<i64>)]
    indicative_price_per_kg_minor: Option<Value>,
    #[schema(value_type = Option<Uuid>)]
    currency_id: Option<String>,
    /// `low`, `medium` or `high`.
    #[schema(value_type = Option<String>)]
    confidence: Option<String>,
}

impl Validate for BuyerDemandRequest {
    type Valid = NewDemand;

    fn validate(self, r: &mut Rules) -> Option<NewDemand> {
        let id = uuid(r, "id", self.id);
        let buyer_id = required_uuid(r, "buyer_id", self.buyer_id);
        let crop_id = required_uuid(r, "crop_id", self.crop_id);
        let quantity_kg = match filled(self.quantity_kg.clone()) {
            None => {
                r.missing("quantity_kg");
                None
            }
            Some(_) => r.measure(
                "quantity_kg",
                self.quantity_kg,
                Decimal::ZERO,
                Decimal::new(999_999_999_999, 2),
                2,
            ),
        };
        r.check(
            "quantity_kg",
            quantity_kg != Some(Decimal::ZERO),
            "The quantity kg must be greater than 0.",
        );
        let window_start = date(r, "window_start", self.window_start);
        let window_end = date(r, "window_end", self.window_end);
        if let (Some(s), Some(e)) = (window_start, window_end) {
            r.check(
                "window_end",
                e >= s,
                "The window end must be a date after or equal to window start.",
            );
        }
        let price = filled(self.indicative_price_per_kg_minor)
            .and_then(|v| r.integer("indicative_price_per_kg_minor", &v, 0, i64::MAX));
        let currency_id = uuid(r, "currency_id", self.currency_id);
        if price.is_some() && currency_id.is_none() {
            r.add(
                "currency_id",
                "The currency id field is required when a price is given.",
            );
        }
        let confidence = r.one_of(
            "confidence",
            self.confidence,
            &["low", "medium", "high"],
            None,
        );
        Some(NewDemand {
            id,
            buyer_id: buyer_id?,
            crop_id: crop_id?,
            quantity_kg: quantity_kg?,
            quality_note: r.opt_text("quality_note", self.quality_note, 500),
            window_start: window_start?,
            window_end: window_end?,
            delivery_point: r.opt_text("delivery_point", self.delivery_point, 200),
            indicative_price_per_kg_minor: price,
            currency_id,
            confidence,
        })
    }
}

// ---- opportunities ----

/// The demand an opportunity answers.
#[derive(Serialize, ToSchema)]
pub struct OpportunityDemandRef {
    id: Uuid,
    buyer: MarketBuyerRef,
    quantity_kg: String,
    window_start: NaiveDate,
    window_end: NaiveDate,
}

/// A village named on an opportunity.
#[derive(Serialize, ToSchema)]
pub struct MarketVillageRef {
    id: Uuid,
    name: String,
}

/// A village that CAN supply a demand. Never a sale, delivery, payment or contract.
#[derive(Serialize, ToSchema)]
pub struct OpportunityResource {
    id: Uuid,
    demand: OpportunityDemandRef,
    village: MarketVillageRef,
    crop: MarketCropRef,
    /// The sum of its supply lines; only the server writes it.
    #[schema(example = "0.00")]
    offered_quantity_kg: String,
    /// `proposed`, `shared`, `accepted`, `declined` or `lapsed`.
    status: String,
    #[schema(required = true)]
    note: Option<String>,
    captured_at: DateTime<Utc>,
    #[schema(required = true, value_type = Option<ActorRef>)]
    captured_by: Option<Value>,
    /// Supply lines the caller may read: all of them for staff of the village, otherwise only
    /// lines from the caller's own farms.
    supply: Vec<SupplyLineResource>,
    /// Kg from the caller's own farms; null for an account with no person record.
    #[schema(required = true, example = "2000.00")]
    my_contribution_kg: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<Opportunity> for OpportunityResource {
    fn from((o, lines): Opportunity) -> Self {
        Self {
            id: o.id,
            demand: OpportunityDemandRef {
                id: o.buyer_demand_id,
                buyer: MarketBuyerRef {
                    id: o.buyer_id,
                    name: o.buyer_name,
                },
                quantity_kg: kg(o.demand_quantity_kg),
                window_start: o.window_start,
                window_end: o.window_end,
            },
            village: MarketVillageRef {
                id: o.village_id,
                name: o.village_name,
            },
            crop: MarketCropRef {
                id: o.crop_id,
                code: o.crop_code,
                name_en: o.crop_name_en,
                name_sw: o.crop_name_sw,
            },
            offered_quantity_kg: kg(o.offered_quantity_kg),
            status: o.status,
            note: o.note,
            captured_at: o.captured_at,
            captured_by: o.captured_by,
            supply: lines.into_iter().map(Into::into).collect(),
            my_contribution_kg: o.my_contribution_kg.map(kg),
            created_at: o.created_at,
            updated_at: o.updated_at,
        }
    }
}

/// A validated proposal. `status` and `crop_id` are only checked against RUAHA's rules.
pub struct NewOpportunity {
    pub id: Option<Uuid>,
    pub village_id: Uuid,
    pub crop_id: Option<Uuid>,
    pub note: Option<String>,
    pub status: Option<String>,
}

/// POST /farm/buyer-demands/{demand}/opportunities.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Propose a village against a demand (ops or admin). Born `proposed` with the \
    demand's crop and nothing offered; the offered total is the server's. A client `id` makes \
    retries safe."
)]
pub struct OpportunityRequest {
    #[schema(value_type = Option<Uuid>)]
    id: Option<String>,
    #[schema(value_type = Uuid)]
    village_id: Option<String>,
    /// If sent, must be the demand's crop.
    #[schema(value_type = Option<Uuid>)]
    crop_id: Option<String>,
    #[schema(value_type = Option<String>, max_length = 1000)]
    note: Option<String>,
    /// If sent, must be `proposed`.
    #[schema(value_type = Option<String>)]
    status: Option<String>,
}

impl Validate for OpportunityRequest {
    type Valid = NewOpportunity;

    fn validate(self, r: &mut Rules) -> Option<NewOpportunity> {
        let id = uuid(r, "id", self.id);
        let village_id = required_uuid(r, "village_id", self.village_id);
        let crop_id = uuid(r, "crop_id", self.crop_id);
        let note = r.opt_text("note", self.note, NOTE_MAX);
        Some(NewOpportunity {
            id,
            village_id: village_id?,
            crop_id,
            note,
            status: optional(self.status),
        })
    }
}

/// POST /farm/opportunities/{opportunity}/status.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "proposed → shared|declined|lapsed, shared → accepted|declined|lapsed, \
    accepted → declined|lapsed. Declined and lapsed are terminal."
)]
pub struct OpportunityStatusRequest {
    #[schema(value_type = String, example = "shared")]
    status: Option<String>,
}

impl Validate for OpportunityStatusRequest {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        if optional(self.status.clone()).is_none() {
            r.missing("status");
            return None;
        }
        r.one_of("status", self.status, &STATUSES, None)
    }
}

// ---- supply lines ----

/// The farmer behind a harvest: the farm's primary manager.
#[derive(Serialize, ToSchema)]
pub struct MarketFarmerRef {
    person_id: Uuid,
    name: String,
}

fn farmer(id: Option<Uuid>, name: Option<String>) -> Option<MarketFarmerRef> {
    id.map(|person_id| MarketFarmerRef {
        person_id,
        name: name.unwrap_or_default(),
    })
}

/// Harvest kg promised to an opportunity. Never edited or detached.
#[derive(Serialize, ToSchema)]
pub struct SupplyLineResource {
    id: Uuid,
    harvest_report_id: Uuid,
    crop_cycle_id: Uuid,
    plot_id: Uuid,
    plot_label: String,
    crop_id: Uuid,
    #[schema(required = true)]
    farmer: Option<MarketFarmerRef>,
    #[schema(example = "2000.00")]
    contributed_kg: String,
    /// Who attached it; null unless the caller is staff of the opportunity's village.
    #[schema(required = true, value_type = Option<ActorRef>)]
    captured_by: Option<Value>,
    captured_at: DateTime<Utc>,
}

impl From<LineRow> for SupplyLineResource {
    fn from(l: LineRow) -> Self {
        Self {
            id: l.id,
            harvest_report_id: l.harvest_report_id,
            crop_cycle_id: l.crop_cycle_id,
            plot_id: l.plot_id,
            plot_label: l.plot_label,
            crop_id: l.crop_id,
            farmer: farmer(l.farmer_person_id, l.farmer_name),
            contributed_kg: kg(l.contributed_kg),
            captured_by: l.captured_by,
            captured_at: l.captured_at,
        }
    }
}

/// A validated line.
pub struct NewLine {
    pub id: Option<Uuid>,
    pub harvest_report_id: Uuid,
    pub contributed_kg: Decimal,
}

/// POST /farm/opportunities/{opportunity}/supply.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Attach a current expected harvest figure of the opportunity's village and \
    crop (ops or admin). The attacher is the caller. A client `id` makes retries safe."
)]
pub struct SupplyLineRequest {
    #[schema(value_type = Option<Uuid>)]
    id: Option<String>,
    #[schema(value_type = Uuid)]
    harvest_report_id: Option<String>,
    /// kg > 0, at most 2 decimals.
    #[schema(value_type = String, example = "2000")]
    contributed_kg: Option<Value>,
}

impl Validate for SupplyLineRequest {
    type Valid = NewLine;

    fn validate(self, r: &mut Rules) -> Option<NewLine> {
        let id = uuid(r, "id", self.id);
        let harvest = required_uuid(r, "harvest_report_id", self.harvest_report_id);
        let kg = match filled(self.contributed_kg.clone()) {
            None => {
                r.missing("contributed_kg");
                None
            }
            Some(_) => r.measure(
                "contributed_kg",
                self.contributed_kg,
                Decimal::ZERO,
                Decimal::new(999_999_999_999, 2),
                2,
            ),
        };
        r.check(
            "contributed_kg",
            kg != Some(Decimal::ZERO),
            "The contributed kg must be greater than 0.",
        );
        Some(NewLine {
            id,
            harvest_report_id: harvest?,
            contributed_kg: kg?,
        })
    }
}

// ---- availability, village supply, matches ----

/// GET /farm/harvest-available query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AvailableQuery {
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    #[param(value_type = Option<Uuid>)]
    pub village_id: Option<String>,
    #[param(value_type = Option<Uuid>)]
    pub crop_id: Option<String>,
}

impl AvailableQuery {
    /// (village, crop); a malformed id is a 422.
    pub fn filter(&self) -> Result<(Option<Uuid>, Option<Uuid>), AppError> {
        let id = |field: &str, v: &Option<String>| {
            optional(v.clone())
                .map(|v| Uuid::parse_str(&v))
                .transpose()
                .map_err(|_| {
                    let f = field.replace('_', " ");
                    AppError::field(field, format!("The {f} must be a valid UUID."))
                })
        };
        Ok((
            id("village_id", &self.village_id)?,
            id("crop_id", &self.crop_id)?,
        ))
    }
}

/// A current expected figure and what is already promised from it.
#[derive(Serialize, ToSchema)]
pub struct HarvestAvailableResource {
    harvest_report_id: Uuid,
    crop_cycle_id: Uuid,
    village_id: Uuid,
    crop_id: Uuid,
    plot_id: Uuid,
    plot_label: String,
    #[schema(required = true)]
    farmer: Option<MarketFarmerRef>,
    #[schema(required = true)]
    harvest_start: Option<NaiveDate>,
    #[schema(required = true)]
    harvest_end: Option<NaiveDate>,
    #[schema(example = "3200.00")]
    quantity_kg: String,
    #[schema(required = true)]
    confidence: Option<String>,
    verification: String,
    committed_kg: String,
    available_kg: String,
}

impl From<AvailableRow> for HarvestAvailableResource {
    fn from(a: AvailableRow) -> Self {
        Self {
            harvest_report_id: a.harvest_report_id,
            crop_cycle_id: a.crop_cycle_id,
            village_id: a.village_id,
            crop_id: a.crop_id,
            plot_id: a.plot_id,
            plot_label: a.plot_label,
            farmer: farmer(a.farmer_person_id, a.farmer_name),
            harvest_start: a.harvest_start,
            harvest_end: a.harvest_end,
            quantity_kg: kg(a.quantity_kg),
            confidence: a.confidence,
            verification: a.verification,
            committed_kg: kg(a.committed_kg),
            available_kg: kg(a.available_kg),
        }
    }
}

/// Expected, committed and available kg of one crop in one harvest month.
#[derive(Serialize, ToSchema)]
pub struct VillageSupplyResource {
    crop_id: Uuid,
    /// First day of the harvest month; null for cycles without a harvest window.
    #[schema(required = true)]
    window_month: Option<NaiveDate>,
    #[schema(required = true)]
    window_start: Option<NaiveDate>,
    #[schema(required = true)]
    window_end: Option<NaiveDate>,
    expected_kg: String,
    committed_kg: String,
    available_kg: String,
    cycle_count: i64,
    verified_cycles: i64,
}

impl From<SupplyRow> for VillageSupplyResource {
    fn from(r: SupplyRow) -> Self {
        Self {
            crop_id: r.crop_id,
            window_month: r.window_month,
            window_start: r.window_start,
            window_end: r.window_end,
            expected_kg: kg(r.expected_kg),
            committed_kg: kg(r.committed_kg),
            available_kg: kg(r.available_kg),
            cycle_count: r.cycle_count,
            verified_cycles: r.verified_cycles,
        }
    }
}

/// The opportunity a match already has.
#[derive(Serialize, ToSchema)]
pub struct MatchOpportunityRef {
    id: Uuid,
    status: String,
}

/// A comparison, not a match: what one village could cover of one open demand. No allocation,
/// ranking or assignment.
#[derive(Serialize, ToSchema)]
pub struct DemandMatchResource {
    buyer_demand_id: Uuid,
    buyer: MarketBuyerRef,
    /// Same as `buyer.name`, as the Tower lists it.
    buyer_name: String,
    crop_id: Uuid,
    demand_kg: String,
    window_start: NaiveDate,
    window_end: NaiveDate,
    village: MarketVillageRef,
    available_kg: String,
    committed_kg: String,
    coverable_kg: String,
    /// Percent of the demand, 1 decimal.
    #[schema(example = "24.0")]
    coverage_pct: String,
    #[schema(required = true)]
    opportunity: Option<MatchOpportunityRef>,
}

impl From<MatchRow> for DemandMatchResource {
    fn from(m: MatchRow) -> Self {
        Self {
            buyer_demand_id: m.buyer_demand_id,
            buyer: MarketBuyerRef {
                id: m.buyer_id,
                name: m.buyer_name.clone(),
            },
            buyer_name: m.buyer_name,
            crop_id: m.crop_id,
            demand_kg: kg(m.demand_kg),
            window_start: m.window_start,
            window_end: m.window_end,
            village: MarketVillageRef {
                id: m.village_id,
                name: m.village_name,
            },
            available_kg: kg(m.available_kg),
            committed_kg: kg(m.committed_kg),
            coverable_kg: kg(m.coverable_kg),
            coverage_pct: format!("{:.1}", m.coverage_pct),
            opportunity: m
                .opportunity_id
                .zip(m.opportunity_status)
                .map(|(id, status)| MatchOpportunityRef { id, status }),
        }
    }
}
