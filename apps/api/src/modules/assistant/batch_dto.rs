//! Fixed batch inputs; fully resolved business requests only.

use super::dto::ProposalInput;
use crate::{
    http::json::{Rules, Validate},
    modules::auth::{password::Password, token},
};
use serde::Deserialize;
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

/// Exact ordered drafts; session binding is supplied once for the whole batch.
/// Fifty covers one price-list import page; office keeps model-built batches at ten.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchInput {
    pub session_id: Uuid,
    #[schema(min_items = 2, max_items = 50)]
    pub actions: Vec<Value>,
}
/// Validated drafts, each carrying the same owner-bound session.
pub struct BatchDraft {
    pub session_id: Uuid,
    pub actions: Vec<ProposalInput>,
}
impl Validate for BatchInput {
    type Valid = BatchDraft;
    fn validate(self, r: &mut Rules) -> Option<BatchDraft> {
        r.check(
            "actions",
            (2..=50).contains(&self.actions.len()),
            "Choose two through fifty exact actions.",
        );
        r.check(
            "actions",
            serde_json::to_string(&self.actions).map_or(usize::MAX, |v| v.len()) <= 65536,
            "The combined business drafts exceed 65536 bytes.",
        );
        let mut actions = Vec::new();
        for (index, mut value) in self.actions.into_iter().enumerate() {
            let Some(object) = value.as_object_mut() else {
                r.add(
                    &format!("actions.{index}"),
                    "Use an exact business request.",
                );
                continue;
            };
            if object.contains_key("session_id") {
                r.add(
                    &format!("actions.{index}"),
                    "Supply the session once for the batch.",
                );
                continue;
            }
            object.insert("session_id".into(), serde_json::json!(self.session_id));
            match serde_json::from_value::<ProposalInput>(value) {
                Ok(draft) => {
                    let mut rules = Rules::default();
                    let normalized = draft.validate(&mut rules);
                    match rules.into_result() {
                        Ok(()) => {
                            if let Some(input) = normalized {
                                actions.push(input);
                            }
                        }
                        Err(error) => r.add(&format!("actions.{index}"), error.to_string()),
                    }
                }
                Err(_) => r.add(
                    &format!("actions.{index}"),
                    "Use an exact resolved business request.",
                ),
            }
        }
        Some(BatchDraft {
            session_id: self.session_id,
            actions,
        })
    }
}
/// Private controller proof, hashed in PostgreSQL and never exposed to model/UI.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdvanceInput {
    pub session_id: Uuid,
    #[schema(value_type=String)]
    pub execution_token: Password,
    pub action_id: Uuid,
}
impl Validate for AdvanceInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        r.check(
            "execution_token",
            token::looks_valid(self.execution_token.expose()),
            "Invalid batch controller proof.",
        );
        Some(self)
    }
}
/// Stop future admissions; already admitted work may finish.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PauseInput {
    pub session_id: Uuid,
    #[serde(default = "default_reason")]
    pub reason: String,
}
fn default_reason() -> String {
    "disconnected".into()
}
impl Validate for PauseInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        r.check(
            "reason",
            matches!(
                self.reason.as_str(),
                "disconnected"
                    | "changed_intent"
                    | "unknown"
                    | "failed"
                    | "partial"
                    | "queued"
                    | "expired"
                    | "cancelled"
                    | "authorization"
                    | "semantic_failure"
                    | "interrupted"
            ),
            "Choose a batch stop reason.",
        );
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn valid(value: Value) -> bool {
        let draft: BatchInput = serde_json::from_value(value).unwrap();
        let mut rules = Rules::default();
        draft.validate(&mut rules);
        rules.into_result().is_ok()
    }
    fn action() -> Value {
        json!({"operation_id":"warehouses_create","method":"POST","path":"/v1/workspace/01950000-0000-7000-8000-000000000001/warehouses","body":{"name":"Reviewed"}})
    }
    #[test]
    fn batch_bounds_and_single_session_binding_are_explicit() {
        let session = Uuid::now_v7();
        assert!(valid(
            json!({"session_id":session,"actions":[action(),action()]})
        ));
        assert!(!valid(json!({"session_id":session,"actions":[action()]})));
        assert!(!valid(
            json!({"session_id":session,"actions":vec![action();51]})
        ));
        let mut nested = action();
        nested["session_id"] = json!(session);
        assert!(!valid(
            json!({"session_id":session,"actions":[nested,action()]})
        ));
        let mut secret = action();
        secret["body"] = json!({"credentials":{"password":"never-review-secret"}});
        assert!(!valid(
            json!({"session_id":session,"actions":[secret,action()]})
        ));
    }
}
