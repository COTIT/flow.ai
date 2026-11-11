//! POST /settings body: a partial object keyed by section. Each section sent is validated
//! against its field table; unknown sections and keys are dropped. Kept credentials are
//! taken out of the sections into their own list.

use serde::{Deserialize, Serialize};
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
#[derive(Deserialize, utoipa::ToSchema)]
#[schema(
    value_type = Object,
    description = "Sections to save, keyed by section name. Each sent section is validated and replaces the stored one; unknown sections and keys are dropped. Send a credential's mask back to keep it."
)]
pub struct UpdateSettingsRequest(Map<String, Value>);

/// Settings read reply.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SettingsData {
    /// Every section as an object, or with `?type=` that one section or dotted path (any JSON value; null when unset). Credentials print masked.
    pub data: Value,
    /// Deployment-owned payment capabilities, separate from editable workspace settings.
    pub meta: SettingsMeta,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SettingsMeta {
    pub globadyme_environment: Option<crate::payments::GlobadymeEnvironment>,
    pub production_origin_verified: bool,
}

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

#[cfg(test)]
mod deployment_metadata_tests {
    use super::*;
    use crate::payments::GlobadymeEnvironment::{Production, Sandbox};
    use serde_json::json;

    #[test]
    fn metadata_wraps_every_data_shape_without_editing_its_values() {
        for data in [
            json!({"integrations":{"payment":{"environment":"sandbox"}}}),
            json!(["path value"]),
            json!(null),
        ] {
            for (environment, expected) in [
                (Some(Sandbox), json!("sandbox")),
                (Some(Production), json!("production")),
                (None, Value::Null),
            ] {
                let reply = serde_json::to_value(SettingsData {
                    data: data.clone(),
                    meta: SettingsMeta {
                        globadyme_environment: environment,
                        production_origin_verified: false,
                    },
                })
                .unwrap();
                assert_eq!(
                    reply,
                    json!({"data":data,"meta":{"globadyme_environment":expected,"production_origin_verified":false}})
                );
            }
        }
    }

    #[test]
    fn deployment_environment_schema_and_wire_values_share_lowercase_enum() {
        let schema = serde_json::to_value(
            <crate::payments::GlobadymeEnvironment as utoipa::PartialSchema>::schema(),
        )
        .unwrap();
        assert_eq!(schema["enum"], json!(["sandbox", "production"]));
        assert_eq!(Sandbox.as_str(), "sandbox");
        assert_eq!(Production.as_str(), "production");
    }
}
