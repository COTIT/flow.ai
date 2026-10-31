//! Deal bodies. Keys follow Laravel DealResource / DealIndexResource and the Store / Update /
//! UpdateDealStatus / AddRelationToDeal request rules (vimdesk-web deal.service.js,
//! pipeline.store.js).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::{
        json::{Rules, Validate, filled, human, present},
        list::Page,
    },
    modules::{
        currencies::dto::CurrencyResource, deals::repo::DealRow, users::dto::UserEmailResource,
    },
};

const TITLE_MAX: usize = 255;
const DESCRIPTION_MAX: usize = 50_000;
/// Laravel DealPriority values.
const PRIORITIES: [&str; 3] = ["high", "low", "medium"];
/// Laravel's board column cap (`paginate(999)`).
pub const COLUMN_PAGE: i64 = 999;
/// Highest drop index: past any board page, small enough that a drag never walks a whole
/// column under its lock (Laravel took any integer).
const INDEX_MAX: i64 = 10_000;
/// Laravel rules.DealPipelineExist for `deal_pipeline_id`.
pub const PIPELINE_MISSING: &str = "The deal pipeline id does not exist";
/// Laravel rules.ValidateDealStatus.
pub const STATUS_NOT_IN_PIPELINE: &str = "This status is not one of the deal pipeline statuses";
/// Laravel rules.RelationExist.
pub const RELATION_MISSING: &str = "This relation does not exist";
/// Laravel CurrencyExist for `currency_id`.
pub const CURRENCY_MISSING: &str = "This currency id does not exist";
/// Laravel `currency.response.no_default_currency`.
pub const NO_MAIN_CURRENCY: &str = "No default currency has been found. Please create one";
/// Laravel rules.UserExist for `sales_agent_id`.
pub const AGENT_MISSING: &str = "This sales agent id is not a valid user_id";
/// Laravel `after:yesterday`.
pub const CLOSE_DATE_PAST: &str = "The expected close date must be a date after yesterday.";

// Laravel DealResource (`full`) or DealIndexResource (board columns): same keys, the index
// form leaves out the pipeline, its column and the custom sections.
/// A deal.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A deal. Board columns (and lists) leave out `deal_pipeline`, `deal_pipeline_status` and `custom_sections` under `relationships`."
)]
pub struct DealResource {
    id: Uuid,
    title: String,
    #[schema(required = true)]
    description: Option<String>,
    date: NaiveDate,
    // Raw `decimal(…,8)` text, as Laravel left the column uncast.
    /// Decimal text with 8 places, e.g. `"1250.50000000"`.
    #[schema(example = "1250.50000000")]
    deal_value: String,
    /// `high`, `medium` or `low`.
    deal_priority: String,
    #[schema(required = true)]
    expected_close_date: Option<NaiveDate>,
    position: i64,
    // Not ported (archiving was 403 for everyone in Laravel); always false.
    /// Always false (archiving is not supported).
    archived: bool,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    relationships: Relationships,
}

/// Deal `relationships`; null when the linked row is missing.
#[derive(Serialize, ToSchema)]
struct Relationships {
    #[schema(value_type = Option<UserEmailResource>, required = true)]
    sales_agent: Value,
    /// The relation (customer) list resource.
    #[schema(value_type = Option<Object>, required = true)]
    relation: Value,
    #[schema(value_type = Option<CurrencyResource>, required = true)]
    currency: Value,
    /// Single deal only.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<crate::modules::deal_pipelines::dto::PipelineResource>)]
    deal_pipeline: Option<Value>,
    /// Single deal only.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<crate::modules::deal_pipelines::dto::StatusResource>)]
    deal_pipeline_status: Option<Value>,
    /// Single deal only; always empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<Object>>, max_items = 0)]
    custom_sections: Option<[(); 0]>,
}

