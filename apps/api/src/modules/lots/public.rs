//! The public lot page: an allow-list of the lot, its product and its published passport.
//! No internal ids, no workspace, no draft, no quantities; file ids become short-lived links.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::lots::{
    passport::{Block, Certificate, Fact, MapPoint, Material, Origin, Passport},
    repo::PublicRow,
};

/// A signed download link.
#[derive(Clone, Serialize, ToSchema)]
pub struct Link {
    url: String,
    /// Fetch before this; ask the page again for a fresh link.
    expires_at: DateTime<Utc>,
}

impl Link {
    /// `url`, valid until `expires_at`.
    pub fn new(url: String, expires_at: DateTime<Utc>) -> Self {
        Self { url, expires_at }
    }
}

/// What a scanned jar shows.
#[derive(Serialize, ToSchema)]
#[schema(description = "Public page of one lot: batch, product, published passport.")]
pub struct PublicLot {
    code: String,
    public_code: String,
    /// `released`, `quarantine` or `recalled` (a recall is shown, not hidden).
    status: String,
    #[schema(required = true)]
    produced_on: Option<NaiveDate>,
    #[schema(required = true)]
    best_before: Option<NaiveDate>,
    #[schema(required = true)]
    expires_on: Option<NaiveDate>,
    #[schema(required = true)]
    origin: Option<String>,
    product: PublicProduct,
    passport: PublicPassport,
    /// Consented farmer fields only; empty until farmer consent exists.
    farmers: Vec<PublicFarmer>,
}

/// The lot's product, public fields only.
#[derive(Serialize, ToSchema)]
pub struct PublicProduct {
    name: String,
    #[schema(required = true)]
    brand: Option<String>,
    #[schema(required = true)]
    short_description: Option<String>,
}

// shortcut: never built until consented supply links exist (Phase 4, G10); drop the allow then.
#[allow(dead_code)]
/// The farmer fields a page may ever show (allow-list by type: no contact, no exact
/// location, no money).
#[derive(Serialize, ToSchema)]
pub struct PublicFarmer {
    village: String,
    first_name: String,
    #[schema(required = true)]
    photo: Option<Link>,
    harvest_dates: Vec<NaiveDate>,
}

/// The published passport with links in place of file ids.
#[derive(Serialize, ToSchema)]
pub struct PublicPassport {
    #[schema(required = true)]
    title: Option<String>,
    #[schema(required = true)]
    intro: Option<String>,
    #[schema(required = true)]
    banner: Option<Link>,
    #[schema(required = true)]
    origin: Option<Origin>,
    #[schema(required = true)]
    map: Option<MapPoint>,
    materials: Vec<Material>,
    certificates: Vec<PublicCertificate>,
    story: Vec<PublicBlock>,
    version: i32,
    published_at: DateTime<Utc>,
}

/// A certificate with its document link (null when the file is gone).
#[derive(Serialize, ToSchema)]
pub struct PublicCertificate {
    name: String,
    #[schema(required = true)]
    issuer: Option<String>,
    #[schema(required = true)]
    number: Option<String>,
    #[schema(required = true)]
    valid_until: Option<NaiveDate>,
    #[schema(required = true)]
    file: Option<Link>,
}

/// A story block; image blocks carry a link (null when the file is gone).
#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicBlock {
    Text {
        #[schema(required = true)]
        title: Option<String>,
        body: String,
    },
    Image {
        #[schema(required = true)]
        image: Option<Link>,
        #[schema(required = true)]
        caption: Option<String>,
    },
    Facts {
        items: Vec<Fact>,
    },
    Quote {
        text: String,
        #[schema(required = true)]
        by: Option<String>,
    },
}

impl PublicLot {
    /// The page for `row` and its passport `p`; `links` maps file id to link (missing = null).
    pub fn new(row: PublicRow, p: Passport, links: &HashMap<Uuid, Link>) -> Self {
        let link = |id: Option<Uuid>| id.and_then(|id| links.get(&id).cloned());
        let certificates = p
            .certificates
            .into_iter()
            .map(|c: Certificate| PublicCertificate {
                file: link(c.file_id),
                name: c.name,
                issuer: c.issuer,
                number: c.number,
                valid_until: c.valid_until,
            })
            .collect();
        let story = p
            .story
            .into_iter()
            .map(|b| match b {
                Block::Text { title, body } => PublicBlock::Text { title, body },
                Block::Image { file_id, caption } => PublicBlock::Image {
                    image: link(Some(file_id)),
                    caption,
                },
                Block::Facts { items } => PublicBlock::Facts { items },
                Block::Quote { text, by } => PublicBlock::Quote { text, by },
            })
            .collect();
        Self {
            code: row.code,
            public_code: row.public_code,
            status: row.status,
            produced_on: row.produced_on,
            best_before: row.best_before,
            expires_on: row.expires_on,
            origin: row.origin,
            product: PublicProduct {
                name: row.product_name,
                brand: row.brand,
                short_description: row.short_description,
            },
            passport: PublicPassport {
                title: p.title,
                intro: p.intro,
                banner: link(p.banner_file_id),
                origin: p.origin,
                map: p.map,
                materials: p.materials,
                certificates,
                story,
                version: row.version,
                published_at: row.published_at,
            },
            // shortcut: no farmer data until consented supply links exist (Phase 4, G10).
            farmers: Vec::new(),
        }
    }
}
