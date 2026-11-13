//! Bounded dialogue inputs and history query parameters.

use crate::{
    AppError,
    http::json::{Rules, Validate},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// New member-owned dialogue.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateInput {
    pub title: Option<String>,
}
impl Validate for CreateInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        r.check(
            "title",
            self.title
                .as_ref()
                .is_none_or(|s| s.len() <= 200 && !s.contains('\0')),
            "Title must not exceed 200 bytes.",
        );
        Some(self)
    }
}
/// Trusted provider source bound to fresh execution session.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceInput {
    pub session_id: Uuid,
    pub provider_conversation_id: String,
    pub agent_id: String,
    pub text_only: bool,
}
impl Validate for SourceInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        identifier(
            r,
            "provider_conversation_id",
            &self.provider_conversation_id,
        );
        identifier(r, "agent_id", &self.agent_id);
        Some(self)
    }
}
/// Client-observed dialogue event; execution state never stored here.
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub key: String,
    pub ordinal: i64,
    pub role: String,
    pub text: String,
    pub modality: String,
    pub status: String,
    #[serde(default)]
    pub interrupted: bool,
}
/// Authenticated original-login event batch.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EventsInput {
    pub session_id: Uuid,
    pub source_id: Uuid,
    pub events: Vec<Event>,
}
impl Validate for EventsInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        r.check(
            "events",
            !self.events.is_empty() && self.events.len() <= 100,
            "Send 1 to 100 events.",
        );
        let mut keys = std::collections::HashSet::new();
        let mut ordinals = std::collections::HashSet::new();
        r.check(
            "events",
            self.events
                .iter()
                .all(|e| keys.insert(&e.key) && ordinals.insert(e.ordinal)),
            "Event keys and ordinals must be unique within batch.",
        );
        for e in &self.events {
            turn(r, &e.key, &e.role, &e.text, &e.modality);
            r.check(
                "ordinal",
                (0..1000000).contains(&e.ordinal),
                "Ordinal must be between 0 and 999999.",
            );
            r.check(
                "status",
                matches!(
                    e.status.as_str(),
                    "queued" | "sent_unconfirmed" | "complete"
                ),
                "Choose queued, sent_unconfirmed or complete.",
            );
        }
        Some(self)
    }
}
/// Canonical provider turn after webhook verification by Office.
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FinalTurn {
    pub key: String,
    pub role: String,
    pub text: String,
    pub modality: String,
    #[serde(default)]
    pub interrupted: bool,
}
/// Signature-only canonical source snapshot.
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IngestInput {
    pub provider_conversation_id: String,
    pub agent_id: String,
    pub turns: Vec<FinalTurn>,
    pub summary: Option<String>,
}
impl Validate for IngestInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        identifier(
            r,
            "provider_conversation_id",
            &self.provider_conversation_id,
        );
        identifier(r, "agent_id", &self.agent_id);
        r.check(
            "turns",
            self.turns.len() <= 1000,
            "Send at most 1000 turns.",
        );
        r.check(
            "summary",
            self.summary
                .as_ref()
                .is_none_or(|s| s.len() <= 16384 && !s.contains('\0')),
            "Summary must not exceed 16384 bytes.",
        );
        for t in &self.turns {
            turn(r, &t.key, &t.role, &t.text, &t.modality);
        }
        let mut keys = std::collections::HashSet::new();
        r.check(
            "turns",
            self.turns.iter().all(|t| keys.insert(&t.key)),
            "Turn keys must be unique.",
        );
        Some(self)
    }
}
fn identifier(r: &mut Rules, field: &str, value: &str) {
    r.check(
        field,
        (1..=200).contains(&value.len())
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "Use a provider identifier.",
    );
}
fn turn(r: &mut Rules, key: &str, role: &str, text: &str, modality: &str) {
    r.check(
        "key",
        !key.is_empty()
            && key.len() <= 256
            && !key.contains('\0')
            && super::redaction::archive_text(key) == key,
        "Key must contain 1 to 256 bytes.",
    );
    r.check(
        "role",
        matches!(role, "user" | "agent"),
        "Choose user or agent.",
    );
    r.check(
        "text",
        text.len() <= 16384 && !text.contains('\0'),
        "Text must not exceed 16384 bytes.",
    );
    r.check(
        "modality",
        matches!(modality, "text" | "voice"),
        "Choose text or voice.",
    );
}
/// Bounded revision-aware history page. Tail returns latest complete turns ascending.
#[derive(Default, Deserialize, IntoParams)]
#[into_params(parameter_in=Query)]
pub struct HistoryQuery {
    pub session_id: Option<Uuid>,
    pub cursor: Option<String>,
    pub revision: Option<i64>,
    pub q: Option<String>,
    pub source_id: Option<Uuid>,
    pub limit: Option<i64>,
    #[serde(default)]
    pub tail: bool,
}
impl HistoryQuery {
    /// Validate limits before database access.
    pub fn check(&self) -> Result<(), AppError> {
        if self.limit.is_some_and(|n| !(1..=100).contains(&n)) {
            return Err(AppError::field(
                "limit",
                "Choose a limit between 1 and 100.",
            ));
        }
        if self
            .q
            .as_ref()
            .is_some_and(|s| s.len() > 200 || s.contains('\0'))
        {
            return Err(AppError::field("q", "Search must not exceed 200 bytes."));
        }
        if self
            .cursor
            .as_ref()
            .is_some_and(|s| s.len() > 200 || s.contains('\0'))
        {
            return Err(AppError::field("cursor", "Invalid history cursor."));
        }
        Ok(())
    }
}

