//! Sales group request/response bodies. Keys follow Laravel SalesGroupResource and the
//! Store/UpdateSalesGroupRequest rules (vimdesk-web settings/sales-group).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, present},
    modules::sales_groups::repo::SalesGroupRow,
};

const MAX: usize = 150;

/// Laravel SalesGroupResource.
#[derive(Serialize)]
pub struct SalesGroupResource {
    id: Uuid,
    name: String,
    purchase_code: Option<String>,
    revenue_code: Option<String>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
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

#[derive(Serialize)]
pub struct SalesGroupCreated {
    pub sales_group_id: Uuid,
}

/// GET list query. Unknown params ignored.
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// POST /settings/sales-groups.
#[derive(Deserialize)]
pub struct CreateSalesGroupRequest {
    name: Option<String>,
    purchase_code: Option<String>,
    revenue_code: Option<String>,
    meta: Option<Value>,
}

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

/// POST /settings/sales-groups/{id}. `name` stays required (Laravel update rules); codes and
/// meta patch (absent = keep, null = clear).
#[derive(Deserialize)]
pub struct UpdateSalesGroupRequest {
    name: Option<String>,
    #[serde(default, deserialize_with = "present")]
    purchase_code: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    revenue_code: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    meta: Option<Option<Value>>,
}

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