/// Embedded resources of one deal, already in their resource shape (null when missing).
pub struct Embeds {
    pub sales_agent: Value,
    pub relation: Value,
    pub currency: Value,
    /// Pipeline and column resources; None for the index form.
    pub board: Option<(Value, Value)>,
}

impl DealResource {
    /// One deal with its embeds.
    pub fn new(d: DealRow, e: Embeds) -> Self {
        let mut value = d.deal_value;
        value.rescale(8);
        let full = e.board.is_some();
        let (deal_pipeline, deal_pipeline_status) = e.board.unzip();
        Self {
            id: d.id,
            title: d.title,
            description: d.description,
            date: d.date.unwrap_or_else(|| d.created_at.date_naive()),
            deal_value: value.to_string(),
            deal_priority: d.deal_priority,
            expected_close_date: d.expected_close_date,
            position: d.position,
            archived: false,
            created_at: d.created_at,
            last_updated_at: d.updated_at,
            meta: d.meta,
            relationships: Relationships {
                sales_agent: e.sales_agent,
                relation: e.relation,
                currency: e.currency,
                deal_pipeline,
                deal_pipeline_status,
                custom_sections: full.then_some([]),
            },
        }
    }
}

/// Create response.
#[derive(Serialize, ToSchema)]
pub struct DealCreated {
    pub deal_id: Uuid,
}

// GET /deals query. `pipeline_id` and `status` are the list view's board filters (Laravel
// ignored them); unknown params ignored.
/// GET /deals query; unknown params ignored.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search text.
    pub q: Option<String>,
    /// Only deals of this relation.
    #[param(value_type = Option<Uuid>)]
    pub relation_id: Option<String>,
    /// Only deals on this pipeline.
    #[param(value_type = Option<Uuid>)]
    pub pipeline_id: Option<String>,
    /// Only deals in this column (status id).
    #[param(value_type = Option<Uuid>)]
    pub status: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    page_size: Option<String>,
}

impl ListQuery {
    /// Laravel BaseModel paging: `page_size` default 50, max 100.
    pub fn page(&self) -> Page {
        Page::parse(self.page.as_deref(), self.page_size.as_deref())
    }

    /// `relation_id` (Laravel `nullable|uuid`), `pipeline_id` and `status` filters; a
    /// malformed one is a 422 rather than a silently unfiltered list.
    pub fn filters(&self) -> Result<[Option<Uuid>; 3], crate::error::AppError> {
        let mut r = Rules::default();
        let mut id = |field: &str, v: &Option<String>| {
            let raw = v.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
            uuid(&mut r, field, &Value::String(raw.to_owned()))
        };
        let ids = [
            id("relation_id", &self.relation_id),
            id("pipeline_id", &self.pipeline_id),
            id("status", &self.status),
        ];
        r.into_result()?;
        Ok(ids)
    }
}

// Board column query: Laravel `paginate(999)`, so only `page` moves.
/// Board column query: pages of 999.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ColumnQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    page: Option<String>,
}

impl ColumnQuery {
    /// Pages of 999.
    pub fn page(&self) -> Page {
        Page {
            number: Page::parse(self.page.as_deref(), None).number,
            size: COLUMN_PAGE,
        }
    }
}

/// POST /deals.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New deal. Without `currency_id` the workspace default currency is used.")]
pub struct CreateDealRequest {
    #[schema(value_type = Uuid)]
    deal_pipeline_id: Option<Value>,
    /// A column of `deal_pipeline_id`.
    #[schema(value_type = Uuid)]
    deal_pipeline_status_id: Option<Value>,
    #[schema(value_type = Option<Uuid>)]
    relation_id: Option<Value>,
    #[schema(value_type = Option<Uuid>)]
    currency_id: Option<Value>,
    /// A member of the workspace.
    #[schema(value_type = Uuid)]
    sales_agent_id: Option<Value>,
    #[schema(value_type = String, max_length = 255)]
    title: Option<String>,
    #[schema(value_type = String, max_length = 50000)]
    description: Option<String>,
    /// `YYYY-MM-DD`; defaults to today.
    #[schema(value_type = Option<NaiveDate>)]
    date: Option<Value>,
    /// Decimal; defaults to 0.
    #[schema(value_type = Option<String>, example = "1250.50")]
    deal_value: Option<Value>,
    /// `high`, `medium` or `low`.
    #[schema(value_type = String)]
    deal_priority: Option<String>,
    /// `YYYY-MM-DD`, today or later.
    #[schema(value_type = NaiveDate)]
    expected_close_date: Option<Value>,
    #[schema(value_type = Option<Object>)]
    meta: Option<Value>,
    /// Accepted and ignored.
    #[schema(value_type = Option<Vec<Object>>)]
    custom_sections: Option<Value>,
}

