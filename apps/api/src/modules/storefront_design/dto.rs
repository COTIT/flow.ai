//! Home design schema, capability discovery and revision response contracts.

use crate::{
    AppError,
    http::json::{Rules, Validate},
    modules::storefront::dto::StorefrontProduct,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use utoipa::{IntoParams, PartialSchema, ToSchema};
use uuid::Uuid;

/// Current document version; unknown versions need explicit migration.
pub const SCHEMA_VERSION: u8 = 1;
/// Maximum canonical document bytes.
pub const DOCUMENT_BYTES: usize = 32768;

/// Only home is configurable in phase one.
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StorefrontDesignPage {
    Home,
}
/// Reviewed, contrast-safe presentation presets.
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StorefrontDesignTheme {
    Forest,
    Neutral,
}
/// Reviewed hero composition.
#[derive(Clone, Default, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StorefrontHeroLayout {
    Centered,
    #[default]
    Split,
}
/// Only existing public catalog data may supply products.
#[derive(Clone, Default, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StorefrontDesignSource {
    #[default]
    Catalog,
}

/// Plain-text hero; CTA always opens `/shop`.
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StorefrontHeroProps {
    #[schema(min_length = 1, max_length = 160)]
    pub heading: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(max_length = 1000)]
    pub body: Option<String>,
    #[serde(default)]
    pub layout: StorefrontHeroLayout,
}
/// Fixed responsive product columns.
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StorefrontDesignColumns {
    #[schema(minimum = 1, maximum = 2)]
    pub mobile: u8,
    #[schema(minimum = 1, maximum = 3)]
    pub tablet: u8,
    #[schema(minimum = 1, maximum = 4)]
    pub desktop: u8,
}
impl Default for StorefrontDesignColumns {
    fn default() -> Self {
        Self {
            mobile: 1,
            tablet: 2,
            desktop: 3,
        }
    }
}
fn grid_limit() -> u8 {
    6
}
/// Bounded collection binding; prices and inventory are never design props.
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StorefrontProductGridProps {
    #[schema(min_length = 1, max_length = 160)]
    pub heading: String,
    #[serde(default)]
    pub source: StorefrontDesignSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_id: Option<Uuid>,
    #[serde(default = "grid_limit")]
    #[schema(minimum = 1, maximum = 12, default = 6)]
    pub limit: u8,
    #[serde(default)]
    pub columns: StorefrontDesignColumns,
}
/// Closed, flat section union; no HTML, scripts, media URLs or arbitrary CSS.
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StorefrontDesignSection {
    #[schema(max_properties = 3)]
    Hero {
        #[schema(min_length = 1, max_length = 64, pattern = "^[a-z][a-z0-9-]{0,63}$")]
        id: String,
        props: StorefrontHeroProps,
    },
    #[schema(max_properties = 3)]
    ProductGrid {
        #[schema(min_length = 1, max_length = 64, pattern = "^[a-z][a-z0-9-]{0,63}$")]
        id: String,
        props: StorefrontProductGridProps,
    },
}
impl StorefrontDesignSection {
    /// Stable section identity.
    pub fn id(&self) -> &str {
        match self {
            Self::Hero { id, .. } | Self::ProductGrid { id, .. } => id,
        }
    }
}
/// Versioned presentation document; Rust defines shared OpenAPI/TypeScript shape.
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StorefrontDesignDocument {
    #[schema(minimum = 1, maximum = 1)]
    pub schema_version: u8,
    pub page: StorefrontDesignPage,
    pub theme: StorefrontDesignTheme,
    #[schema(min_items = 1, max_items = 12)]
    pub sections: Vec<StorefrontDesignSection>,
}
impl StorefrontDesignDocument {
    /// Validate complete document, including limits across sections.
    pub fn check(&self) -> Result<(), AppError> {
        let mut r = Rules::default();
        r.check(
            "document.schema_version",
            self.schema_version == SCHEMA_VERSION,
            "Unsupported design schema version.",
        );
        r.check(
            "document.sections",
            (1..=12).contains(&self.sections.len()),
            "Use 1 to 12 sections.",
        );
        let bytes = serde_json::to_vec(self)
            .map_err(|_| AppError::Internal("design serialization failed".into()))?;
        r.check(
            "document",
            bytes.len() <= DOCUMENT_BYTES,
            "Design document exceeds 32 KiB.",
        );
        let (mut heroes, mut grids) = (0, 0);
        let mut ids = BTreeSet::new();
        for (i, s) in self.sections.iter().enumerate() {
            let id = s.id();
            let slug = (1..=64).contains(&id.len())
                && id.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-');
            r.check(
                &format!("document.sections.{i}.id"),
                slug && ids.insert(id),
                "Use a unique lowercase slug, 1 to 64 characters.",
            );
            let heading = match s {
                StorefrontDesignSection::Hero { props, .. } => {
                    heroes += 1;
                    if let Some(body) = &props.body {
                        r.max_chars(&format!("document.sections.{i}.props.body"), body, 1000);
                    }
                    &props.heading
                }
                StorefrontDesignSection::ProductGrid { props, .. } => {
                    grids += 1;
                    r.check(
                        &format!("document.sections.{i}.props.limit"),
                        (1..=12).contains(&props.limit),
                        "Use 1 to 12 products.",
                    );
                    let c = &props.columns;
                    r.check(
                        &format!("document.sections.{i}.props.columns"),
                        (1..=2).contains(&c.mobile)
                            && (1..=3).contains(&c.tablet)
                            && (1..=4).contains(&c.desktop),
                        "Use supported responsive columns.",
                    );
                    &props.heading
                }
            };
            r.check(
                &format!("document.sections.{i}.props.heading"),
                !heading.trim().is_empty(),
                "Heading is required.",
            );
            r.max_chars(
                &format!("document.sections.{i}.props.heading"),
                heading,
                160,
            );
        }
        r.check(
            "document.sections",
            heroes <= 1 && grids <= 4,
            "Use at most one hero and four product grids.",
        );
        r.into_result()
    }
    /// Decode stored documents through same trust boundary as new drafts.
    pub fn from_value(value: Value) -> Result<Self, AppError> {
        let doc: Self = serde_json::from_value(value)
            .map_err(|_| AppError::field("document", "Stored design document is unsupported."))?;
        doc.check()?;
        Ok(doc)
    }
}
/// New immutable draft, guarded by page version.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StorefrontDesignSave {
    #[schema(minimum = 0)]
    pub expected_version: i64,
    pub document: StorefrontDesignDocument,
}
impl Validate for StorefrontDesignSave {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        r.check(
            "expected_version",
            self.expected_version >= 0,
            "Version must be nonnegative.",
        );
        match self.document.check() {
            Ok(()) => {}
            Err(AppError::Validation(errors)) => {
                for (field, messages) in errors {
                    for message in messages {
                        r.add(&field, message);
                    }
                }
            }
            Err(_) => r.add("document", "Invalid design document."),
        }
        Some(self)
    }
}
/// Explicit restore or publish selection.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StorefrontDesignSelect {
    #[schema(minimum = 0)]
    pub expected_version: i64,
    pub revision_id: Uuid,
}
impl Validate for StorefrontDesignSelect {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        r.check(
            "expected_version",
            self.expected_version >= 0,
            "Version must be nonnegative.",
        );
        Some(self)
    }
}
/// Most targeted changes one edit may carry.
pub const MAX_CHANGES: usize = 10;

