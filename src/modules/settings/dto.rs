//! POST /settings body: a partial object keyed by section. Each section sent is validated
//! against its field table; unknown sections and keys are dropped.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    http::json::{Rules, Validate},
    modules::settings::schema::{self, Reference, WRITABLE},
};

/// Raw body; only the writable sections are read.
#[derive(Deserialize)]
pub struct UpdateSettingsRequest(Map<String, Value>);

/// Validated sections to store, plus the uuids the service must find in this workspace.
pub struct SettingsUpdate {
    pub sections: Map<String, Value>,
    pub refs: Vec<Reference>,
}

impl Validate for UpdateSettingsRequest {
    type Valid = SettingsUpdate;

    fn validate(mut self, r: &mut Rules) -> Option<SettingsUpdate> {
        let mut sections = Map::new();
        let mut refs = Vec::new();
        for (name, fields) in WRITABLE {
            if let Some(v) = self.0.get(*name) {
                let kept = schema::section(r, name, fields, v, &mut refs);
                sections.insert((*name).to_owned(), kept);
            }
        }
        let meta = self.0.remove("meta");
        if meta.is_some()
            && let Some(meta) = r.meta(meta)
        {
            sections.insert("meta".to_owned(), meta.unwrap_or(Value::Null));
        }
        Some(SettingsUpdate { sections, refs })
    }
}