/// Validated create; ids still need their workspace checks.
pub struct NewDeal {
    pub pipeline: Uuid,
    pub status: Uuid,
    pub relation: Option<Uuid>,
    pub currency: Option<Uuid>,
    pub sales_agent: Uuid,
    pub title: String,
    pub description: String,
    pub date: Option<NaiveDate>,
    pub deal_value: Decimal,
    pub priority: String,
    pub expected_close_date: NaiveDate,
    pub meta: Option<Value>,
}

impl Validate for CreateDealRequest {
    type Valid = NewDeal;

    fn validate(self, r: &mut Rules) -> Option<NewDeal> {
        r.custom_sections("custom_sections", self.custom_sections.as_ref());
        let pipeline = r
            .required_value("deal_pipeline_id", self.deal_pipeline_id)
            .and_then(|v| id_or(r, "deal_pipeline_id", &v, PIPELINE_MISSING));
        let status = r
            .required_value("deal_pipeline_status_id", self.deal_pipeline_status_id)
            .and_then(|v| id_or(r, "deal_pipeline_status_id", &v, STATUS_NOT_IN_PIPELINE));
        let relation =
            filled(self.relation_id).map(|v| id_or(r, "relation_id", &v, RELATION_MISSING));
        let currency =
            filled(self.currency_id).map(|v| id_or(r, "currency_id", &v, CURRENCY_MISSING));
        let sales_agent = r
            .required_value("sales_agent_id", self.sales_agent_id)
            .and_then(|v| uuid(r, "sales_agent_id", &v));
        let title = r.text("title", self.title, TITLE_MAX);
        let description = r.text("description", self.description, DESCRIPTION_MAX);
        let date = filled(self.date).map(|v| ymd(r, "date", &v));
        let deal_value = filled(self.deal_value).map(|v| value(r, &v));
        let priority = r
            .required("deal_priority", self.deal_priority)
            .and_then(|p| priority(r, &p));
        let close = r
            .required_value("expected_close_date", self.expected_close_date)
            .and_then(|v| ymd(r, "expected_close_date", &v))
            .and_then(|d| not_past(r, d));
        let meta = r.meta(self.meta);
        Some(NewDeal {
            pipeline: pipeline?,
            status: status?,
            relation: relation.map_or(Some(None), |x| x.map(Some))?,
            currency: currency.map_or(Some(None), |x| x.map(Some))?,
            sales_agent: sales_agent?,
            title,
            description,
            date: date.map_or(Some(None), |x| x.map(Some))?,
            deal_value: deal_value.unwrap_or(Some(Decimal::ZERO))?,
            priority: priority?,
            expected_close_date: close?,
            meta: meta?,
        })
    }
}

/// POST /deals/{id}: a patch. The edit modal also sends the user's own id as `id` (ignored)
/// and the unchanged board and column.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Changes to a deal. Leave a field out to keep it.")]
pub struct UpdateDealRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 255)]
    title: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 50000)]
    description: Option<Option<String>>,
    /// Decimal.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, example = "1250.50")]
    deal_value: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<NaiveDate>)]
    date: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    sales_agent_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    deal_pipeline_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    deal_pipeline_status_id: Option<Option<Value>>,
    /// `high`, `medium` or `low`.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    deal_priority: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<NaiveDate>)]
    expected_close_date: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
    /// Accepted and ignored.
    #[schema(value_type = Option<Vec<Object>>)]
    custom_sections: Option<Value>,
}

