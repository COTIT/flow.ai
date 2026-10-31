//! Custom status request/response bodies. Keys follow Laravel CustomStatusResource and the
//! Store/Update/ListCustomStatusRequest rules (vimdesk-web custom-statuses.service.js).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
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

// Laravel CustomStatusResource; `archived` is a JSON bool (Laravel sent 0/1).
/// A user-defined status for one domain.
#[derive(Serialize, ToSchema)]
pub struct CustomStatusResource {
    id: Uuid,
    name: String,
    /// Domain the status belongs to, lowercase.
    #[schema(example = "deal")]
    domain: String,
    archived: bool,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
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
#[derive(Serialize, ToSchema)]
pub struct CustomStatusCreated {
    pub custom_status_id: Uuid,
}

// GET list query. Laravel paged by `limit`, not `page_size`. Unknown params ignored.
/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search on name.
    pub q: Option<String>,
    /// Only this domain, any case; unknown domain is 422.
    pub domain: Option<String>,
    /// Page size 1..=200, default 50; anything else is 422.
    #[param(value_type = Option<i64>)]
    pub limit: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
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
#[derive(Deserialize, ToSchema)]
#[schema(description = "New custom status. `name` is unique within its domain.")]
pub struct CreateCustomStatusRequest {
    #[schema(value_type = String, max_length = 55)]
    name: Option<String>,
    /// Domain, any case.
    #[schema(value_type = String, example = "deal")]
    domain: Option<String>,
    #[schema(value_type = Option<Object>)]
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

// POST /custom-status/{id}. Domain and archived can't change here (Laravel rules).
/// POST /custom-status/{id}.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Changes to a custom status. Domain and `archived` can't change here; leave \
        a field out to keep it."
)]
pub struct UpdateCustomStatusRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = String, required = false, max_length = 55)]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
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
