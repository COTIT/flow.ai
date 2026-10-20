//! Custom status request/response bodies. Keys follow Laravel CustomStatusResource and the
//! Store/Update/ListCustomStatusRequest rules (vimdesk-web custom-statuses.service.js).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    error::AppError,
    http::{
        json::{Rules, Validate, present},
        list::Page,
    },
    modules::{custom_statuses::repo::CustomStatusRow, custom_types::dto::domain},
};

const NAME_MAX: usize = 55;
/// Laravel ListCustomStatusRequest `limit`: integer 1..200, default 50.
const LIMIT_MAX: i64 = 200;
const LIMIT_DEFAULT: i64 = 50;

/// Laravel CustomStatusResource; `archived` is a JSON bool (Laravel sent 0/1).
#[derive(Serialize)]
pub struct CustomStatusResource {
    id: Uuid,
    name: String,
    domain: String,
    archived: bool,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    meta: Option<Value>,
}

impl From<CustomStatusRow> for CustomStatusResource {
    fn from(s: CustomStatusRow) -> Self {
        Self {
            id: s.id,
            name: s.name,
            domain: s.domain,
            archived: s.archived,
            created_at: s.created_at,
            last_updated_at: s.updated_at,
            meta: s.meta,
        }
    }
}

/// Create response: the new row's id.
#[derive(Serialize)]
pub struct CustomStatusCreated {
    pub custom_status_id: Uuid,
}

/// GET list query. Laravel paged by `limit`, not `page_size`. Unknown params ignored.
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub domain: Option<String>,
    pub limit: Option<String>,
    pub page: Option<String>,
}

impl ListQuery {
    /// Page sized by `limit` (1..200, default 50); 422 outside that range.
    pub fn page(&self) -> Result<Page, AppError> {
        let number = Page::parse(self.page.as_deref(), None).number;
        let Some(limit) = self.limit.as_deref().filter(|l| !l.trim().is_empty()) else {
            return Ok(Page {
                number,
                size: LIMIT_DEFAULT,
            });
        };
        let mut r = Rules::default();
        let size = r.integer("limit", &Value::String(limit.to_owned()), 1, LIMIT_MAX);
        r.into_result()?;
        Ok(Page {
            number,
            size: size.unwrap_or(LIMIT_DEFAULT),
        })
    }
}

/// POST /custom-status.
#[derive(Deserialize)]
pub struct CreateCustomStatusRequest {
    name: Option<String>,
    domain: Option<String>,
    meta: Option<Value>,
}

/// Validated create input.
pub struct NewCustomStatus {
    pub name: String,
    pub domain: String,
    pub meta: Option<Value>,
}

impl Validate for CreateCustomStatusRequest {
    type Valid = NewCustomStatus;

    fn validate(self, r: &mut Rules) -> Option<NewCustomStatus> {
        Some(NewCustomStatus {
            name: r.text("name", self.name, NAME_MAX),
            domain: domain(r, self.domain),
            meta: r.meta(self.meta).flatten(),
        })
    }
}

/// POST /custom-status/{id}. Domain and archived can't change here (Laravel rules).
#[derive(Deserialize)]
pub struct UpdateCustomStatusRequest {
    #[serde(default, deserialize_with = "present")]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    meta: Option<Option<Value>>,
}

/// Validated update; `None` keeps the stored value.
pub struct CustomStatusPatch {
    pub name: Option<String>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateCustomStatusRequest {
    type Valid = CustomStatusPatch;

    fn validate(self, r: &mut Rules) -> Option<CustomStatusPatch> {
        Some(CustomStatusPatch {
            name: r.patch_text("name", self.name, NAME_MAX),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}
