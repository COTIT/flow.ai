//! POST /settings body: a partial object keyed by section. Each section sent is validated
//! against its field table; unknown sections and keys are dropped. Kept credentials are
//! taken out of the sections into their own list.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    http::json::{Rules, Validate},
    modules::{
        auth::token::PlainSecret,
        settings::{
            credentials::{MASK, PATHS},
            schema::{self, Reference, WRITABLE},
        },
    },
};

/// Raw body; only the writable sections are read.
#[derive(Deserialize)]
pub struct UpdateSettingsRequest(Map<String, Value>);

/// Validated sections to store, plus the uuids the service must find in this workspace.
pub struct SettingsUpdate {
    pub sections: Map<String, Value>,
    pub refs: Vec<Reference>,
    /// Credentials of the sent sections; an unsent section's are left alone.
    pub credentials: Vec<(&'static str, Change)>,
}

/// What a save does to one stored credential.
pub enum Change {
    /// The mask was sent back: keep the stored one (checked by the service).
    Keep,
    Set(PlainSecret),
    Clear,
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
        let credentials = take_credentials(&mut sections);
        Some(SettingsUpdate {
            sections,
            refs,
            credentials,
        })
    }
}

/// Take the kept credentials out of the sent sections, leaving null. In a sent section the
/// mask keeps the stored value, a string sets it, and null, blank or a missing key clears it
/// (the rest of the section resets too). An unsent section keeps its credentials.
fn take_credentials(sections: &mut Map<String, Value>) -> Vec<(&'static str, Change)> {
    let mut out = Vec::new();
    for path in PATHS {
        let Some((name, rest)) = path.split_once('.') else {
            continue;
        };
        let Some(section) = sections.get_mut(name) else {
            continue;
        };
        let slot = rest.split('.').try_fold(section, |v, k| v.get_mut(k));
        match slot.map(Value::take) {
            Some(Value::String(s)) if s == MASK => out.push((*path, Change::Keep)),
            Some(Value::String(s)) => out.push((*path, Change::Set(PlainSecret::new(s)))),
            _ => out.push((*path, Change::Clear)),
        }
    }
    out
}
