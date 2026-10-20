//! Brand bodies: Laravel ProductBrandResource and Store/UpdateProductBrandRequest.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human, is_url, present},
    modules::product_brands::repo::BrandRow,
};

/// Laravel ProductBrandResource.
#[derive(Serialize)]
pub struct BrandResource {
    id: Uuid,
    product_brand_id: Uuid,
    name: String,
    description: Option<String>,
    logo: Option<String>,
    website: Option<String>,
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<BrandRow> for BrandResource {
    fn from(b: BrandRow) -> Self {
        Self {
            id: b.id,
            product_brand_id: b.id,
            name: b.name,
            description: b.description,
            logo: b.logo,
            website: b.website,
            meta: b.meta,
            created_at: b.created_at,
            updated_at: b.updated_at,
        }
    }
}

/// Create response.
#[derive(Serialize)]
pub struct BrandCreated {
    pub product_brand_id: Uuid,
}

/// GET list query. Unknown params ignored.
#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// Validated body. Create sends every field; update leaves absent ones `None` (keep).
pub struct BrandInput {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub logo: Option<Option<String>>,
    pub website: Option<Option<String>>,
    pub meta: Option<Option<Value>>,
}

/// POST body for create and update.
#[derive(Deserialize)]
pub struct BrandBody {
    #[serde(default, deserialize_with = "present")]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    description: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    logo: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    website: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    meta: Option<Option<Value>>,
}

/// POST /product-brands: `name` required.
#[derive(Deserialize)]
pub struct CreateBrandRequest(BrandBody);

/// POST /product-brands/{id}: `sometimes|required` name, the rest patches.
#[derive(Deserialize)]
pub struct UpdateBrandRequest(BrandBody);

impl Validate for CreateBrandRequest {
    type Valid = BrandInput;

    fn validate(self, r: &mut Rules) -> Option<BrandInput> {
        let b = self.0;
        Some(BrandInput {
            name: Some(r.text("name", b.name.flatten(), 255)),
            ..rest(r, BrandBody { name: None, ..b }, false)
        })
    }
}

impl Validate for UpdateBrandRequest {
    type Valid = BrandInput;

    fn validate(self, r: &mut Rules) -> Option<BrandInput> {
        let b = self.0;
        Some(BrandInput {
            name: r.patch_text("name", b.name.clone(), 255),
            ..rest(r, b, true)
        })
    }
}

fn rest(r: &mut Rules, b: BrandBody, patch: bool) -> BrandInput {
    let keep = |v: Option<Option<String>>| if patch { v } else { Some(v.flatten()) };
    BrandInput {
        name: None,
        description: keep(b.description).map(|v| r.opt_text("description", v, usize::MAX)),
        logo: keep(b.logo).map(|v| url(r, "logo", v)),
        website: keep(b.website).map(|v| url(r, "website", v)),
        meta: if patch {
            b.meta.and_then(|v| r.meta(v))
        } else {
            r.meta(b.meta.flatten())
        },
    }
}

/// `nullable|url|max:2048`.
fn url(r: &mut Rules, key: &str, v: Option<String>) -> Option<String> {
    let v = r.opt_text(key, v, 2048)?;
    if !is_url(&v) {
        r.add(key, format!("The {} must be a valid URL.", human(key)));
    }
    Some(v)
}
