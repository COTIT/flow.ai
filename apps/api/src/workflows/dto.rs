//! Execution / incident bodies, list queries and resources (contracts §5.5).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::approvals::dto::Person,
};

pub const ACTIONS: [&str; 5] = [
    "link_existing",
    "allow_retry",
    "close_no_po",
    "accept_variance",
    "reject_variance",
];
const REASON_MAX: usize = 2000;

/// `GET …/executions` filters.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExecutionListQuery {
    pub workflow_key: Option<String>,
    pub status: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// `GET …/incidents` filters.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct IncidentListQuery {
    /// open or resolved.
    pub status: Option<String>,
    pub kind: Option<String>,
    /// transient, authorization, data, conflict, missing or unknown_outcome.
    pub category: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// retry body (`retry_after_fix`).
#[derive(Deserialize, ToSchema)]
pub struct RetryRequest {
    #[schema(max_length = 2000)]
    reason: Option<String>,
}

impl Validate for RetryRequest {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        Some(r.text("reason", self.reason, REASON_MAX))
    }
}

/// resolve body.
#[derive(Deserialize, ToSchema)]
pub struct ResolveRequest {
    /// link_existing, allow_retry, close_no_po, accept_variance or reject_variance.
    action: Option<String>,
    #[schema(max_length = 2000)]
    reason: Option<String>,
    /// NetSuite PO id; required for link_existing.
    #[schema(max_length = 100)]
    target_id: Option<String>,
}

/// Valid resolve body.
pub struct ResolveInput {
    pub action: String,
    pub reason: String,
    pub target_id: Option<String>,
}

impl Validate for ResolveRequest {
    type Valid = ResolveInput;

    fn validate(self, r: &mut Rules) -> Option<ResolveInput> {
        let action = r.text("action", self.action, 32);
        if !action.is_empty() {
            r.check(
                "action",
                ACTIONS.contains(&action.as_str()),
                "The selected action is invalid.",
            );
        }
        let reason = r.text("reason", self.reason, REASON_MAX);
        let target_id = r.opt_text("target_id", self.target_id, 100);
        if action == "link_existing" && target_id.is_none() {
            r.add("target_id", "The target id field is required.");
        }
        Some(ResolveInput {
            action,
            reason,
            target_id,
        })
    }
}

/// incident resolve body.
#[derive(Deserialize, ToSchema)]
pub struct IncidentResolveRequest {
    #[schema(max_length = 2000)]
    note: Option<String>,
    /// `reconciled` (needs a verified read-back) or `manual_closed` (default).
    resolution_kind: Option<String>,
}

/// Valid incident resolve: note + resolution kind.
pub struct IncidentResolve {
    pub note: String,
    pub resolution_kind: String,
}

impl Validate for IncidentResolveRequest {
    type Valid = IncidentResolve;

    fn validate(self, r: &mut Rules) -> Option<IncidentResolve> {
        let note = r.text("note", self.note, REASON_MAX);
        let resolution_kind = self
            .resolution_kind
            .unwrap_or_else(|| "manual_closed".into());
        r.check(
            "resolution_kind",
            ["reconciled", "manual_closed"].contains(&resolution_kind.as_str()),
            "The selected resolution kind is invalid.",
        );
        Some(IncidentResolve {
            note,
            resolution_kind,
        })
    }
}

/// One attempt to create the PO.
#[derive(Serialize, ToSchema)]
pub struct AttemptView {
    pub attempt_no: i16,
    pub granted_by: Option<Uuid>,
    pub grant_reason: Option<String>,
    pub write_intent_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
    pub outcome: Option<String>,
}

/// One timeline entry.
#[derive(Serialize, ToSchema)]
pub struct EventView {
    pub kind: String,
    pub actor_kind: String,
    pub created_at: DateTime<Utc>,
    /// Readable line for the timeline.
    pub summary: String,
}

/// One human resolution.
#[derive(Serialize, ToSchema)]
pub struct ResolutionView {
    pub action: String,
    pub actor: Person,
    pub reason: String,
    pub outcome: String,
    pub created_at: DateTime<Utc>,
}

/// Execution (contracts §5.5). Detail adds attempts, events and resolutions.
#[derive(Serialize, ToSchema)]
pub struct ExecutionResource {
    pub id: Uuid,
    pub approval_request_id: Uuid,
    pub workflow_key: String,
    pub correlation_id: String,
    pub status: String,
    pub phase: String,
    pub reason: Option<String>,
    pub resolution: Option<String>,
    pub error_class: Option<String>,
    pub error_message: Option<String>,
    pub target_ref: Option<String>,
    /// True only for `succeeded` and `succeeded_with_variance`.
    pub verified: bool,
    pub remote_attempts: i16,
    pub manual_retry_grants: i16,
    /// Human actions open now (`retry` for blocked runs).
    pub allowed_actions: Vec<&'static str>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempts: Option<Vec<AttemptView>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<EventView>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolutions: Option<Vec<ResolutionView>>,
}

/// Incident (contracts §5.5).
#[derive(Serialize, ToSchema)]
pub struct IncidentResource {
    pub id: Uuid,
    pub kind: String,
    pub subject_ref: String,
    pub status: String,
    pub pair_id: Option<Uuid>,
    pub approval_request_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
    #[schema(value_type = Object)]
    pub detail: Value,
    pub note: Option<String>,
    pub resolved_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    /// WF-03 category; null on WF-01 kinds.
    pub category: Option<String>,
    /// `{sentence, records, last_attempt}`; the provider code is not exposed.
    #[schema(value_type = Option<Object>)]
    pub diagnosis: Option<Value>,
    pub verified_at: Option<DateTime<Utc>>,
    /// `reconciled` or `manual_closed` once resolved.
    pub resolution_kind: Option<String>,
}

/// Expectation behind an incident (contracts §11.7).
#[derive(Serialize, ToSchema)]
pub struct ExpectationView {
    pub status: String,
    pub due_at: DateTime<Utc>,
    pub checks: i16,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub target_kind: String,
    pub target_external_id: String,
}

/// Incident + expectation + timeline (contracts §11.7).
#[derive(Serialize, ToSchema)]
pub struct IncidentDetail {
    #[serde(flatten)]
    pub incident: IncidentResource,
    pub expectation: Option<ExpectationView>,
    pub timeline: Vec<EventView>,
}
