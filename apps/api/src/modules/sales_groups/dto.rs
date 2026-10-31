//! Sales group request/response bodies. Keys follow Laravel SalesGroupResource and the
//! Store/UpdateSalesGroupRequest rules (vimdesk-web settings/sales-group).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, present},
    modules::sales_groups::repo::SalesGroupRow,
};

const MAX: usize = 150;

// Laravel SalesGroupResource.
/// Bookkeeping group with purchase and revenue ledger codes.
#[derive(Serialize, ToSchema)]
pub struct SalesGroupResource {
    id: Uuid,
    name: String,
    #[schema(required = true)]
    purchase_code: Option<String>,
    #[schema(required = true)]
    revenue_code: Option<String>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
}

impl From<SalesGroupRow> for SalesGroupResource {
    fn from(g: SalesGroupRow) -> Self {
        Self {
            id: g.id,
            name: g.name,
            purchase_code: g.purchase_code,
            revenue_code: g.revenue_code,
            created_at: g.created_at,
            last_updated_at: g.updated_at,
            meta: g.meta,
        }
    }
}

/// Create response: the new row's id.
#[derive(Serialize, ToSchema)]
pub struct SalesGroupCreated {
    pub sales_group_id: Uuid,
}

/// GET list query. Unknown params ignored.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search text.
    pub q: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// POST /settings/sales-groups.
#[derive(Deserialize, ToSchema)]
#[schema(description = "New sales group. `name` is unique per workspace.")]
pub struct CreateSalesGroupRequest {
    #[schema(value_type = String, max_length = 150)]
    name: Option<String>,
    #[schema(max_length = 150)]
    purchase_code: Option<String>,
    #[schema(max_length = 150)]
    revenue_code: Option<String>,
    #[schema(value_type = Option<Object>)]
    meta: Option<Value>,
}

/// Validated create input.
pub struct NewSalesGroup {
    pub name: String,
    pub purchase_code: Option<String>,
    pub revenue_code: Option<String>,
    pub meta: Option<Value>,
}

impl Validate for CreateSalesGroupRequest {
    type Valid = NewSalesGroup;

    fn validate(self, r: &mut Rules) -> Option<NewSalesGroup> {
        Some(NewSalesGroup {
            name: r.text("name", self.name, MAX),
            purchase_code: r.opt_text("purchase_code", self.purchase_code, MAX),
            revenue_code: r.opt_text("revenue_code", self.revenue_code, MAX),
            meta: r.meta(self.meta).flatten(),
        })
    }
}

// POST /settings/sales-groups/{id}. `name` stays required (Laravel update rules); codes and
// meta patch (absent = keep, null = clear).
/// POST /settings/sales-groups/{id}.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Changes to a sales group. `name` is always required; leave another field \
        out to keep it, send null to clear it."
)]
pub struct UpdateSalesGroupRequest {
    #[schema(value_type = String, max_length = 150)]
    name: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 150)]
    purchase_code: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 150)]
    revenue_code: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
}

/// Validated update; `None` keeps the stored value.
pub struct SalesGroupPatch {
    pub name: String,
    pub purchase_code: Option<Option<String>>,
    pub revenue_code: Option<Option<String>>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateSalesGroupRequest {
    type Valid = SalesGroupPatch;

    fn validate(self, r: &mut Rules) -> Option<SalesGroupPatch> {
        Some(SalesGroupPatch {
            name: r.text("name", self.name, MAX),
            purchase_code: r.patch_opt_text("purchase_code", self.purchase_code, MAX),
            revenue_code: r.patch_opt_text("revenue_code", self.revenue_code, MAX),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}
