//! Catalog response body: Laravel ListPermissionController shape.

use serde::{Serialize, Serializer, ser::SerializeMap};
use utoipa::{
    PartialSchema, ToSchema,
    openapi::{
        RefOr,
        schema::{ArrayBuilder, ObjectBuilder, Schema, Type},
    },
};

use crate::modules::permissions::catalog::{FRONTEND_ONLY, GROUPS, names};

/// `{<group>: [actions], ..., "__classification": {frontend_only, api_enforced}}`, groups in
/// catalog order and `__classification` last, as Laravel sends it.
pub struct CatalogResponse;

/// Hand-written: the body is a map serialized by hand, so derive can't describe it.
impl PartialSchema for CatalogResponse {
    fn schema() -> RefOr<Schema> {
        let names = || ArrayBuilder::new().items(ObjectBuilder::new().schema_type(Type::String));
        ObjectBuilder::new()
            .description(Some(
                "Permission groups: each key is a group, its value the group's actions \
                 (permission name = `group.action`). `__classification` sorts the names by \
                 who enforces them.",
            ))
            .property(
                "__classification",
                ObjectBuilder::new()
                    .property("frontend_only", names())
                    .property("api_enforced", names())
                    .required("frontend_only")
                    .required("api_enforced"),
            )
            .required("__classification")
            .additional_properties(Some(names()))
            .into()
    }
}

impl ToSchema for CatalogResponse {}

#[derive(Serialize)]
struct Classification {
    frontend_only: [&'static str; 3],
    /// Every catalog name minus the frontend-only ones.
    api_enforced: Vec<String>,
}

impl Serialize for CatalogResponse {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(GROUPS.len() + 1))?;
        for (group, actions) in GROUPS {
            map.serialize_entry(group, actions)?;
        }
        map.serialize_entry(
            "__classification",
            &Classification {
                frontend_only: FRONTEND_ONLY,
                // No-op today (frontend-only names aren't in GROUPS); kept so a regenerated
                // catalog that lists them still classifies like Laravel.
                api_enforced: names()
                    .filter(|n| !FRONTEND_ONLY.contains(&n.as_str()))
                    .collect(),
            },
        )?;
        map.end()
    }
}
