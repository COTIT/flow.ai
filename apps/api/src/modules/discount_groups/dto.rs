//! Discount group request/response bodies.
// Laravel Store/UpdateDiscountGroupRequest, Attach/DetachDiscountGroupToRelationRequest,
// DiscountGroupResource.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::discount_groups::repo::GroupRow,
};

const MAX: usize = 150;

// Laravel DiscountGroupResource; `supplier` is the supplier's RelationIndexResource.
#[derive(Serialize, ToSchema)]
#[schema(description = "A supplier discount group.")]
pub struct GroupResource {
    id: Uuid,
    name: String,
    active: bool,
    internal: bool,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
    /// `supplier_relation`: the supplier (relation index shape), or null when it is gone.
    #[schema(value_type = Object)]
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
#[derive(Serialize, ToSchema)]
pub struct GroupCreated {
    pub discount_group_id: Uuid,
}

/// GET list query. Unknown params ignored.
// Laravel ListDiscountGroupRequest.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search on the name.
    pub q: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// 1 to 100: a plain list of that many, without `meta`. 0 or blank = paged.
    #[param(value_type = Option<i64>, minimum = 0, maximum = 100)]
    pub limit: Option<String>,
    #[param(value_type = Option<bool>)]
    pub active: Option<String>,
    #[param(value_type = Option<bool>)]
    pub internal: Option<String>,
}

/// POST /discount-groups.
#[derive(Deserialize, ToSchema)]
#[schema(as = CreateDiscountGroupRequest)]
pub struct CreateGroupRequest {
    #[schema(value_type = String, max_length = 150)]
    name: Option<String>,
    #[schema(value_type = bool)]
    active: Option<Value>,
    #[schema(value_type = Option<bool>)]
    internal: Option<Value>,
    #[schema(value_type = Uuid)]
    supplier_relation_id: Option<Value>,
    #[schema(value_type = Option<Object>)]
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

/// POST /discount-groups/{id}: name, active, meta.
// Laravel UpdateDiscountGroupRequest.
#[derive(Deserialize, ToSchema)]
#[schema(as = UpdateDiscountGroupRequest)]
pub struct UpdateGroupRequest {
    #[schema(value_type = Option<String>)]
    name: Option<String>,
    #[schema(value_type = Option<bool>)]
    active: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
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
#[derive(Deserialize, ToSchema)]
#[schema(as = DiscountGroupRelationsRequest)]
pub struct RelationIdsRequest {
    #[schema(value_type = Vec<Uuid>)]
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

/// List response: a page, or with `?limit=` a plain list without `meta`.
#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as = DiscountGroupList)]
pub struct GroupListBody {
    data: Vec<GroupResource>,
    /// Left out with `?limit=`.
    meta: Option<crate::http::list::Meta>,
}
