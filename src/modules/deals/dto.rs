//! Deal bodies. Keys follow Laravel DealResource / DealIndexResource and the Store / Update /
//! UpdateDealStatus / AddRelationToDeal request rules (vimdesk-web deal.service.js,
//! pipeline.store.js).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::{
        json::{Rules, Validate, filled, human, present},
        list::Page,
    },
    modules::deals::repo::DealRow,
};

const TITLE_MAX: usize = 255;
const DESCRIPTION_MAX: usize = 50_000;
/// Laravel DealPriority values.
const PRIORITIES: [&str; 3] = ["high", "low", "medium"];
/// Laravel's board column cap (`paginate(999)`).
pub const COLUMN_PAGE: i64 = 999;
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

/// Laravel DealResource (`full`) or DealIndexResource (board columns): same keys, the index
/// form leaves out the pipeline, its column and the custom sections.
#[derive(Serialize)]
pub struct DealResource {
    id: Uuid,
    title: String,
    description: Option<String>,
    date: NaiveDate,
    /// Raw `decimal(…,8)` text, as Laravel left the column uncast.
    deal_value: String,
    deal_priority: String,
    expected_close_date: Option<NaiveDate>,
    position: i64,
    /// Not ported (archiving was 403 for everyone in Laravel); always false.
    archived: bool,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    meta: Option<Value>,
    relationships: Relationships,
}

#[derive(Serialize)]
struct Relationships {
    sales_agent: Value,
    relation: Value,
    currency: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    deal_pipeline: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deal_pipeline_status: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
#[derive(Serialize)]
pub struct DealCreated {
    pub deal_id: Uuid,
}

/// GET /deals query. `pipeline_id` and `status` are the list view's board filters (Laravel
/// ignored them); unknown params ignored.
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub relation_id: Option<String>,
    pub pipeline_id: Option<String>,
    pub status: Option<String>,
    page: Option<String>,
    page_size: Option<String>,
}

impl ListQuery {
    /// Laravel BaseModel paging: `page_size` default 50, max 100.
    pub fn page(&self) -> Page {
        Page::parse(self.page.as_deref(), self.page_size.as_deref())
    }

    /// Laravel `relation_id` `nullable|uuid`.
    pub fn relation(&self) -> Result<Option<Uuid>, crate::error::AppError> {
        let Some(raw) = self
            .relation_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return Ok(None);
        };
        Uuid::parse_str(raw).map(Some).map_err(|_| {
            crate::error::AppError::field("relation_id", "The relation id must be a valid UUID.")
        })
    }
}

/// Board column query: Laravel `paginate(999)`, so only `page` moves.
#[derive(Deserialize)]
pub struct ColumnQuery {
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
#[derive(Deserialize)]
pub struct CreateDealRequest {
    deal_pipeline_id: Option<Value>,
    deal_pipeline_status_id: Option<Value>,
    relation_id: Option<Value>,
    currency_id: Option<Value>,
    sales_agent_id: Option<Value>,
    title: Option<String>,
    description: Option<String>,
    date: Option<Value>,
    deal_value: Option<Value>,
    deal_priority: Option<String>,
    expected_close_date: Option<Value>,
    meta: Option<Value>,
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
#[derive(Deserialize)]
pub struct UpdateDealRequest {
    #[serde(default, deserialize_with = "present")]
    title: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    description: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    deal_value: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    date: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    sales_agent_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    deal_pipeline_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    deal_pipeline_status_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    deal_priority: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    expected_close_date: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    meta: Option<Option<Value>>,
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
#[derive(Deserialize)]
pub struct MoveDealRequest {
    deal_pipeline_id: Option<Value>,
    deal_pipeline_status_id: Option<Value>,
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
            .and_then(|v| r.integer("position", &v, 0, i32::MAX.into()));
        Some(DealMove {
            pipeline: pipeline?,
            status: status?,
            index: index?,
        })
    }
}

/// POST /deals/{id}/add-relation.
#[derive(Deserialize)]
pub struct AddRelationRequest {
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
