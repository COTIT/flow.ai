//! Pair bodies (contracts §5.2) and the config rules of §2.1.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    integrations::pairs::repo::PairRow,
};

/// (key, min, max, default) of the integer settings.
const NUMBERS: [(&str, i64, i64, i64); 5] = [
    ("poll_minutes", 1, 1440, 5),
    ("backfill_days", 0, 365, 0),
    ("full_scan_minutes", 15, 1440, 60),
    ("succeeded_watch_days", 1, 365, 30),
    ("tax_tolerance_minor", 0, i64::MAX, 0),
];

/// Settings with every default filled.
pub fn default_config() -> Map<String, Value> {
    let mut m: Map<String, Value> = NUMBERS
        .iter()
        .map(|(k, _, _, d)| ((*k).into(), json!(d)))
        .collect();
    m.insert("default_locations".into(), json!({}));
    m
}

/// Check a full config (defaults already merged in); unknown keys are refused.
pub fn check_config(config: &Map<String, Value>, r: &mut Rules) -> Value {
    let mut out = Map::new();
    for (key, value) in config {
        let field = format!("config.{key}");
        if let Some((_, min, max, _)) = NUMBERS.iter().find(|(k, ..)| k == key) {
            if let Some(n) = r.integer(&field, value, *min, *max) {
                out.insert(key.clone(), json!(n));
            }
        } else if key == "default_locations" {
            let ok = value.as_object().is_some_and(|m| {
                m.iter().all(|(k, v)| {
                    (1..=100).contains(&k.chars().count())
                        && v.as_str()
                            .is_some_and(|s| (1..=100).contains(&s.chars().count()))
                })
            });
            r.check(
                &field,
                ok,
                "The default locations must map subsidiary ids to location ids.",
            );
            out.insert(key.clone(), value.clone());
        } else {
            r.add(&field, format!("The config.{key} field is not allowed."));
        }
    }
    Value::Object(out)
}

#[derive(Serialize, ToSchema)]
#[schema(description = "A source→target pair running one workflow.")]
pub struct PairResource {
    id: Uuid,
    workflow_key: String,
    source_connection_id: Uuid,
    target_connection_id: Uuid,
    status: String,
    #[schema(required = true)]
    paused_reason: Option<String>,
    #[schema(value_type = Object)]
    config: Value,
    #[schema(required = true)]
    activated_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    last_polled_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<PairRow> for PairResource {
    fn from(p: PairRow) -> Self {
        Self {
            id: p.id,
            workflow_key: p.workflow_key,
            source_connection_id: p.source_connection_id,
            target_connection_id: p.target_connection_id,
            status: p.status,
            paused_reason: p.paused_reason,
            config: p.config,
            activated_at: p.activated_at,
            last_polled_at: p.last_polled_at,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct PairCreated {
    pub pair_id: Uuid,
}

#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Create (source and target connection) or patch (config only). Config keys missing on create take their defaults; a patch merges into the stored config."
)]
pub struct PairInput {
    #[schema(example = "wf01_purchase")]
    workflow_key: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    source_connection_id: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    target_connection_id: Option<String>,
    #[schema(value_type = Option<Object>)]
    config: Option<Value>,
}

/// Shape-checked pair input.
pub struct Input {
    pub source: Option<String>,
    pub target: Option<String>,
    pub config: Option<Map<String, Value>>,
}

impl Validate for PairInput {
    type Valid = Input;

    fn validate(self, r: &mut Rules) -> Option<Input> {
        if let Some(k) = &self.workflow_key {
            r.check(
                "workflow_key",
                k == "wf01_purchase",
                "The selected workflow key is invalid.",
            );
        }
        let config = match self.config {
            None => None,
            Some(Value::Object(m)) => Some(m),
            Some(_) => {
                r.add("config", "The config must be an object.");
                None
            }
        };
        Some(Input {
            source: self.source_connection_id,
            target: self.target_connection_id,
            config,
        })
    }
}
