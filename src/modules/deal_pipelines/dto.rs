//! Deal pipeline and status bodies. Keys follow Laravel DealPipelineResource /
//! DealPipelineStatusResource and the Store/Update request rules (vimdesk-web
//! deal-pipeline.service.js, deal-pipeline-status.service.js, settings/pipelines).

use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    http::{
        json::{Rules, Validate, filled, human, present},
        list::Page,
    },
    modules::{
        custom_statuses::dto::CustomStatusResource,
        deal_pipelines::repo::{PipelineRow, StatusRow},
        deals::repo::SummaryRow,
    },
};

const TEXT_MAX: usize = 255;
/// Highest column position a client may ask for. Shifts may push columns past it, never near
/// `i32::MAX` (Laravel took any integer).
const POSITION_MAX: i64 = 10_000;
/// Laravel SortingEnum values.
const SORTINGS: [&str; 3] = ["a-z", "pos", "z-a"];
/// Laravel rules.CustomStatusExist for `custom_status_id`.
pub const STATUS_MISSING: &str = "The custom status id does not exist";
/// Laravel rules.DealPipelineStatusExist for `custom_status_id`.
pub const STATUS_IN_PIPELINE: &str = "This custom status id already exist in the dealPipeline";
/// Laravel rules.properties_not_valid_value_exist.
pub const FLAG_TAKEN: &str =
    "You already have a Deal Pipeline status with the same properties values";
/// Laravel rules.properties_not_valid_both_true.
const BOTH_FLAGS: &str = "Properties field is not valid, can not have is_won and is_lost field both checked at the same time";

/// Laravel DealPipelineResource. `summary` counts won/lost deals (zero until deals exist).
#[derive(Serialize)]
pub struct PipelineResource {
    id: Uuid,
    name: String,
    #[serde(rename = "type")]
    kind: Option<String>,
    sorting: String,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    meta: Option<Value>,
    summary: Value,
    relationships: PipelineRelationships,
}

#[derive(Serialize)]
struct PipelineRelationships {
    statuses: Vec<StatusResource>,
}

impl PipelineResource {
    /// One board with its columns (in position order) and its won/lost totals.
    pub fn new(p: PipelineRow, statuses: Vec<StatusRow>, totals: &[&SummaryRow]) -> Self {
        Self {
            id: p.id,
            name: p.name,
            kind: p.kind,
            sorting: p.sorting,
            created_at: p.created_at,
            last_updated_at: p.updated_at,
            meta: p.meta,
            summary: summary(totals),
            relationships: PipelineRelationships {
                statuses: statuses.into_iter().map(Into::into).collect(),
            },
        }
    }
}

/// Laravel SummarizeDealsForDealPipeline: per side, the deal count (`total_amount`) and the
/// value sum per currency code, as PHP floats; an empty side prints `total_values` as [].
fn summary(totals: &[&SummaryRow]) -> Value {
    let side = |won: bool| {
        let mut values = Map::new();
        let mut count = 0;
        for t in totals.iter().filter(|t| t.is_won == won) {
            count += t.deals;
            values.insert(t.asset_type.clone(), json!(t.total.to_f64().unwrap_or(0.0)));
        }
        let values = if values.is_empty() {
            json!([])
        } else {
            Value::Object(values)
        };
        json!({ "total_amount": count, "total_values": values })
    };
    json!({ "won": side(true), "lost": side(false) })
}

/// Laravel DealPipelineStatusResource: `rules` and `properties` as their value objects printed
/// them; the custom status is null once it is deleted.
#[derive(Serialize)]
pub struct StatusResource {
    id: Uuid,
    position: i32,
    rules: LaneRules,
    properties: LaneProps,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    meta: Option<Value>,
    relationships: StatusRelationships,
}

#[derive(Serialize)]
struct StatusRelationships {
    custom_status: Option<CustomStatusResource>,
}

impl From<StatusRow> for StatusResource {
    fn from(s: StatusRow) -> Self {
        Self {
            id: s.id,
            position: s.position,
            rules: s.rules,
            properties: s.props,
            created_at: s.created_at,
            last_updated_at: s.updated_at,
            meta: s.meta,
            relationships: StatusRelationships {
                custom_status: s.custom_status.map(Into::into),
            },
        }
    }
}

/// Pipeline create response.
#[derive(Serialize)]
pub struct PipelineCreated {
    pub deal_pipeline_id: Uuid,
}

/// Status create response.
#[derive(Serialize)]
pub struct StatusCreated {
    pub deal_pipeline_status_id: Uuid,
}

/// Laravel StatusRules: lane limits, stored and shown, enforced nowhere (as in Laravel).
#[derive(Serialize, Clone, Copy)]
pub struct LaneRules {
    pub max_items_per_lane: i32,
    pub max_days_in_lane: i32,
    pub auto_archive_in_days: i32,
}

