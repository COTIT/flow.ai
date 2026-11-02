//! Bug report bodies and the multipart form.

use axum::extract::Multipart;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::AppError,
    modules::{farm_bug_reports::repo::ReportRow, farm_records::dto::ActorRef},
};

/// Each image is capped at 5 MB.
pub const MAX_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_ACTIONS: usize = 30;

/// A filed report: its id and the ticket number people say out loud (BUG-12).
#[derive(Serialize, ToSchema)]
pub struct BugReportFiled {
    id: Uuid,
    r#ref: i64,
}

impl BugReportFiled {
    pub fn new(id: Uuid, r#ref: i64) -> Self {
        Self { id, r#ref }
    }
}

/// A bug report as farm admin reads it. Image links last 10 minutes.
#[derive(Serialize, ToSchema)]
pub struct BugReportResource {
    id: Uuid,
    r#ref: i64,
    #[schema(value_type = ActorRef)]
    reporter: Value,
    route: String,
    user_agent: String,
    viewport: String,
    description: String,
    /// The last actions before the report, as the app sent them (at most 30).
    #[schema(value_type = Vec<Object>)]
    actions: Value,
    has_screenshot: bool,
    has_photo: bool,
    /// Only on a single read.
    #[schema(required = true)]
    screenshot_url: Option<String>,
    #[schema(required = true)]
    photo_url: Option<String>,
    /// `open`, `in_progress`, `fixed` or `wont_fix`; moved by operators only.
    status: String,
    #[schema(required = true)]
    resolution: Option<String>,
    created_at: DateTime<Utc>,
}

impl BugReportResource {
    pub fn new(r: ReportRow, screenshot_url: Option<String>, photo_url: Option<String>) -> Self {
        Self {
            id: r.id,
            r#ref: r.r#ref,
            reporter: r.reporter,
            route: r.route,
            user_agent: r.user_agent,
            viewport: r.viewport,
            description: r.description,
            actions: r.actions,
            has_screenshot: r.screenshot_key.is_some(),
            has_photo: r.photo_key.is_some(),
            screenshot_url,
            photo_url,
            status: r.status,
            resolution: r.resolution,
            created_at: r.created_at,
        }
    }
}

/// Multipart form. `screenshot` or `photo` (or both) is required.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct BugReportForm {
    /// Client-made id, so a retried report is filed once.
    id: Option<Uuid>,
    route: String,
    user_agent: String,
    viewport: String,
    description: String,
    /// JSON array of the last actions, at most 30.
    actions: String,
    #[schema(value_type = Option<String>, format = Binary)]
    screenshot: Option<Vec<u8>>,
    #[schema(value_type = Option<String>, format = Binary)]
    photo: Option<Vec<u8>>,
}

/// The parsed form, unchecked.
#[derive(Default)]
pub struct Form {
    pub id: Option<Uuid>,
    pub route: Option<String>,
    pub user_agent: Option<String>,
    pub viewport: Option<String>,
    pub description: Option<String>,
    pub actions: Option<String>,
    pub screenshot: Option<Vec<u8>>,
    pub photo: Option<Vec<u8>>,
}

fn bad(field: &str, message: &str) -> AppError {
    AppError::field(field, message)
}

/// Read the form; images capped at 5 MB each while streaming. Unknown fields are refused, so
/// nobody can name a reporter or a storage key.
pub async fn from_multipart(mut form: Multipart) -> Result<Form, AppError> {
    let mut f = Form::default();
    while let Some(mut field) = form
        .next_field()
        .await
        .map_err(|e| bad("form", &e.body_text()))?
    {
        let name = field.name().unwrap_or_default().to_owned();
        match name.as_str() {
            "screenshot" | "photo" => {
                let mut bytes = Vec::new();
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| bad(&name, &e.body_text()))?
                {
                    if bytes.len() + chunk.len() > MAX_BYTES {
                        return Err(bad(
                            &name,
                            &format!("The {name} must not be greater than 5 MB."),
                        ));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let slot = if name == "screenshot" {
                    &mut f.screenshot
                } else {
                    &mut f.photo
                };
                *slot = Some(bytes).filter(|b| !b.is_empty());
            }
            "id" | "route" | "user_agent" | "viewport" | "description" | "actions" => {
                let text = field.text().await.map_err(|e| bad(&name, &e.body_text()))?;
                match name.as_str() {
                    "id" => {
                        f.id = Some(
                            Uuid::parse_str(text.trim())
                                .map_err(|_| bad("id", "The id must be a valid UUID."))?,
                        );
                    }
                    "route" => f.route = Some(text),
                    "user_agent" => f.user_agent = Some(text),
                    "viewport" => f.viewport = Some(text),
                    "description" => f.description = Some(text),
                    _ => f.actions = Some(text),
                }
            }
            // Not echoed: the name is client-made.
            _ => {
                return Err(AppError::Unprocessable(
                    "The upload has an unknown field.".into(),
                ));
            }
        }
    }
    Ok(f)
}
