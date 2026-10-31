//! Passport content: a typed shape (unknown keys are refused, so no farmer contact or exact
//! location can be stored), length and count limits, and the map point rounded to ~1 km.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use serde_json::Value;

use crate::{
    error::AppError,
    http::json::{Rules, Validate, is_country},
};

const SHORT: usize = 120;
const INTRO: usize = 2000;
const BODY: usize = 5000;
const QUOTE: usize = 1000;
const CAPTION: usize = 300;
const NUMBER: usize = 64;
const FACT_LABEL: usize = 60;
const MAX_MATERIALS: usize = 30;
const MAX_CERTIFICATES: usize = 20;
const MAX_BLOCKS: usize = 30;
const MAX_FACTS: usize = 20;

/// A lot's passport: header, origin, map point, materials, certificates and story blocks.
#[derive(Clone, Debug, Default, Deserialize, Serialize, ToSchema)]
#[serde(default, deny_unknown_fields)]
#[schema(description = "Passport of one lot. Unknown keys are refused.")]
pub struct Passport {
    /// Required to publish.
    #[schema(max_length = 120)]
    pub title: Option<String>,
    #[schema(max_length = 2000)]
    pub intro: Option<String>,
    /// An `image/*` file of this workspace.
    pub banner_file_id: Option<Uuid>,
    pub origin: Option<Origin>,
    pub map: Option<MapPoint>,
    #[schema(max_items = 30)]
    pub materials: Vec<Material>,
    #[schema(max_items = 20)]
    pub certificates: Vec<Certificate>,
    #[schema(max_items = 30)]
    pub story: Vec<Block>,
}

/// Where the lot comes from.
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    /// ISO 3166-1 alpha-2, stored upper case.
    #[schema(example = "TZ")]
    pub country: String,
    #[serde(default)]
    #[schema(max_length = 120)]
    pub region: Option<String>,
    #[serde(default)]
    #[schema(max_length = 120)]
    pub place: Option<String>,
}

/// A map pin, rounded to 2 decimals (about 1 km) when saved: never an exact farm point.
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MapPoint {
    #[schema(minimum = -90, maximum = 90)]
    pub lat: f64,
    #[schema(minimum = -180, maximum = 180)]
    pub lng: f64,
    #[serde(default)]
    #[schema(max_length = 120)]
    pub label: Option<String>,
}

/// One ingredient or material.
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Material {
    #[schema(max_length = 120)]
    pub name: String,
    #[serde(default)]
    #[schema(max_length = 120)]
    pub origin: Option<String>,
    /// Share in basis points (10000 = 100%).
    #[serde(default)]
    #[schema(minimum = 0, maximum = 10000)]
    pub share_bp: Option<u32>,
}

/// A certificate, optionally with its document.
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Certificate {
    #[schema(max_length = 120)]
    pub name: String,
    #[serde(default)]
    #[schema(max_length = 120)]
    pub issuer: Option<String>,
    #[serde(default)]
    #[schema(max_length = 64)]
    pub number: Option<String>,
    #[serde(default)]
    pub valid_until: Option<NaiveDate>,
    /// An `image/*` or `application/pdf` file of this workspace.
    #[serde(default)]
    pub file_id: Option<Uuid>,
}

/// One story block, by `kind`.
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    Text {
        #[serde(default)]
        title: Option<String>,
        body: String,
    },
    Image {
        /// An `image/*` file of this workspace.
        file_id: Uuid,
        #[serde(default)]
        caption: Option<String>,
    },
    Facts {
        items: Vec<Fact>,
    },
    Quote {
        text: String,
        #[serde(default)]
        by: Option<String>,
    },
}

/// A label/value pair in a facts block.
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    #[schema(max_length = 60)]
    pub label: String,
    #[schema(max_length = 120)]
    pub value: String,
}

/// What a referenced file may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileUse {
    /// `image/*` only.
    Image,
    /// `image/*` or `application/pdf`.
    Document,
}

impl FileUse {
    /// True when `content_type` suits this use.
    pub fn allows(self, content_type: &str) -> bool {
        content_type.starts_with("image/")
            || (self == Self::Document && content_type == "application/pdf")
    }
}

impl Passport {
    /// A passport read back from the database (validated when it was saved).
    pub fn from_stored(v: Value) -> Result<Self, AppError> {
        serde_json::from_value(v).map_err(|e| AppError::Internal(format!("stored passport: {e}")))
    }

    /// Every file the passport points at, with its field path and use.
    pub fn files(&self) -> Vec<(String, Uuid, FileUse)> {
        let mut out = Vec::new();
        if let Some(id) = self.banner_file_id {
            out.push(("banner_file_id".to_owned(), id, FileUse::Image));
        }
        for (i, c) in self.certificates.iter().enumerate() {
            if let Some(id) = c.file_id {
                out.push((format!("certificates.{i}.file_id"), id, FileUse::Document));
            }
        }
        for (i, b) in self.story.iter().enumerate() {
            if let Block::Image { file_id, .. } = b {
                out.push((format!("story.{i}.file_id"), *file_id, FileUse::Image));
            }
        }
        out
    }
}

/// Trimmed text: blank = None, else at most `max` chars.
fn opt(r: &mut Rules, field: &str, v: Option<String>, max: usize) -> Option<String> {
    r.opt_text(field, v, max)
}

/// Required trimmed text of at most `max` chars.
fn req(r: &mut Rules, field: &str, v: String, max: usize) -> String {
    r.text(field, Some(v), max)
}

