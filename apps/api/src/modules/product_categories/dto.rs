//! Category bodies.
// Laravel ProductCategoryResource and Store/UpdateProductCategoryRequest.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{
    IntoParams, ToSchema,
    openapi::{
        Ref,
        schema::{ArrayBuilder, ObjectBuilder, OneOfBuilder, Schema, SchemaType, Type},
    },
};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, present},
    modules::product_categories::repo::CategoryRow,
};

/// Laravel `rules.ValidateId` for domain `product_category`.
pub const BAD_PARENT: &str = "The parent id is an invalid id for the product_category domain.";

// Laravel ProductCategoryResource.
#[derive(Serialize, ToSchema)]
#[schema(description = "A product category. `relationships` is filled on the single-category GET.")]
pub struct CategoryResource {
    id: Uuid,
    product_category_id: Uuid,
    name: String,
    #[schema(required = true)]
    description: Option<String>,
    #[schema(value_type = Option<Object>, required = true)]
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[schema(schema_with = relationships_schema)]
    relationships: Relationships,
}

/// Wire shape of [`Relationships`]: untagged and recursive, so derive can't describe it.
fn relationships_schema() -> Schema {
    let category = || Ref::from_schema_name("CategoryResource");
    OneOfBuilder::new()
        .item(ArrayBuilder::new().max_items(Some(0)).build())
        .item(
            ObjectBuilder::new()
                .property(
                    "parent",
                    OneOfBuilder::new()
                        .item(category())
                        .item(ObjectBuilder::new().schema_type(SchemaType::Type(Type::Null))),
                )
                .property("children", ArrayBuilder::new().items(category()))
                .required("parent")
                .required("children"),
        )
        .description(Some(
            "`[]` on lists, otherwise `{\"parent\": ..., \"children\": [...]}`",
        ))
        .into()
}

/// Laravel only loaded `parent` and `children` on GET one; elsewhere both were missing, so
/// PHP printed an empty array.
#[derive(Serialize)]
#[serde(untagged)]
enum Relationships {
    NotLoaded([(); 0]),
    Loaded {
        parent: Option<Box<CategoryResource>>,
        children: Vec<CategoryResource>,
    },
}

impl From<CategoryRow> for CategoryResource {
    fn from(c: CategoryRow) -> Self {
        Self::build(c, Relationships::NotLoaded([]))
    }
}

impl CategoryResource {
    /// GET one: the category with its parent and live children.
    pub fn with_family(
        c: CategoryRow,
        parent: Option<CategoryRow>,
        children: Vec<CategoryRow>,
    ) -> Self {
        Self::build(
            c,
            Relationships::Loaded {
                parent: parent.map(|p| Box::new(p.into())),
                children: children.into_iter().map(Into::into).collect(),
            },
        )
    }

    fn build(c: CategoryRow, relationships: Relationships) -> Self {
        Self {
            id: c.id,
            product_category_id: c.id,
            name: c.name,
            description: c.description,
            meta: c.meta,
            created_at: c.created_at,
            updated_at: c.updated_at,
            relationships,
        }
    }
}

/// Create response.
#[derive(Serialize, ToSchema)]
pub struct CategoryCreated {
    pub product_category_id: Uuid,
}

/// GET list query. Unknown params ignored.
// Laravel ListProductCategory.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// Search on the name.
    pub q: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
    /// Only children of this category.
    #[param(value_type = Option<Uuid>)]
    pub parent_id: Option<String>,
    /// `1` = only top-level categories.
    #[param(value_type = Option<bool>)]
    pub root_only: Option<String>,
}

/// Validated body. Create sends every field; update leaves absent ones `None` (keep).
pub struct CategoryInput {
    pub name: Option<String>,
    pub parent_id: Option<Option<Uuid>>,
    pub description: Option<Option<String>>,
    pub meta: Option<Option<Value>>,
}

/// POST body for create and update.
#[derive(Deserialize, ToSchema)]
#[schema(as = ProductCategoryRequest, description = "Create needs `name`; update keeps fields left out, null clears them.")]
pub struct CategoryBody {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = String, max_length = 255)]
    name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    parent_id: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    description: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    meta: Option<Option<Value>>,
}

/// POST /product-categories: `name` required.
#[derive(Deserialize)]
pub struct CreateCategoryRequest(CategoryBody);

/// POST /product-categories/{id}: absent keys keep the stored value.
#[derive(Deserialize)]
pub struct UpdateCategoryRequest(CategoryBody);

impl Validate for CreateCategoryRequest {
    type Valid = CategoryInput;

    fn validate(self, r: &mut Rules) -> Option<CategoryInput> {
        let b = self.0;
        Some(CategoryInput {
            name: Some(r.text("name", b.name.flatten(), 255)),
            parent_id: Some(b.parent_id.flatten().and_then(|v| parent(r, v))),
            description: Some(r.opt_text("description", b.description.flatten(), usize::MAX)),
            meta: r.meta(b.meta.flatten()),
        })
    }
}

impl Validate for UpdateCategoryRequest {
    type Valid = CategoryInput;

    fn validate(self, r: &mut Rules) -> Option<CategoryInput> {
        let b = self.0;
        Some(CategoryInput {
            name: r.patch_text("name", b.name, 255),
            parent_id: b.parent_id.map(|v| v.and_then(|v| parent(r, v))),
            description: b
                .description
                .map(|v| r.opt_text("description", v, usize::MAX)),
            meta: b.meta.and_then(|v| r.meta(v)),
        })
    }
}

/// `nullable|uuid|ValidateId`: a malformed id fails both rules; existence is the service's.
fn parent(r: &mut Rules, v: Value) -> Option<Uuid> {
    let v = filled(Some(v))?;
    let id = v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok());
    if id.is_none() {
        r.add("parent_id", "The parent id must be a valid UUID.");
        r.add("parent_id", BAD_PARENT);
    }
    id
}
