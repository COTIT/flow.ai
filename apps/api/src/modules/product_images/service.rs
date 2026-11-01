//! Product image rules: only stored `image/*` files of the workspace, only variants of the
//! product, at most 20, each file once. Writing files needs `file.get` too, as lot passports do.

use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::{NOT_ALLOWED, WorkspaceAccess},
    modules::{
        files::{repo as files, service as files_service},
        lots::public::Link,
        product_images::{
            dto::{NewImages, ProductImageResource},
            repo::{self, ImageRow},
        },
    },
};

const FILE_GET: &str = "file.get";
const NOT_IMAGE: &str = "The file must be an image uploaded to this workspace.";
const BAD_VARIANT: &str = "The selected product variant id is invalid.";

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Signed links of the stored files among `ids`; none on a server without storage.
async fn links(
    state: &AppState,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, Link>, AppError> {
    let mut out = HashMap::new();
    if let Ok(store) = files_service::store(state) {
        for f in files::find_many(&state.db, workspace_id, ids).await? {
            let (url, expires_at) = files_service::signed_link(store, &f)?;
            out.insert(f.id, Link::new(url, expires_at));
        }
    }
    Ok(out)
}

async fn resources(
    state: &AppState,
    workspace_id: Uuid,
    rows: &[ImageRow],
) -> Result<Vec<ProductImageResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.file_id).collect();
    let links = links(state, workspace_id, &ids).await?;
    Ok(rows
        .iter()
        .map(|r| ProductImageResource::new(r, links.get(&r.file_id).cloned()))
        .collect())
}

/// The product's images in order; 404 for a product that isn't a live one of the workspace.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    product: &str,
) -> Result<Vec<ProductImageResource>, AppError> {
    let id = parse(product)?;
    if !repo::product_exists(&state.db, workspace_id, id).await? {
        return Err(AppError::NotFound);
    }
    let rows = repo::for_products(&state.db, workspace_id, &[id]).await?;
    resources(state, workspace_id, &rows).await
}

/// Replace the product's images; the new list.
pub async fn replace(
    state: &AppState,
    access: &WorkspaceAccess,
    product: &str,
    NewImages(images): NewImages,
) -> Result<Vec<ProductImageResource>, AppError> {
    let ws = access.workspace_id;
    let id = parse(product)?;
    let mut tx = state.db.begin().await?;
    if !repo::lock_product(&mut tx, ws, id).await? {
        return Err(AppError::NotFound);
    }
    if !images.is_empty() && !access.allows(FILE_GET) {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let file_ids: Vec<Uuid> = images.iter().map(|i| i.0).collect();
    let found = files::find_many(&mut *tx, ws, &file_ids).await?;
    let wanted: Vec<Uuid> = images.iter().filter_map(|i| i.1).collect();
    let variants = repo::variants_of(&mut *tx, ws, id, &wanted).await?;
    let mut errors = std::collections::BTreeMap::new();
    for (n, (file, variant)) in images.iter().enumerate() {
        let image = found
            .iter()
            .any(|f| f.id == *file && f.content_type.starts_with("image/"));
        if !image {
            errors.insert(format!("images.{n}.file_id"), vec![NOT_IMAGE.to_owned()]);
        }
        if variant.is_some_and(|v| !variants.contains(&v)) {
            let key = format!("images.{n}.product_variant_id");
            errors.insert(key, vec![BAD_VARIANT.to_owned()]);
        }
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    repo::replace(&mut tx, ws, id, &images).await?;
    let rows = repo::for_products(&mut *tx, ws, &[id]).await?;
    tx.commit().await?;
    resources(state, ws, &rows).await
}

/// Storefront links per (product, variant) for a page of products; variant None = the
/// product's own images. Two statements per page.
pub async fn storefront_links(
    state: &AppState,
    workspace_id: Uuid,
    product_ids: &[Uuid],
) -> Result<HashMap<(Uuid, Option<Uuid>), Vec<Link>>, AppError> {
    let rows = repo::for_products(&state.db, workspace_id, product_ids).await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.file_id).collect();
    let links = links(state, workspace_id, &ids).await?;
    let mut out: HashMap<(Uuid, Option<Uuid>), Vec<Link>> = HashMap::new();
    for r in &rows {
        if let Some(l) = links.get(&r.file_id) {
            out.entry((r.product_id, r.product_variant_id))
                .or_default()
                .push(l.clone());
        }
    }
    Ok(out)
}
