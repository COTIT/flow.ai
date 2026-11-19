//! Approval request/policy bodies, list query and response resources (contracts §5.3).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::http::json::{Rules, Validate};

pub const STATUSES: [&str; 6] = [
    "blocked",
    "pending",
    "approved",
    "rejected",
    "returned",
    "invalidated",
];
pub const WORKFLOW_KEYS: [&str; 1] = ["wf01_purchase"];
const COMMENT_MAX: usize = 2000;

fn is_fingerprint(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// approve / reject / return body.
#[derive(Deserialize, ToSchema)]
pub struct DecideRequest {
    /// The fingerprint the approver saw (64 lowercase hex).
    #[schema(example = "9f2c…")]
    fingerprint: Option<String>,
    /// Required for reject and return.
    #[schema(max_length = 2000)]
    comment: Option<String>,
}

/// Valid decide body.
pub struct DecideInput {
    pub fingerprint: String,
    pub comment: Option<String>,
}

impl Validate for DecideRequest {
    type Valid = DecideInput;

    fn validate(self, r: &mut Rules) -> Option<DecideInput> {
        let fingerprint = r.text("fingerprint", self.fingerprint, 64);
        if !fingerprint.is_empty() {
            r.check(
                "fingerprint",
                is_fingerprint(&fingerprint),
                "The fingerprint field format is invalid.",
            );
        }
        let comment = r.opt_text("comment", self.comment, COMMENT_MAX);
        Some(DecideInput {
            fingerprint,
            comment,
        })
    }
}

/// resubmit body.
#[derive(Deserialize, ToSchema)]
pub struct ResubmitRequest {
    #[schema(max_length = 2000)]
    comment: Option<String>,
}

impl Validate for ResubmitRequest {
    type Valid = Option<String>;

    fn validate(self, r: &mut Rules) -> Option<Option<String>> {
        Some(r.opt_text("comment", self.comment, COMMENT_MAX))
    }
}

/// New policy.
#[derive(Deserialize, ToSchema)]
pub struct CreatePolicyRequest {
    #[schema(example = "wf01_purchase")]
    workflow_key: Option<String>,
    #[schema(example = "EUR", max_length = 64)]
    currency_ref: Option<String>,
    /// Untaxed amount in minor units from which `integration.approve_high` is needed.
    #[schema(value_type = i64, minimum = 0)]
    threshold_minor: Option<Value>,
}

/// Valid new policy.
pub struct NewPolicy {
    pub workflow_key: String,
    pub currency_ref: String,
    pub threshold_minor: i64,
}

fn threshold(r: &mut Rules, v: Option<Value>) -> Option<i64> {
    let v = r.required_value("threshold_minor", v)?;
    r.integer("threshold_minor", &v, 0, i64::MAX)
}

impl Validate for CreatePolicyRequest {
    type Valid = NewPolicy;

    fn validate(self, r: &mut Rules) -> Option<NewPolicy> {
        let workflow_key = r.text("workflow_key", self.workflow_key, 64);
        if !workflow_key.is_empty() {
            r.check(
                "workflow_key",
                WORKFLOW_KEYS.contains(&workflow_key.as_str()),
                "The selected workflow key is invalid.",
            );
        }
        Some(NewPolicy {
            workflow_key,
            currency_ref: r.text("currency_ref", self.currency_ref, 64),
            threshold_minor: threshold(r, self.threshold_minor).unwrap_or_default(),
        })
    }
}

/// Policy patch: absent keeps.
#[derive(Deserialize, ToSchema)]
pub struct UpdatePolicyRequest {
    #[schema(value_type = Option<i64>, minimum = 0)]
    threshold_minor: Option<Value>,
    #[schema(value_type = Option<bool>)]
    active: Option<Value>,
}

/// Valid policy patch.
pub struct PolicyPatch {
    pub threshold_minor: Option<i64>,
    pub active: Option<bool>,
}

impl Validate for UpdatePolicyRequest {
    type Valid = PolicyPatch;

    fn validate(self, r: &mut Rules) -> Option<PolicyPatch> {
        let threshold_minor = self.threshold_minor.and_then(|v| threshold(r, Some(v)));
        let active = self.active.and_then(|v| r.boolean("active", &v));
        Some(PolicyPatch {
            threshold_minor,
            active,
        })
    }
}

/// `GET /approvals/requests` filters.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RequestListQuery {
    pub workflow_key: Option<String>,
    /// blocked, pending, approved, rejected, returned or invalidated.
    pub status: Option<String>,
    /// `all` (needs an integration permission, else forced to `mine`), `mine` or `decidable`.
    pub scope: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// Paging only.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// Exact amount: integer minor units of `currency_ref`.
#[derive(Serialize, ToSchema)]
pub struct Amount {
    pub minor: i64,
    pub currency_ref: String,
    pub decimals: i16,
}

/// A member, by id and display name.
#[derive(Serialize, ToSchema)]
pub struct Person {
    pub user_id: Uuid,
    pub name: String,
}

/// One parsed `blocked_reasons` code (`code[:entity[:source_id]]`).
#[derive(Serialize, ToSchema)]
pub struct BlockedReason {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

impl BlockedReason {
    /// `unmapped:vendor:42` → code, entity, source_id. Other codes keep their rest as entity
    /// (`no_policy:EUR`, `connection_inactive:<uuid>`).
    pub fn parse(raw: &str) -> Self {
        let mut it = raw.splitn(3, ':');
        Self {
            code: it.next().unwrap_or_default().to_owned(),
            entity: it.next().map(str::to_owned),
            source_id: it.next().map(str::to_owned),
        }
    }
}

/// The decision on a request.
#[derive(Serialize, ToSchema)]
pub struct DecisionView {
    /// approve, reject or return.
    pub decision: String,
    pub actor: Person,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Execution status, kept apart from the approval status (PLAN test 23).
#[derive(Serialize, ToSchema)]
pub struct ExecutionSummary {
    pub id: Uuid,
    pub status: String,
    pub phase: String,
    pub reason: Option<String>,
    pub resolution: Option<String>,
    pub error_class: Option<String>,
    /// Readable, never a secret.
    pub error_message: Option<String>,
    pub target_ref: Option<String>,
    /// True only for `succeeded` and `succeeded_with_variance`.
    pub verified: bool,
    pub updated_at: DateTime<Utc>,
}

/// An open incident on the request.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct IncidentRef {
    pub id: Uuid,
    pub kind: String,
}

/// ApprovalRequest (contracts §5.3). List rows leave out `snapshot`.
#[derive(Serialize, ToSchema)]
pub struct ApprovalRequestResource {
    pub id: Uuid,
    pub workflow_key: String,
    pub pair_id: Uuid,
    pub subject_ref: String,
    pub revision: i32,
    pub fingerprint: String,
    pub status: String,
    pub blocked_reasons: Vec<BlockedReason>,
    pub invalidated_reason: Option<String>,
    pub amount: Amount,
    pub required_permission: Option<String>,
    pub requester: Option<Person>,
    pub decision: Option<DecisionView>,
    /// The caller passes four-eyes and holds the needed permission, and it is pending.
    pub can_decide: bool,
    pub execution: Option<ExecutionSummary>,
    pub open_incidents: Vec<IncidentRef>,
    /// Snapshot JSON v1 (contracts §5.4); detail only.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub snapshot: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 201 body of resubmit.
#[derive(Serialize, ToSchema)]
pub struct Resubmitted {
    pub approval_request_id: Uuid,
}

/// Approval policy.
#[derive(Serialize, ToSchema)]
pub struct PolicyResource {
    pub id: Uuid,
    pub workflow_key: String,
    pub currency_ref: String,
    pub threshold_minor: i64,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 201 body of policy create.
#[derive(Serialize, ToSchema)]
pub struct PolicyCreated {
    pub policy_id: Uuid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_reason_codes_parse() {
        let r = BlockedReason::parse("unmapped:vendor:42");
        assert_eq!(
            (r.code.as_str(), r.entity.as_deref(), r.source_id.as_deref()),
            ("unmapped", Some("vendor"), Some("42"))
        );
        let r = BlockedReason::parse("no_requester");
        assert_eq!(
            (r.code.as_str(), r.entity, r.source_id),
            ("no_requester", None, None)
        );
        assert!(is_fingerprint(&"a".repeat(64)));
        assert!(!is_fingerprint(&"A".repeat(64)));
    }
}