/// Member dialogue metadata; detail includes bounded recent sources.
#[derive(Serialize, ToSchema)]
pub struct ConversationResource {
    pub id: Uuid,
    pub title: String,
    /// Lifecycle: active, ended or deleted.
    pub state: String,
    pub revision: i64,
    /// Canonical coverage: complete, provisional or partial.
    pub coverage: String,
    pub last_activity_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<ConversationSourceResource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources_has_more: Option<bool>,
}
/// Source provenance; execution id appears only in signed creation response.
#[derive(Serialize, ToSchema)]
pub struct ConversationSourceResource {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub ordinal: i64,
    pub revision: i64,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_only: Option<bool>,
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}
/// Bounded conversation list.
#[derive(Serialize, ToSchema)]
pub struct ConversationListResource {
    pub conversations: Vec<ConversationResource>,
    pub cursor: Option<String>,
    pub has_more: bool,
}
/// Exact stored user or assistant dialogue; delivery uncertainty stays explicit.
#[derive(Serialize, ToSchema)]
pub struct ConversationMessageResource {
    pub id: Uuid,
    pub source_id: Uuid,
    pub key: String,
    pub ordinal: i64,
    pub role: String,
    pub text: String,
    pub modality: String,
    pub status: String,
    pub interrupted: bool,
    pub provenance: String,
}
/// Source without canonical final transcript.
#[derive(Serialize, ToSchema)]
pub struct ConversationGapResource {
    pub source_id: Uuid,
    pub state: String,
}
/// Byte-bounded exact history page; cursor embeds content revision.
#[derive(Serialize, ToSchema)]
pub struct ConversationMessagesResource {
    pub messages: Vec<ConversationMessageResource>,
    pub revision: i64,
    pub cursor: Option<String>,
    pub has_more: bool,
    pub through_sequence: Option<String>,
    pub gaps: Vec<ConversationGapResource>,
}
/// Deleted-content confirmation or accepted final-source state.
#[derive(Serialize, ToSchema)]
pub struct ConversationStateResource {
    pub id: Uuid,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
}

/// Bounded source-summary directory parameters.
#[derive(Default, Deserialize, IntoParams)]
#[into_params(parameter_in=Query)]
pub struct DetailQuery {
    pub source_before: Option<i64>,
    pub source_limit: Option<i64>,
}
/// Dialogue metadata plus paged immutable-source summaries.
#[derive(Serialize, ToSchema)]
pub struct ConversationDetailResource {
    pub conversation: ConversationResource,
    pub sources: Vec<ConversationSourceResource>,
    pub sources_has_more: bool,
    pub next_source_cursor: Option<i64>,
}