/// One targeted change to current draft; closed list of editable properties.
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum StorefrontDesignChange {
    /// Switch reviewed theme preset.
    SetTheme { theme: StorefrontDesignTheme },
    /// Change hero text or layout; omitted properties stay.
    SetHero {
        section_id: String,
        #[serde(default)]
        #[schema(min_length = 1, max_length = 160)]
        heading: Option<String>,
        #[serde(default)]
        #[schema(min_length = 1, max_length = 1000)]
        body: Option<String>,
        #[serde(default)]
        layout: Option<StorefrontHeroLayout>,
    },
    /// Remove hero body text.
    ClearHeroBody { section_id: String },
    /// Change product grid heading, category, size or columns; omitted properties stay.
    SetProductGrid {
        section_id: String,
        #[serde(default)]
        #[schema(min_length = 1, max_length = 160)]
        heading: Option<String>,
        #[serde(default)]
        category_id: Option<Uuid>,
        #[serde(default)]
        #[schema(minimum = 1, maximum = 12)]
        limit: Option<u8>,
        #[serde(default)]
        columns: Option<StorefrontDesignColumns>,
    },
    /// Grid shows all categories again.
    ShowAllProducts { section_id: String },
    /// Insert section; `position` 1 is first, absent appends.
    AddSection {
        section: StorefrontDesignSection,
        #[serde(default)]
        #[schema(minimum = 1, maximum = 12)]
        position: Option<u8>,
    },
    /// Drop section; at least one must remain.
    RemoveSection { section_id: String },
    /// Move section to 1-based position.
    MoveSection {
        section_id: String,
        #[schema(minimum = 1, maximum = 12)]
        position: u8,
    },
}
/// Targeted edit of current draft, guarded by page version and draft identity.
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StorefrontDesignEdit {
    #[schema(minimum = 0)]
    pub expected_version: i64,
    /// Current draft revision; absent only while page has no draft.
    #[serde(default)]
    pub base_revision_id: Option<Uuid>,
    #[schema(min_items = 1, max_items = 10)]
    pub changes: Vec<StorefrontDesignChange>,
}
impl Validate for StorefrontDesignEdit {
    type Valid = Self;
    fn validate(self, r: &mut Rules) -> Option<Self> {
        r.check(
            "expected_version",
            self.expected_version >= 0,
            "Version must be nonnegative.",
        );
        r.check(
            "changes",
            (1..=MAX_CHANGES).contains(&self.changes.len()),
            "Use 1 to 10 changes.",
        );
        Some(self)
    }
}
/// One field-level difference shown on review cards; `—` marks removed value.
#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct StorefrontDesignChangeRow {
    pub field: String,
    #[schema(required = true)]
    pub label: Option<String>,
    #[schema(required = true)]
    pub before: Option<String>,
    pub after: String,
    /// Category id dispatch stores for `category` rows; `after` carries its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_id: Option<Uuid>,
}
/// Dry run of targeted edit; nothing stored.
#[derive(Serialize, ToSchema)]
pub struct StorefrontDesignEditPreview {
    pub version: i64,
    #[schema(required = true)]
    pub base_revision_id: Option<Uuid>,
    pub document: StorefrontDesignDocument,
    pub collections: Vec<StorefrontDesignCollection>,
    pub changes: Vec<StorefrontDesignChangeRow>,
}
/// Base for first edit on page without draft: registry examples.
pub fn starter() -> StorefrontDesignDocument {
    StorefrontDesignDocument {
        schema_version: SCHEMA_VERSION,
        page: StorefrontDesignPage::Home,
        theme: StorefrontDesignTheme::Forest,
        sections: vec![
            StorefrontDesignSection::Hero {
                id: "hero-main".into(),
                props: StorefrontHeroProps {
                    heading: "From farm to your table".into(),
                    body: None,
                    layout: StorefrontHeroLayout::Split,
                },
            },
            StorefrontDesignSection::ProductGrid {
                id: "featured-products".into(),
                props: StorefrontProductGridProps {
                    heading: "Our products".into(),
                    source: StorefrontDesignSource::Catalog,
                    category_id: None,
                    limit: grid_limit(),
                    columns: StorefrontDesignColumns::default(),
                },
            },
        ],
    }
}
/// Immutable revision content.
#[derive(Serialize, ToSchema)]
pub struct StorefrontDesignRevision {
    pub revision_id: Uuid,
    pub document: StorefrontDesignDocument,
}
/// Staff page state; absent initial page has version zero.
#[derive(Serialize, ToSchema, Default)]
pub struct StorefrontDesignState {
    pub version: i64,
    #[schema(required = true)]
    pub draft_revision_id: Option<Uuid>,
    #[schema(required = true)]
    pub published_revision_id: Option<Uuid>,
    #[schema(required = true)]
    pub draft: Option<StorefrontDesignRevision>,
}
/// Bounded catalog data supplied outside presentation JSONB.
#[derive(Serialize, ToSchema)]
pub struct StorefrontDesignCollection {
    pub section_id: String,
    pub products: Vec<StorefrontProduct>,
}
/// Public payload; draft state and actor snapshots never included.
#[derive(Serialize, ToSchema)]
pub struct StorefrontPublishedDesign {
    pub revision_id: Uuid,
    pub document: StorefrontDesignDocument,
    pub collections: Vec<StorefrontDesignCollection>,
}
/// Authorized preview carries concurrency state alongside same renderer props.
#[derive(Serialize, ToSchema)]
pub struct StorefrontDesignPreview {
    pub version: i64,
    pub revision_id: Uuid,
    pub document: StorefrontDesignDocument,
    pub collections: Vec<StorefrontDesignCollection>,
}
/// History metadata without multiplying full document responses.
#[derive(Serialize, ToSchema)]
pub struct StorefrontDesignRevisionSummary {
    pub revision_id: Uuid,
    /// Draft this revision replaced; null for first revision. Undo restores it.
    #[schema(required = true)]
    pub parent_revision_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub created_by: Uuid,
    #[schema(required = true)]
    pub restored_from_revision_id: Option<Uuid>,
    pub was_published: bool,
}
/// Preview only accepts an explicit owned revision.
#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in=Query)]
pub struct PreviewQuery {
    pub revision_id: Uuid,
}
/// Bounded design history query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in=Query)]
pub struct HistoryQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// Registry document generated from Rust schema definitions, never independent TS schema.
pub fn capabilities() -> Result<Value, AppError> {
    let mut nested = Vec::new();
    StorefrontDesignDocument::schemas(&mut nested);
    StorefrontDesignEdit::schemas(&mut nested);
    let mut schema = serde_json::to_value(StorefrontDesignDocument::schema())
        .map_err(|_| AppError::Internal("design schema serialization failed".into()))?;
    let mut edit = serde_json::to_value(StorefrontDesignEdit::schema())
        .map_err(|_| AppError::Internal("design schema serialization failed".into()))?;
    let mut defs = serde_json::Map::new();
    for (name, s) in nested {
        defs.insert(
            name,
            serde_json::to_value(s)
                .map_err(|_| AppError::Internal("design schema serialization failed".into()))?,
        );
    }
    fn refs(v: &mut Value) {
        match v {
            Value::Object(map) => {
                if let Some(Value::String(s)) = map.get_mut("$ref") {
                    *s = s.replace("#/components/schemas/", "#/$defs/");
                }
                for child in map.values_mut() {
                    refs(child);
                }
            }
            Value::Array(a) => {
                for child in a {
                    refs(child);
                }
            }
            _ => {}
        }
    }
    edit["$defs"] = Value::Object(defs.clone());
    refs(&mut edit);
    edit["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    schema["$defs"] = Value::Object(defs);
    refs(&mut schema);
    schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    schema["properties"]["sections"]["allOf"] = json!([
     {"contains":{"properties":{"type":{"const":"hero"}},"required":["type"]},"minContains":0,"maxContains":1},
     {"contains":{"properties":{"type":{"const":"product_grid"}},"required":["type"]},"minContains":0,"maxContains":4}
    ]);
    Ok(
        json!({"schema_version":SCHEMA_VERSION,"document_schema":schema,"edit_schema":edit,"constraints":{"max_document_bytes":DOCUMENT_BYTES,"unique_section_ids":true,"max_hero":1,"max_product_grid":4,"max_changes":MAX_CHANGES},"components":[
        {"id":"hero","version":1,"purpose":"Introduce storefront","description":"Plain heading and body with fixed Shop CTA.","properties_schema":"#/$defs/StorefrontHeroProps","editable":["heading","body","layout"],"allowed_children":[],"responsive_behavior":"Centered or split; stacks on mobile.","data_requirements":[],"runtime_capabilities":[],"accessibility":["Page heading hierarchy","Plain text","Keyboard-accessible CTA"],"defaults":{"layout":"split"},"examples":[{"id":"hero-main","type":"hero","props":{"heading":"From farm to your table"}}],"deprecated":false,"deprecation":null},
        {"id":"product_grid","version":1,"purpose":"Browse sellable products","description":"Live public catalog cards; business data stays outside design document.","properties_schema":"#/$defs/StorefrontProductGridProps","editable":["heading","category_id","limit","columns"],"allowed_children":[],"responsive_behavior":"Explicit mobile, tablet and desktop columns.","data_requirements":["catalog"],"runtime_capabilities":["catalog:read"],"accessibility":["Section heading","Named product links","Image alternative text"],"defaults":{"source":"catalog","limit":6,"columns":{"mobile":1,"tablet":2,"desktop":3}},"examples":[{"id":"featured-products","type":"product_grid","props":{"heading":"Our products"}}],"deprecated":false,"deprecation":null}
        ]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/storefront-design-v1.json"
        ))
        .unwrap()
    }

    #[test]
    fn document_schema_and_runtime_reject_unsafe_or_unknown_props() {
        let registry = capabilities().unwrap();
        let schema = &registry["document_schema"];
        let validator = jsonschema::validator_for(schema).unwrap();
        let good = fixture();
        assert!(validator.is_valid(&good), "{schema}");
        assert!(StorefrontDesignDocument::from_value(good.clone()).is_ok());
        for path in ["root", "section", "props", "columns"] {
            let mut bad = good.clone();
            match path {
                "root" => bad["script"] = json!("alert(1)"),
                "section" => bad["sections"][0]["children"] = json!([]),
                "props" => bad["sections"][0]["props"]["href"] = json!("javascript:alert(1)"),
                _ => bad["sections"][1]["props"]["columns"]["raw_css"] = json!("position:fixed"),
            }
            assert!(
                StorefrontDesignDocument::from_value(bad.clone()).is_err(),
                "{path}"
            );
            assert!(!validator.is_valid(&bad), "{path}");
        }
    }

    #[test]
    fn edit_schema_accepts_targeted_changes_and_rejects_documents() {
        let registry = capabilities().unwrap();
        let validator = jsonschema::validator_for(&registry["edit_schema"]).unwrap();
        let good = json!({"expected_version":0,"changes":[
            {"op":"set_theme","theme":"neutral"},
            {"op":"set_product_grid","section_id":"featured-products","limit":4},
            {"op":"add_section","section":{"id":"more","type":"product_grid","props":{"heading":"More"}},"position":1}
        ]});
        assert!(validator.is_valid(&good), "{}", registry["edit_schema"]);
        assert!(serde_json::from_value::<StorefrontDesignEdit>(good).is_ok());
        for bad in [
            json!({"expected_version":0,"document":fixture(),"changes":[{"op":"set_theme","theme":"neutral"}]}),
            json!({"expected_version":0,"changes":[{"op":"replace_document","document":fixture()}]}),
            json!({"expected_version":0,"changes":[]}),
        ] {
            assert!(!validator.is_valid(&bad), "{bad}");
        }
        assert_eq!(
            registry["components"][0]["editable"],
            json!(["heading", "body", "layout"])
        );
        assert_eq!(registry["constraints"]["max_changes"], 10);
    }

    #[test]
    fn section_counts_defaults_ids_and_bounds_are_enforced() {
        let mut doc = fixture();
        doc["sections"][0]["props"]
            .as_object_mut()
            .unwrap()
            .remove("layout");
        doc["sections"][1]["props"]
            .as_object_mut()
            .unwrap()
            .remove("limit");
        let normalized =
            serde_json::to_value(StorefrontDesignDocument::from_value(doc).unwrap()).unwrap();
        assert_eq!(normalized["sections"][0]["props"]["layout"], "split");
        assert_eq!(normalized["sections"][1]["props"]["limit"], 6);
        let schema = capabilities().unwrap()["document_schema"].clone();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let mut many = fixture();
        let mut hero = many["sections"][0].clone();
        hero["id"] = json!("second-hero");
        many["sections"].as_array_mut().unwrap().push(hero);
        assert!(StorefrontDesignDocument::from_value(many.clone()).is_err());
        assert!(!validator.is_valid(&many));
        let mut duplicate = fixture();
        duplicate["sections"][1]["id"] = json!("hero-main");
        assert!(StorefrontDesignDocument::from_value(duplicate).is_err());
        let mut bounds = fixture();
        bounds["sections"][1]["props"]["columns"]["desktop"] = json!(5);
        assert!(StorefrontDesignDocument::from_value(bounds.clone()).is_err());
        assert!(!validator.is_valid(&bounds));
        let mut boundary = fixture();
        boundary["sections"][0]["props"]["heading"] = json!("é".repeat(160));
        boundary["sections"][0]["props"]["body"] = json!("🌱".repeat(1000));
        boundary["sections"][1]["props"]["limit"] = json!(12);
        boundary["sections"][1]["props"]["columns"] = json!({"mobile":2,"tablet":3,"desktop":4});
        assert!(StorefrontDesignDocument::from_value(boundary.clone()).is_ok());
        assert!(validator.is_valid(&boundary));
        boundary["sections"][0]["props"]["body"] = json!("x".repeat(DOCUMENT_BYTES + 1));
        assert!(StorefrontDesignDocument::from_value(boundary).is_err());
    }
}