/// Laravel StatusProperties: lane colour, probability text and the won/lost flags.
#[derive(Serialize, Clone)]
pub struct LaneProps {
    pub lane_color: String,
    pub deal_probability: String,
    pub is_won: bool,
    pub is_lost: bool,
}

/// POST /deal-pipelines. Laravel's store rules had no `type`, so it was dropped; genesy keeps it.
#[derive(Deserialize)]
pub struct CreatePipelineRequest {
    name: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    sorting: Option<String>,
    meta: Option<Value>,
}

/// Validated pipeline create.
pub struct NewPipeline {
    pub name: String,
    pub kind: Option<String>,
    pub sorting: String,
    pub meta: Option<Value>,
}

impl Validate for CreatePipelineRequest {
    type Valid = NewPipeline;

    fn validate(self, r: &mut Rules) -> Option<NewPipeline> {
        let sorting = r.one_of("sorting", self.sorting, &SORTINGS, None);
        Some(NewPipeline {
            name: r.text("name", self.name, TEXT_MAX),
            kind: r.opt_text("type", self.kind, TEXT_MAX),
            sorting: sorting.unwrap_or_else(|| "a-z".to_owned()),
            meta: r.meta(self.meta).flatten(),
        })
    }
}

/// POST /deal-pipelines/{id}: a patch.
#[derive(Deserialize)]
pub struct UpdatePipelineRequest {
    #[serde(default, deserialize_with = "present")]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present", rename = "type")]
    kind: Option<Option<String>>,
    sorting: Option<String>,
    #[serde(default, deserialize_with = "present")]
    meta: Option<Option<Value>>,
}

/// GET list paging params; unknown ones ignored.
#[derive(Deserialize)]
pub struct ListQuery {
    page: Option<String>,
    page_size: Option<String>,
}

impl ListQuery {
    /// Laravel BaseModel paging: `page_size` default 50, max 100.
    pub fn page(&self) -> Page {
        Page::parse(self.page.as_deref(), self.page_size.as_deref())
    }
}

