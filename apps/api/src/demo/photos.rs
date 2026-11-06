//! AFM's product photos for `seed-demo --images`: fetched one at a time from AFM's image CDN,
//! kept in the bucket under `genesy/demo-cache/afm/<sha256 of the URL>` so later reseeds never need AFM,
//! then stored as workspace files and set as each product's images. A photo that can't be had
//! is counted and skipped; the seed goes on.

use std::time::Duration;

use reqwest::{Method, header};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{Shop, catalog::Catalog};
use crate::{
    error::AppError,
    modules::files::service::{self as files, NewFile},
    outbound::Outbound,
    storage::Store,
};

/// AFM's image CDN: every photo URL in `afm_catalogue.json` starts with it.
pub const CDN: &str = "https://images.squarespace-cdn.com";
const AGENT: &str = "GenesyDemoSeed/1.0 (+https://genesy.app; AFM staging demo, with consent)";
/// The file upload limit.
const MAX_BYTES: usize = 10 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

/// How the seed reaches the photos.
pub struct Photos {
    /// Public hosts only (no trusted operator host).
    pub http: Outbound,
    /// Where `CDN` URLs are fetched from: `CDN` itself, or a local fake in tests.
    pub origin: String,
    /// Pause after each download, to stay polite to AFM's CDN.
    pub pause: Duration,
}

impl Photos {
    /// The real CDN, a short pause.
    pub fn afm() -> Result<Self, String> {
        Ok(Self {
            http: Outbound::new(&[])?,
            origin: CDN.into(),
            pause: Duration::from_millis(300),
        })
    }
}

/// Bucket key of the cached copy of `url`: under `genesy/`, the only prefix the uploads keys may write.
pub fn cache_key(url: &str) -> String {
    format!(
        "genesy/demo-cache/afm/{}",
        hex::encode(Sha256::digest(url.as_bytes()))
    )
}

/// The bytes of `url`: the cached copy, else downloaded (and cached). None when unreachable.
pub async fn fetch(store: &Store, photos: &Photos, url: &str) -> Result<Option<Vec<u8>>, AppError> {
    let key = cache_key(url);
    if let Some(bytes) = store.get(&key).await? {
        return Ok(Some(bytes));
    }
    let from = match url.strip_prefix(CDN) {
        Some(path) => format!("{}{path}", photos.origin),
        None => return Ok(None),
    };
    let req = photos
        .http
        .request(Method::GET, &from)
        .header(header::USER_AGENT, AGENT)
        .timeout(TIMEOUT);
    let got = match photos.http.send(req).await {
        Ok(res) if res.status().is_success() => res.bytes().await.ok(),
        Ok(res) => {
            tracing::warn!(status = %res.status(), "demo photo not fetched");
            None
        }
        Err(err) => {
            tracing::warn!(error = %err, "demo photo not fetched");
            None
        }
    };
    tokio::time::sleep(photos.pause).await;
    let Some(bytes) = got.filter(|b| !b.is_empty() && b.len() <= MAX_BYTES) else {
        return Ok(None);
    };
    let bytes = bytes.to_vec();
    store
        .put(
            &key,
            bytes.clone(),
            "application/octet-stream",
            "photo",
            false,
        )
        .await?;
    Ok(Some(bytes))
}

/// Store every product's photos and set them as its images; (missing, total) photos.
pub(crate) async fn attach(
    shop: &Shop<'_>,
    catalog: &Catalog,
    owner: Uuid,
    photos: &Photos,
) -> Result<(usize, usize), AppError> {
    let total = catalog.products.iter().map(|p| p.images.len()).sum();
    let Ok(store) = files::store(shop.state) else {
        tracing::warn!("seed-demo --images: no file storage configured, photos skipped");
        return Ok((total, total));
    };
    let mut missing = 0;
    for p in &catalog.products {
        let mut ids = Vec::new();
        for (n, url) in p.images.iter().enumerate() {
            let Some(bytes) = fetch(store, photos, url).await? else {
                missing += 1;
                continue;
            };
            let name = format!("{}-{}", p.slug, n + 1);
            match files::create(
                shop.state,
                shop.ws_id,
                owner,
                NewFile {
                    document_name: &name,
                    description: None,
                    meta: None,
                    bytes,
                },
            )
            .await
            {
                Ok(id) => ids.push(json!({ "file_id": id })),
                // Not an image type the API takes: counted, not fatal.
                Err(AppError::Validation(_)) => missing += 1,
                Err(err) => return Err(err),
            }
        }
        if !ids.is_empty() {
            shop.api
                .call(
                    axum::http::Method::PUT,
                    &shop.at(&format!("/products/{}/images", p.id)),
                    Some(json!({ "images": ids })),
                    Some(&shop.owner),
                )
                .await?;
        }
    }
    Ok((missing, total))
}