/// Validated patch; `None` keeps the stored value.
#[derive(Default)]
pub struct DealPatch {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub deal_value: Option<Decimal>,
    pub date: Option<Option<NaiveDate>>,
    pub sales_agent: Option<Option<Uuid>>,
    pub pipeline: Option<Uuid>,
    pub status: Option<Uuid>,
    pub priority: Option<String>,
    /// Checked against today in the service: an unchanged stored date may be in the past.
    pub expected_close_date: Option<Option<NaiveDate>>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateDealRequest {
    type Valid = DealPatch;

    fn validate(self, r: &mut Rules) -> Option<DealPatch> {
        r.custom_sections("custom_sections", self.custom_sections.as_ref());
        // Null on a NOT NULL column is "required" (Laravel wrote NULL and answered 500).
        let mut required = |field: &str, v: Option<Option<Value>>| -> Option<Value> {
            match v.map(filled) {
                Some(None) => {
                    r.missing(field);
                    None
                }
                Some(Some(v)) => Some(v),
                None => None,
            }
        };
        let pipeline = required("deal_pipeline_id", self.deal_pipeline_id);
        let status = required("deal_pipeline_status_id", self.deal_pipeline_status_id);
        let deal_value = required("deal_value", self.deal_value);
        Some(DealPatch {
            title: r.patch_text("title", self.title, TITLE_MAX),
            description: r.patch_opt_text("description", self.description, DESCRIPTION_MAX),
            deal_value: deal_value.and_then(|v| value(r, &v)),
            date: self
                .date
                .map(filled)
                .map(|v| v.and_then(|v| ymd(r, "date", &v))),
            sales_agent: self
                .sales_agent_id
                .map(filled)
                .map(|v| v.and_then(|v| uuid(r, "sales_agent_id", &v))),
            pipeline: pipeline.and_then(|v| uuid(r, "deal_pipeline_id", &v)),
            status: status.and_then(|v| uuid(r, "deal_pipeline_status_id", &v)),
            priority: match self.deal_priority {
                Some(p) => r.required("deal_priority", p).and_then(|p| priority(r, &p)),
                None => None,
            },
            expected_close_date: self
                .expected_close_date
                .map(filled)
                .map(|v| v.and_then(|v| any_date(r, "expected_close_date", &v))),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}

/// POST /deals/{id}/update-status: the kanban drag. `position` is the 0-based index in the
/// target column.
#[derive(Deserialize, ToSchema)]
pub struct MoveDealRequest {
    #[schema(value_type = Uuid)]
    deal_pipeline_id: Option<Value>,
    /// Target column.
    #[schema(value_type = Uuid)]
    deal_pipeline_status_id: Option<Value>,
    /// 0-based index in the target column.
    #[schema(value_type = i64, minimum = 0, maximum = 10000)]
    position: Option<Value>,
}

/// Validated drag.
pub struct DealMove {
    pub pipeline: Uuid,
    pub status: Uuid,
    pub index: i64,
}

impl Validate for MoveDealRequest {
    type Valid = DealMove;

    fn validate(self, r: &mut Rules) -> Option<DealMove> {
        let pipeline = r
            .required_value("deal_pipeline_id", self.deal_pipeline_id)
            .and_then(|v| id_or(r, "deal_pipeline_id", &v, PIPELINE_MISSING));
        let status = r
            .required_value("deal_pipeline_status_id", self.deal_pipeline_status_id)
            .and_then(|v| id_or(r, "deal_pipeline_status_id", &v, STATUS_NOT_IN_PIPELINE));
        let index = r
            .required_value("position", self.position)
            .and_then(|v| r.integer("position", &v, 0, INDEX_MAX));
        Some(DealMove {
            pipeline: pipeline?,
            status: status?,
            index: index?,
        })
    }
}

/// POST /deals/{id}/add-relation.
#[derive(Deserialize, ToSchema)]
pub struct AddRelationRequest {
    #[schema(value_type = Uuid)]
    relation_id: Option<Value>,
}

impl Validate for AddRelationRequest {
    type Valid = Uuid;

    fn validate(self, r: &mut Rules) -> Option<Uuid> {
        r.required_value("relation_id", self.relation_id)
            .and_then(|v| uuid(r, "relation_id", &v))
    }
}

/// An id where Laravel's rule had no `uuid` check: a malformed one fails its exists text.
fn id_or(r: &mut Rules, field: &str, v: &Value, missing: &str) -> Option<Uuid> {
    let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
    if id.is_none() {
        r.add(field, missing);
    }
    id
}

/// Laravel `uuid`.
fn uuid(r: &mut Rules, field: &str, v: &Value) -> Option<Uuid> {
    let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
    if id.is_none() {
        r.add(field, format!("The {} must be a valid UUID.", human(field)));
    }
    id
}

/// Laravel `date_format:Y-m-d`.
fn ymd(r: &mut Rules, field: &str, v: &Value) -> Option<NaiveDate> {
    let d = v
        .as_str()
        .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok());
    if d.is_none() {
        let f = human(field);
        r.add(field, format!("The {f} does not match the format Y-m-d."));
    }
    d
}

/// Laravel `date` (update rule): `Y-m-d`, or a timestamp read as its UTC day.
fn any_date(r: &mut Rules, field: &str, v: &Value) -> Option<NaiveDate> {
    let s = v.as_str().map(str::trim).unwrap_or_default();
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|t| t.date())
        })
        .or_else(|| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|t| t.to_utc().date_naive())
        });
    if d.is_none() {
        r.add(field, format!("The {} is not a valid date.", human(field)));
    }
    d
}

