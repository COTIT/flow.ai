//! Catalog response body: Laravel ListPermissionController shape.

use serde::{Serialize, Serializer, ser::SerializeMap};

use crate::modules::permissions::catalog::{FRONTEND_ONLY, GROUPS, names};

/// `{<group>: [actions], ..., "__classification": {frontend_only, api_enforced}}`, groups in
/// catalog order and `__classification` last, as Laravel sends it.
pub struct CatalogResponse;

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