/// Validated pipeline patch; `None` keeps the stored value.
pub struct PipelinePatch {
    pub name: Option<String>,
    pub kind: Option<Option<String>>,
    pub sorting: Option<String>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdatePipelineRequest {
    type Valid = PipelinePatch;

    fn validate(self, r: &mut Rules) -> Option<PipelinePatch> {
        Some(PipelinePatch {
            name: r.patch_text("name", self.name, TEXT_MAX),
            kind: r.patch_opt_text("type", self.kind, TEXT_MAX),
            // Blank or null sorting keeps the stored one (the column has a default).
            sorting: r.one_of("sorting", self.sorting, &SORTINGS, None),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}

/// POST /deal-pipelines/{id}/status. `deal_pipeline_id` is required as in Laravel, but the
/// path names the board.
#[derive(Deserialize)]
pub struct CreateStatusRequest {
    deal_pipeline_id: Option<Value>,
    custom_status_id: Option<Value>,
    position: Option<Value>,
    rules: Option<Value>,
    properties: Option<Value>,
    meta: Option<Value>,
}

/// Validated status create.
pub struct NewStatus {
    pub custom_status_id: Uuid,
    pub position: i32,
    pub rules: LaneRules,
    pub props: LaneProps,
    pub meta: Option<Value>,
}

impl Validate for CreateStatusRequest {
    type Valid = NewStatus;

    fn validate(self, r: &mut Rules) -> Option<NewStatus> {
        pipeline_id(r, self.deal_pipeline_id);
        let custom_status_id = r
            .required_value("custom_status_id", self.custom_status_id)
            .and_then(|v| {
                let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
                if id.is_none() {
                    r.add("custom_status_id", STATUS_MISSING);
                }
                id
            });
        let position = r
            .required_value("position", self.position)
            .and_then(|v| position(r, &v));
        let rules = r
            .required_value("rules", self.rules)
            .and_then(|v| lane_rules(r, &v));
        let props = r
            .required_value("properties", self.properties)
            .and_then(|v| lane_props(r, &v));
        let meta = r.meta(self.meta);
        Some(NewStatus {
            custom_status_id: custom_status_id?,
            position: position?,
            rules: rules?,
            props: props?,
            meta: meta?,
        })
    }
}

/// POST /deal-pipelines/{p}/status/{s}. The edit modal posts the whole board column back
/// (`id`, `title`, `tasks`, `custom_status_id`…): unknown keys are ignored and the custom
/// status never changes. The reorder call sends `position` alone.
#[derive(Deserialize)]
pub struct UpdateStatusRequest {
    deal_pipeline_id: Option<Value>,
    position: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    rules: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    properties: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    meta: Option<Option<Value>>,
}

/// Validated status patch; `None` keeps the stored value.
pub struct StatusPatch {
    pub position: Option<i32>,
    pub rules: Option<LaneRules>,
    pub props: Option<LaneProps>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateStatusRequest {
    type Valid = StatusPatch;

    fn validate(self, r: &mut Rules) -> Option<StatusPatch> {
        pipeline_id(r, self.deal_pipeline_id);
        // Laravel `nullable|integer`: null or blank keeps the position (Laravel answered 500).
        // Laravel ran its value-object rules on null and blank too, so those are 422s here.
        Some(StatusPatch {
            position: filled(self.position).and_then(|v| position(r, &v)),
            rules: self
                .rules
                .and_then(|v| lane_rules(r, &v.unwrap_or(Value::Null))),
            props: self
                .properties
                .and_then(|v| lane_props(r, &v.unwrap_or(Value::Null))),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}

/// Laravel `deal_pipeline_id` `required|uuid`. Checked only: the path picks the board.
fn pipeline_id(r: &mut Rules, v: Option<Value>) {
    let Some(v) = r.required_value("deal_pipeline_id", v) else {
        return;
    };
    if v.as_str()
        .and_then(|s| Uuid::parse_str(s.trim()).ok())
        .is_none()
    {
        let f = human("deal_pipeline_id");
        r.add("deal_pipeline_id", format!("The {f} must be a valid UUID."));
    }
}

/// Column position: an integer 0..=POSITION_MAX (the column is 0-based).
fn position(r: &mut Rules, v: &Value) -> Option<i32> {
    r.integer("position", v, 0, POSITION_MAX)
        .and_then(|n| i32::try_from(n).ok())
}

/// Laravel StatusRules::validate. Any failure reads as one message: Laravel looked for its
/// first error under key 0 of a keyed bag, missed, and fell back to rules.DealPipelineStatusRule.
fn lane_rules(r: &mut Rules, v: &Value) -> Option<LaneRules> {
    let mut scratch = Rules::default();
    let mut int = |key: &str| {
        let n = v.get(key).filter(|x| !x.is_null());
        n.and_then(|x| scratch.integer(key, x, i32::MIN.into(), i32::MAX.into()))
            .and_then(|n| i32::try_from(n).ok())
    };
    let parsed = (
        int("max_items_per_lane"),
        int("max_days_in_lane"),
        int("auto_archive_in_days"),
    );
    match parsed {
        (Some(max_items_per_lane), Some(max_days_in_lane), Some(auto_archive_in_days)) => {
            Some(LaneRules {
                max_items_per_lane,
                max_days_in_lane,
                auto_archive_in_days,
            })
        }
        _ => {
            r.add("rules", "The rules must be an available rules.");
            None
        }
    }
}

/// Laravel StatusProperties::validate plus the won-and-lost check. Same fallback message as
/// `lane_rules`; the won/lost-taken check needs the database and lives in the service.
fn lane_props(r: &mut Rules, v: &Value) -> Option<LaneProps> {
    let text = |key: &str| {
        v.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.chars().count() <= TEXT_MAX)
            .map(str::to_owned)
    };
    let mut scratch = Rules::default();
    let mut flag = |key: &str| {
        v.get(key)
            .filter(|x| !x.is_null())
            .and_then(|x| scratch.boolean(key, x))
    };
    let (won, lost) = (flag("is_won"), flag("is_lost"));
    let (Some(lane_color), Some(deal_probability), Some(is_won), Some(is_lost)) =
        (text("lane_color"), text("deal_probability"), won, lost)
    else {
        r.add(
            "properties",
            "The properties must be an available properties.",
        );
        return None;
    };
    if is_won && is_lost {
        r.add("properties", BOTH_FLAGS);
        return None;
    }
    Some(LaneProps {
        lane_color,
        deal_probability,
        is_won,
        is_lost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules_of(v: Value) -> Option<i32> {
        lane_rules(&mut Rules::default(), &v).map(|l| l.max_items_per_lane)
    }

    #[test]
    fn lane_rules_take_integers_and_digit_strings() {
        let ok =
            json!({ "max_items_per_lane": "7", "max_days_in_lane": 0, "auto_archive_in_days": -1 });
        assert_eq!(rules_of(ok), Some(7));
        for bad in [
            json!([1, 2, 3]),
            json!({ "max_items_per_lane": 1, "max_days_in_lane": 1, "auto_archive_in_days": "x" }),
            json!({ "max_items_per_lane": 2_147_483_648_i64, "max_days_in_lane": 1, "auto_archive_in_days": 1 }),
        ] {
            assert_eq!(rules_of(bad.clone()), None, "{bad}");
        }
    }

    #[test]
    fn lane_props_trim_and_reject_both_flags() {
        let mut r = Rules::default();
        let p = lane_props(
            &mut r,
            &json!({ "lane_color": " #fff ", "deal_probability": "10", "is_won": "1", "is_lost": 0 }),
        )
        .unwrap();
        assert_eq!(
            (p.lane_color.as_str(), p.is_won, p.is_lost),
            ("#fff", true, false)
        );

        let mut r = Rules::default();
        let both = json!({ "lane_color": "#fff", "deal_probability": "10", "is_won": true, "is_lost": true });
        assert!(lane_props(&mut r, &both).is_none());
        assert!(r.into_result().is_err());
    }
}
