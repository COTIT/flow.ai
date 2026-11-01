//! Product image bodies.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate},
    modules::{lots::public::Link, product_images::repo::ImageRow},
};

/// Images per product.
pub const IMAGES_MAX: usize = 20;

/// PUT /products/{product}/images.
#[derive(Deserialize, ToSchema)]
#[schema(description = "The product's images in display order; `[]` removes them all.")]
pub struct ReplaceImagesRequest {
    /// At most 20; each file at most once.
    #[schema(value_type = Vec<ImageInput>)]
    images: Option<Value>,
}

/// One image to show.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct ImageInput {
    /// A stored `image/*` file of this workspace.
    file_id: Uuid,
    /// A variant of this product; leave out for the product itself.
    #[schema(required = false)]
    product_variant_id: Option<Uuid>,
}

/// Validated images: (file, variant) in order.
pub struct NewImages(pub Vec<(Uuid, Option<Uuid>)>);

impl Validate for ReplaceImagesRequest {
    type Valid = NewImages;

    fn validate(self, r: &mut Rules) -> Option<NewImages> {
        let v = r.required_value("images", self.images)?;
        let Some(list) = v.as_array().filter(|l| l.len() <= IMAGES_MAX) else {
            r.add("images", "The images field must list at most 20 images.");
            return None;
        };
        let uuid = |v: &Value| v.as_str().and_then(|s| Uuid::parse_str(s).ok());
        let mut out = Vec::with_capacity(list.len());
        for (n, item) in list.iter().enumerate() {
            let file = uuid(&item["file_id"]);
            match file {
                None => r.add(
                    &format!("images.{n}.file_id"),
                    "The file id must be a valid UUID.",
                ),
                Some(f) if out.iter().any(|(o, _)| *o == f) => {
                    r.add(&format!("images.{n}.file_id"), "The file is listed twice.");
                }
                Some(_) => {}
            }
            let variant = match &item["product_variant_id"] {
                Value::Null => None,
                v => {
                    let id = uuid(v);
                    if id.is_none() {
                        let msg = "The product variant id must be a valid UUID.";
                        r.add(&format!("images.{n}.product_variant_id"), msg);
                    }
                    id
                }
            };
            if let Some(f) = file {
                out.push((f, variant));
            }
        }
        Some(NewImages(out))
    }
}

/// One image of a product.
#[derive(Serialize, ToSchema)]
pub struct ProductImageResource {
    file_id: Uuid,
    /// Null = the product's own image.
    #[schema(required = true)]
    product_variant_id: Option<Uuid>,
    /// 1-based display order.
    position: i16,
    /// Signed link; null on a server without file storage.
    #[schema(required = true)]
    link: Option<Link>,
}

impl ProductImageResource {
    /// `row` with its link, if any.
    pub fn new(row: &ImageRow, link: Option<Link>) -> Self {
        Self {
            file_id: row.file_id,
            product_variant_id: row.product_variant_id,
            position: row.position,
            link,
        }
    }
}