/// Laravel `after:yesterday`: today (UTC, the app timezone) or later.
pub fn not_past(r: &mut Rules, d: NaiveDate) -> Option<NaiveDate> {
    if d < Utc::now().date_naive() {
        r.add("expected_close_date", CLOSE_DATE_PAST);
        return None;
    }
    Some(d)
}

/// Laravel `numeric` on a `decimal(…,8)`; any sign.
fn value(r: &mut Rules, v: &Value) -> Option<Decimal> {
    r.decimal("deal_value", v, -crate::money::MAX, crate::money::MAX)
}

/// Laravel EnumExist(DealPriority): any case, stored lowercase.
fn priority(r: &mut Rules, p: &str) -> Option<String> {
    let lower = p.to_ascii_lowercase();
    if PRIORITIES.contains(&lower.as_str()) {
        return Some(lower);
    }
    r.add(
        "deal_priority",
        format!("The selected deal priority: {p} is invalid."),
    );
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn priority_any_case_stored_lowercase() {
        let mut r = Rules::default();
        assert_eq!(priority(&mut r, "High").as_deref(), Some("high"));
        assert_eq!(priority(&mut r, "urgent"), None);
        assert!(r.into_result().is_err());
    }

    #[test]
    fn dates_parse_like_laravel() {
        let mut r = Rules::default();
        assert!(ymd(&mut r, "date", &json!("2026-10-23")).is_some());
        assert!(ymd(&mut r, "date", &json!("23-10-2026")).is_none());
        let mut r = Rules::default();
        assert_eq!(
            any_date(
                &mut r,
                "expected_close_date",
                &json!("2026-10-23T22:30:00+02:00")
            ),
            NaiveDate::from_ymd_opt(2026, 10, 23)
        );
        assert!(any_date(&mut r, "expected_close_date", &json!("soon")).is_none());
    }
}
