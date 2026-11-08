//! Webhook endpoint request/response bodies, plus the delivery history read.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, PartialSchema, ToSchema, openapi::RefOr, openapi::schema::Schema};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, Validate, is_url, present},
    modules::webhooks::{
        events::EVENTS,
        repo::{DeliveryRow, EndpointPatch, EndpointRow},
    },
    openapi::string_enum,
};

const URL_MAX: usize = 2000;
const DESCRIPTION_MAX: usize = 500;
const EVENTS_MAX: usize = 50;
const BAD_URL: &str = "The url must be a public https URL.";

/// `EVENTS` as a spec enum (docs only).
pub struct WebhookEvent;

impl PartialSchema for WebhookEvent {
    fn schema() -> RefOr<Schema> {
        string_enum(
            EVENTS.iter().copied(),
            "An event an endpoint can subscribe to.",
        )
    }
}

impl ToSchema for WebhookEvent {}

/// An endpoint, never its secret.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "Where this workspace's events are POSTed, signed with the endpoint's secret."
)]
pub struct WebhookEndpointResource {
    id: Uuid,
    #[schema(example = "https://shop.example.com/hooks")]
    url: String,
    #[schema(value_type = Vec<WebhookEvent>)]
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
    #[schema(value_type = Vec<WebhookEvent>)]
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

/// GET deliveries query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeliveriesQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// Only deliveries in this state: `pending`, `delivered` or `failed`.
    pub status: Option<String>,
}

impl DeliveriesQuery {
    /// The status filter; anything outside the three states is 422.
    pub fn status(&self) -> Result<Option<&str>, AppError> {
        match self.status.as_deref() {
            None => Ok(None),
            Some(s @ ("pending" | "delivered" | "failed")) => Ok(Some(s)),
            Some(_) => Err(AppError::field("status", "The selected status is invalid.")),
        }
    }
}

/// One event sent (or being sent) to an endpoint: its outcome and why it failed.
#[derive(Serialize, ToSchema)]
pub struct WebhookDeliveryResource {
    id: Uuid,
    event_id: Uuid,
    /// Event type, e.g. `stock.changed`.
    event_type: String,
    /// `pending` (retrying), `delivered`, or `failed` (gave up after the last attempt).
    status: String,
    attempts: i32,
    /// HTTP status of the last answer; null when no answer came.
    #[schema(required = true)]
    last_status: Option<i16>,
    /// Why the last attempt failed. URL query strings are cut, since they may carry tokens.
    #[schema(required = true)]
    last_error: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[schema(required = true)]
    delivered_at: Option<DateTime<Utc>>,
}

impl From<DeliveryRow> for WebhookDeliveryResource {
    fn from(d: DeliveryRow) -> Self {
        Self {
            id: d.id,
            event_id: d.event_id,
            event_type: d.event_type,
            status: d.status,
            attempts: d.attempts,
            last_status: d.last_status,
            last_error: d.last_error.as_deref().map(strip_queries),
            created_at: d.created_at,
            updated_at: d.updated_at,
            delivered_at: d.delivered_at,
        }
    }
}

/// `(https://h/p?token=x):` -> `(https://h/p):` in each word holding a URL.
fn strip_queries(text: &str) -> String {
    text.split(' ')
        .map(|word| match (word.find("://"), word.find('?')) {
            (Some(scheme), Some(q)) if q > scheme => {
                let end = word[q..]
                    .find([')', '"', '\'', ','])
                    .map_or(word.len(), |i| q + i);
                format!("{}{}", &word[..q], &word[end..])
            }
            _ => word.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// POST /webhook-endpoints.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New endpoint. `url` must be https.")]
pub struct CreateEndpointRequest {
    #[schema(value_type = String, max_length = 2000, example = "https://shop.example.com/hooks")]
    url: Option<String>,
    #[schema(value_type = Vec<WebhookEvent>, example = json!(["stock.changed"]))]
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
    #[schema(value_type = Option<Vec<WebhookEvent>>)]
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

#[cfg(test)]
mod tests {
    use super::strip_queries;

    #[test]
    fn strip_queries_cuts_url_queries_only() {
        assert_eq!(
            strip_queries(
                "error sending request for url (https://h.io/p?token=s3cr3t&a=1): refused"
            ),
            "error sending request for url (https://h.io/p): refused"
        );
        assert_eq!(strip_queries("https://h.io/p?x=1"), "https://h.io/p");
        assert_eq!(
            strip_queries("why? endpoint answered 500"),
            "why? endpoint answered 500"
        );
        assert_eq!(strip_queries("endpoint inactive"), "endpoint inactive");
    }
}
