//! Discount group request/response bodies (Laravel Store/UpdateDiscountGroupRequest,
//! Attach/DetachDiscountGroupToRelationRequest, DiscountGroupResource).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::discount_groups::repo::GroupRow,
};

const MAX: usize = 150;

/// Laravel DiscountGroupResource; `supplier` is the supplier's RelationIndexResource.
#[derive(Serialize)]
pub struct GroupResource {
    id: Uuid,
    name: String,
    active: bool,
    internal: bool,
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    relationships: Value,
}

impl GroupResource {
    /// Render `g` with its supplier (null when the relation is gone).
    pub fn new(g: GroupRow, supplier: Option<Value>) -> Self {
        Self {
            id: g.id,
            name: g.name,
            active: g.active,
            internal: g.internal,
            meta: g.meta,
            created_at: g.created_at,
            last_updated_at: g.updated_at,
            relationships: json!({ "supplier_relation": supplier }),
        }
    }
}

/// Create response.
#[derive(Serialize)]
pub struct GroupCreated {
    pub discount_group_id: Uuid,
}

/// GET list query (Laravel ListDiscountGroupRequest). Unknown params ignored.
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
    pub limit: Option<String>,
    pub active: Option<String>,
    pub internal: Option<String>,
}

/// POST /discount-groups.
#[derive(Deserialize)]
pub struct CreateGroupRequest {
    name: Option<String>,
    active: Option<Value>,
    internal: Option<Value>,
    supplier_relation_id: Option<Value>,
    meta: Option<Value>,
}

/// Validated create input.
pub struct NewGroup {
    pub name: String,
    pub active: bool,
    pub internal: bool,
    pub supplier_relation_id: Uuid,
    pub meta: Option<Value>,
}

impl Validate for CreateGroupRequest {
    type Valid = NewGroup;

    fn validate(self, r: &mut Rules) -> Option<NewGroup> {
        let name = r.text("name", self.name, MAX);
        let active = r
            .required_value("active", self.active)
            .and_then(|v| r.boolean("active", &v));
        let internal = filled(self.internal).and_then(|v| r.boolean("internal", &v));
        let supplier = r
            .required_value("supplier_relation_id", self.supplier_relation_id)
            .and_then(|v| uuid(r, "supplier_relation_id", &v));
        let meta = r.meta(self.meta).flatten();
        Some(NewGroup {
            name,
            active: active?,
            internal: internal.unwrap_or(false),
            supplier_relation_id: supplier?,
            meta,
        })
    }
}

/// POST /discount-groups/{id}: name, active, meta (Laravel UpdateDiscountGroupRequest).
#[derive(Deserialize)]
pub struct UpdateGroupRequest {
    name: Option<String>,
    active: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    meta: Option<Option<Value>>,
}

/// Validated update; `None` keeps the stored value.
pub struct GroupPatch {
    pub name: Option<String>,
    pub active: Option<bool>,
    pub meta: Option<Option<Value>>,
}

impl Validate for UpdateGroupRequest {
    type Valid = GroupPatch;

    fn validate(self, r: &mut Rules) -> Option<GroupPatch> {
        Some(GroupPatch {
            name: r.opt_text("name", self.name, MAX),
            active: filled(self.active).and_then(|v| r.boolean("active", &v)),
            meta: self.meta.and_then(|v| r.meta(v)),
        })
    }
}

/// POST /{id}/attach-to-relations and /detach-from-relations.
#[derive(Deserialize)]
pub struct RelationIdsRequest {
    relation_ids: Option<Value>,
}

/// Laravel ValidateRelations text.
pub const INVALID_RELATIONS: &str = "relation ids must be an array of valid relation_ids";

impl Validate for RelationIdsRequest {
    type Valid = Vec<Uuid>;

    fn validate(self, r: &mut Rules) -> Option<Vec<Uuid>> {
        let ids = match filled(self.relation_ids) {
            None => {
                r.missing("relation_ids");
                return None;
            }
            Some(Value::Array(a)) => a,
            Some(_) => {
                r.add("relation_ids", "The relation ids must be an array.");
                return None;
            }
        };
        let parsed: Option<Vec<Uuid>> = ids
            .iter()
            .map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
            .collect();
        if parsed.is_none() || ids.is_empty() {
            r.add("relation_ids", INVALID_RELATIONS);
        }
        parsed
    }
}

fn uuid(r: &mut Rules, field: &str, v: &Value) -> Option<Uuid> {
    let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
    if id.is_none() {
        let h = crate::http::json::human(field);
        r.add(field, format!("The {h} must be a valid UUID."));
    }
    id
}
