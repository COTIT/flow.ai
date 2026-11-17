//! Mapping bodies (contracts §5.2).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    integrations::mappings::repo::MappingRow,
};

pub const ENTITIES: &[&str] = &[
    "company", "currency", "location", "product", "uom", "user", "vendor",
];

#[derive(Serialize, ToSchema)]
#[schema(description = "Source id → target id for one entity of a pair.")]
pub struct MappingResource {
    id: Uuid,
    pair_id: Uuid,
    entity: String,
    source_id: String,
    target_id: String,
    #[schema(required = true)]
    source_label: Option<String>,
    #[schema(required = true)]
    target_label: Option<String>,
    updated_at: DateTime<Utc>,
}

impl From<MappingRow> for MappingResource {
    fn from(m: MappingRow) -> Self {
        Self {
            id: m.id,
            pair_id: m.pair_id,
            entity: m.entity,
            source_id: m.source_id,
            target_id: m.target_id,
            source_label: m.source_label,
            target_label: m.target_label,
            updated_at: m.updated_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct MappingCreated {
    pub mapping_id: Uuid,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MappingQuery {
    /// One entity only.
    pub entity: Option<String>,
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[schema(
    description = "Upsert by (entity, source_id). For `user`, target_id is an active member's user id."
)]
pub struct MappingInput {
    #[schema(value_type = String, example = "vendor")]
    entity: Option<String>,
    #[schema(value_type = String, max_length = 100)]
    source_id: Option<String>,
    #[schema(value_type = String, max_length = 100)]
    target_id: Option<String>,
    #[schema(max_length = 200)]
    source_label: Option<String>,
    #[schema(max_length = 200)]
    target_label: Option<String>,
}

/// Validated mapping.
pub struct NewMapping {
    pub entity: String,
    pub source_id: String,
    pub target_id: String,
    pub source_label: Option<String>,
    pub target_label: Option<String>,
}

impl Validate for MappingInput {
    type Valid = NewMapping;

    fn validate(self, r: &mut Rules) -> Option<NewMapping> {
        let entity = self
            .entity
            .map(|e| e.trim().to_owned())
            .filter(|e| !e.is_empty());
        match &entity {
            None => r.missing("entity"),
            Some(e) => r.check(
                "entity",
                ENTITIES.contains(&e.as_str()),
                "The selected entity is invalid.",
            ),
        }
        let source_id = r.text("source_id", self.source_id, 100);
        let target_id = r.text("target_id", self.target_id, 100);
        let source_label = r.opt_text("source_label", self.source_label, 200);
        let target_label = r.opt_text("target_label", self.target_label, 200);
        Some(NewMapping {
            entity: entity?,
            source_id,
            target_id,
            source_label,
            target_label,
        })
    }
}
