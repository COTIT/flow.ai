//! Webhook endpoint request/response bodies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, is_url, present},
    modules::webhooks::{events::EVENTS, repo::EndpointPatch, repo::EndpointRow},
};

const URL_MAX: usize = 2000;
const DESCRIPTION_MAX: usize = 500;
const EVENTS_MAX: usize = 50;
const BAD_URL: &str = "The url must be a public https URL.";

/// An endpoint, never its secret.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "Where this workspace's events are POSTed, signed with the endpoint's secret."
)]
pub struct WebhookEndpointResource {
    id: Uuid,
    #[schema(example = "https://shop.example.com/hooks")]
    url: String,
    /// Subscribed events: `product.changed`, `stock.changed`.
    events: Vec<String>,
    #[schema(required = true)]
    description: Option<String>,
    /// Inactive endpoints get nothing.
    active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<EndpointRow> for WebhookEndpointResource {
    fn from(e: EndpointRow) -> Self {
        Self {
            id: e.id,
            url: e.url,
            events: e.events,
            description: e.description,
            active: e.active,
            created_at: e.created_at,
            updated_at: e.updated_at,
        }
    }
}

/// Create response: the endpoint plus its signing secret, shown this once.
#[derive(Serialize, ToSchema)]
#[schema(description = "A new endpoint. Store `secret` now: it is never shown again.")]
pub struct WebhookEndpointCreated {
    id: Uuid,
    url: String,
    events: Vec<String>,
    #[schema(required = true)]
    description: Option<String>,
    active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    /// Standard Webhooks secret: HMAC-SHA256 key is the base64 after `whsec_`.
    #[schema(example = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw")]
    secret: String,
}

impl WebhookEndpointCreated {
    /// From the stored row and the plaintext secret.
    pub fn new(e: EndpointRow, secret: String) -> Self {
        Self {
            id: e.id,
            url: e.url,
            events: e.events,
            description: e.description,
            active: e.active,
            created_at: e.created_at,
            updated_at: e.updated_at,
            secret,
        }
    }
}

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// POST /webhook-endpoints.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New endpoint. `url` must be https.")]
pub struct CreateEndpointRequest {
    #[schema(value_type = String, max_length = 2000, example = "https://shop.example.com/hooks")]
    url: Option<String>,
    #[schema(value_type = Vec<String>, example = json!(["stock.changed"]))]
    events: Option<Value>,
    #[schema(max_length = 500)]
    description: Option<String>,
}

/// Validated create input.
pub struct NewEndpointInput {
    pub url: String,
    pub events: Vec<String>,
    pub description: Option<String>,
}

/// PATCH /webhook-endpoints/{id}: absent keeps the field.
#[derive(Deserialize, ToSchema)]
#[schema(description = "Changes to an endpoint. Leave a field out to keep it.")]
pub struct UpdateEndpointRequest {
    #[schema(value_type = Option<String>, max_length = 2000)]
    url: Option<String>,
    #[schema(value_type = Option<Vec<String>>)]
    events: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 500)]
    description: Option<Option<String>>,
    #[schema(value_type = Option<bool>)]
    active: Option<Value>,
}

/// https only, a dotted host, no userinfo, at most 2000 chars. Private hosts are refused when
/// sending (DNS can change after this check).
fn url(r: &mut Rules, v: Option<String>) -> String {
    let v = r.text("url", v, URL_MAX);
    if !v.is_empty() {
        r.check("url", v.starts_with("https://") && is_url(&v), BAD_URL);
    }
    v
}

/// 1..=50 distinct known events, sorted.
fn events(r: &mut Rules, v: Option<Value>) -> Vec<String> {
    let Some(v) = r.required_value("events", v) else {
        return Vec::new();
    };
    let list: Option<Vec<String>> = v
        .as_array()
        .and_then(|a| a.iter().map(|s| s.as_str().map(str::to_owned)).collect());
    let Some(mut list) = list.filter(|l| (1..=EVENTS_MAX).contains(&l.len())) else {
        r.add("events", "The events field must list 1 to 50 events.");
        return Vec::new();
    };
    if let Some(bad) = list.iter().find(|e| !EVENTS.contains(&e.as_str())) {
        r.add("events", format!("Unknown event {bad}."));
    }
    list.sort();
    list.dedup();
    list
}

impl Validate for CreateEndpointRequest {
    type Valid = NewEndpointInput;

    fn validate(self, r: &mut Rules) -> Option<NewEndpointInput> {
        Some(NewEndpointInput {
            url: url(r, self.url),
            events: events(r, self.events),
            description: r.opt_text("description", self.description, DESCRIPTION_MAX),
        })
    }
}

impl Validate for UpdateEndpointRequest {
    type Valid = EndpointPatch;

    fn validate(self, r: &mut Rules) -> Option<EndpointPatch> {
        Some(EndpointPatch {
            url: self.url.map(|u| url(r, Some(u))),
            events: self.events.map(|e| events(r, Some(e))),
            description: r.patch_opt_text("description", self.description, DESCRIPTION_MAX),
            active: self.active.and_then(|a| r.boolean("active", &a)),
        })
    }
}
