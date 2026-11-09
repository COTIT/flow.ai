//! Developer gap ledger: voice requests assistant could not finish. Bridge-signed writes only; no read route.

use super::gaps_repo;
use crate::{
    AppError, AppState,
    error::UnprocessableBody,
    http::{
        assistant::SignedJson,
        json::{Rules, Validate},
        response::{Data, Envelope, MessageBody},
        workspace::WorkspaceAccess,
    },
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

/// One failed or abandoned voice request; redacted catalog words, never transcripts or record data.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GapInput {
    /// Catalog domain, or `unknown`.
    pub domain: String,
    /// Session language: `en`, `nl` or `sw`.
    pub language: String,
    /// Lowercase words separated by single spaces, at most 120 chars; `unknown` when redaction left too little.
    pub intent: String,
    /// Operation ids the agent tried, at most 10.
    #[serde(default)]
    pub tools: Vec<String>,
    /// `unsupported`, `refused`, `failed`, `cancelled` or `dropped`.
    pub outcome: String,
    /// Backend status of the failed call, 400-599.
    #[serde(default)]
    pub http_status: Option<i16>,
}
impl Validate for GapInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        let word = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase());
        r.check(
            "domain",
            (1..=40).contains(&self.domain.len())
                && self
                    .domain
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_'),
            "Use a catalog domain.",
        );
        r.check(
            "language",
            matches!(self.language.as_str(), "en" | "nl" | "sw"),
            "Choose en, nl or sw.",
        );
        r.check(
            "intent",
            self.intent.len() <= 120 && self.intent.split(' ').all(word),
            "Use lowercase words only: no names, numbers or record data.",
        );
        r.check(
            "tools",
            self.tools.len() <= 10 && self.tools.iter().all(|t| operation_id(t)),
            "List at most 10 operation ids.",
        );
        r.check(
            "outcome",
            matches!(
                self.outcome.as_str(),
                "unsupported" | "refused" | "failed" | "cancelled" | "dropped"
            ),
            "Choose unsupported, refused, failed, cancelled or dropped.",
        );
        r.check(
            "http_status",
            self.http_status.is_none_or(|s| (400..=599).contains(&s)),
            "Use a failure status from 400 to 599.",
        );
        Some(self)
    }
}

fn operation_id(id: &str) -> bool {
    (2..=100).contains(&id.len())
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// What happened after a gap: the user rephrased, or opened an Office page (route pattern only).
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FollowUpInput {
    /// Workspace owning the gap; the page proxy carries no staff bearer.
    pub workspace_id: Uuid,
    /// `rephrased` or `used_ui`.
    pub signal: String,
    /// Required for `used_ui`: page pattern like `/orders/[id]`, no query string or record id.
    #[serde(default)]
    pub route: Option<String>,
}
impl Validate for FollowUpInput {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        match (self.signal.as_str(), self.route.as_deref()) {
            ("rephrased", route) => r.check("route", route.is_none(), "Only used_ui has a route."),
            ("used_ui", Some(route)) => r.check(
                "route",
                route_pattern(route),
                "Use an Office page pattern like /orders/[id].",
            ),
            ("used_ui", None) => r.missing("route"),
            _ => r.add("signal", "Choose rephrased or used_ui."),
        }
        Some(self)
    }
}

fn route_pattern(route: &str) -> bool {
    route.len() <= 80
        && route.starts_with('/')
        && route[1..].split('/').all(|s| {
            s == "[id]"
                || (!s.is_empty()
                    && !s.starts_with('-')
                    && !s.ends_with('-')
                    && !s.contains("--")
                    && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'))
        })
}

/// Record one gap for this workspace; Office bridge signature required.
#[utoipa::path(post,path="/workspace/{workspace}/assistant/gaps",tag="assistant",operation_id="assistant_gaps_create",params(("workspace"=Uuid,Path)),request_body=GapInput,responses((status=201,body=Envelope<Value>,description="Recorded gap id"),(status=422,body=UnprocessableBody,description="Invalid control input")),security(("bearer"=[],"assistant_bridge"=[])))]
pub async fn create(
    State(s): State<AppState>,
    a: WorkspaceAccess,
    SignedJson(i): SignedJson<GapInput>,
) -> Result<(StatusCode, Data<Value>), AppError> {
    let id = gaps_repo::insert(&s.db, a.workspace_id, &i).await?;
    Ok((StatusCode::CREATED, Data(json!({ "id": id }))))
}

/// Set one follow-up signal within 10 minutes of the gap; signature only, filtered by workspace.
#[utoipa::path(post,path="/assistant/gaps/{gap}/follow-up",tag="assistant",operation_id="assistant_gaps_follow_up",params(("gap"=Uuid,Path)),request_body=FollowUpInput,responses((status=200,body=Envelope<Value>,description="Gap with its follow-up"),(status=409,body=MessageBody,description="Missing, expired, changed or consumed control"),(status=422,body=UnprocessableBody,description="Invalid control input")),security(("assistant_bridge"=[])))]
pub async fn follow_up(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    SignedJson(i): SignedJson<FollowUpInput>,
) -> Result<Data<Value>, AppError> {
    match gaps_repo::follow_up(&s.db, i.workspace_id, id, &i.signal, i.route.as_deref()).await? {
        Some(true) => Ok(Data(json!({ "id": id, "follow_up": i.signal }))),
        Some(false) => Err(AppError::Conflict(
            "This gap already has a follow-up or is too old.",
        )),
        None => Err(AppError::NotFound),
    }
}