fn count(r: &mut Rules, field: &str, n: usize, max: usize) {
    r.check(
        field,
        n <= max,
        format!("The {field} may not have more than {max} items."),
    );
}

/// Two decimals: about 1.1 km at the equator.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

impl Validate for Passport {
    type Valid = Passport;

    fn validate(self, r: &mut Rules) -> Option<Passport> {
        count(r, "materials", self.materials.len(), MAX_MATERIALS);
        count(r, "certificates", self.certificates.len(), MAX_CERTIFICATES);
        count(r, "story", self.story.len(), MAX_BLOCKS);
        let origin = self.origin.map(|o| {
            let country = o.country.trim().to_ascii_uppercase();
            r.check(
                "origin.country",
                is_country(&country),
                "The origin country is not a valid ISO 3166 code.",
            );
            Origin {
                country,
                region: opt(r, "origin.region", o.region, SHORT),
                place: opt(r, "origin.place", o.place, SHORT),
            }
        });
        let map = self.map.map(|m| {
            r.check(
                "map.lat",
                (-90.0..=90.0).contains(&m.lat),
                "The latitude must be between -90 and 90.",
            );
            r.check(
                "map.lng",
                (-180.0..=180.0).contains(&m.lng),
                "The longitude must be between -180 and 180.",
            );
            MapPoint {
                lat: round2(m.lat),
                lng: round2(m.lng),
                label: opt(r, "map.label", m.label, SHORT),
            }
        });
        let materials = self
            .materials
            .into_iter()
            .enumerate()
            .map(|(i, m)| {
                if let Some(bp) = m.share_bp {
                    r.check(
                        &format!("materials.{i}.share_bp"),
                        bp <= 10_000,
                        "The share must be between 0 and 10000 basis points.",
                    );
                }
                Material {
                    name: req(r, &format!("materials.{i}.name"), m.name, SHORT),
                    origin: opt(r, &format!("materials.{i}.origin"), m.origin, SHORT),
                    share_bp: m.share_bp,
                }
            })
            .collect();
        let certificates = self
            .certificates
            .into_iter()
            .enumerate()
            .map(|(i, c)| Certificate {
                name: req(r, &format!("certificates.{i}.name"), c.name, SHORT),
                issuer: opt(r, &format!("certificates.{i}.issuer"), c.issuer, SHORT),
                number: opt(r, &format!("certificates.{i}.number"), c.number, NUMBER),
                valid_until: c.valid_until,
                file_id: c.file_id,
            })
            .collect();
        let story = self
            .story
            .into_iter()
            .enumerate()
            .map(|(i, b)| {
                let f = |name: &str| format!("story.{i}.{name}");
                match b {
                    Block::Text { title, body } => Block::Text {
                        title: opt(r, &f("title"), title, SHORT),
                        body: req(r, &f("body"), body, BODY),
                    },
                    Block::Image { file_id, caption } => Block::Image {
                        file_id,
                        caption: opt(r, &f("caption"), caption, CAPTION),
                    },
                    Block::Facts { items } => {
                        count(r, &f("items"), items.len(), MAX_FACTS);
                        let items = items
                            .into_iter()
                            .enumerate()
                            .map(|(j, x)| Fact {
                                label: req(r, &f(&format!("items.{j}.label")), x.label, FACT_LABEL),
                                value: req(r, &f(&format!("items.{j}.value")), x.value, SHORT),
                            })
                            .collect();
                        Block::Facts { items }
                    }
                    Block::Quote { text, by } => Block::Quote {
                        text: req(r, &f("text"), text, QUOTE),
                        by: opt(r, &f("by"), by, SHORT),
                    },
                }
            })
            .collect();
        Some(Passport {
            title: opt(r, "title", self.title, SHORT),
            intro: opt(r, "intro", self.intro, INTRO),
            banner_file_id: self.banner_file_id,
            origin,
            map,
            materials,
            certificates,
            story,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_to_two_decimals() {
        assert_eq!(round2(-8.912345), -8.91);
        assert_eq!(round2(35.456789), 35.46);
        assert_eq!(round2(0.005), 0.01);
    }

    #[test]
    fn files_lists_every_reference_with_its_use() {
        let id = Uuid::now_v7();
        let p: Passport = serde_json::from_value(serde_json::json!({
            "banner_file_id": id,
            "certificates": [{ "name": "c", "file_id": id }],
            "story": [{ "kind": "quote", "text": "q" }, { "kind": "image", "file_id": id }]
        }))
        .unwrap();
        let paths: Vec<(String, FileUse)> = p.files().into_iter().map(|(f, _, u)| (f, u)).collect();
        assert_eq!(
            paths,
            [
                ("banner_file_id".to_owned(), FileUse::Image),
                ("certificates.0.file_id".to_owned(), FileUse::Document),
                ("story.1.file_id".to_owned(), FileUse::Image),
            ]
        );
        assert!(FileUse::Document.allows("application/pdf"));
        assert!(!FileUse::Image.allows("application/pdf"));
    }

    #[test]
    fn unknown_keys_are_refused_at_every_level() {
        for v in [
            serde_json::json!({ "phone": "1" }),
            serde_json::json!({ "map": { "lat": 1, "lng": 1, "exact": true } }),
            serde_json::json!({ "story": [{ "kind": "text", "body": "b", "x": 1 }] }),
        ] {
            assert!(
                serde_json::from_value::<Passport>(v.clone()).is_err(),
                "{v}"
            );
        }
    }
}
