//! AFM catalogue sync (CLI `seed-afm-catalog`, never a route): AFM's store
//! (`afm_catalogue.json`, used with AFM's consent) into AFM's own workspace, `afm` (production) or
//! `afm-demo` (staging), through the API's routes in-process as the workspace's owner. Creates
//! what is missing, repairs catalogue fields that drifted (name, price, texts, category, variants)
//! and never touches stock, store-only flags or another workspace. A rerun changes nothing.

use std::collections::HashMap;

use axum::http::Method;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    catalog::{self, AfmProduct, AfmVariant, CATEGORIES},
    dollars,
    photos::{self, Photos},
};
use crate::{
    AppState, build_app,
    error::AppError,
    farm_demo::{Api, text},
    modules::{
        auth::{service::issue_token, token},
        files::service::{self as files, NewFile},
    },
};

/// The only workspaces the sync writes to: AFM's on production and on staging.
pub const WORKSPACES: &[&str] = &["afm", "afm-demo"];
/// The storefront key the shop container uses.
const KEY_NAME: &str = "Web shop";
const COMPANY: &str = "AFM Shop";

/// What a run changed. `app_key` is a secret: print it once, never log it.
#[derive(Default)]
pub struct Synced {
    pub categories_created: usize,
    pub products_created: usize,
    pub products_updated: usize,
    pub variants_created: usize,
    pub variants_updated: usize,
    pub variants_deleted: usize,
    /// Default currency and company name set.
    pub settings_changed: usize,
    pub photos_added: usize,
    /// Photos that could not be had (CDN down, not an image): those products stay bare.
    pub photos_missing: usize,
    /// The new storefront key (only with `--app-key`, only when none was live).
    pub app_key: Option<String>,
}

impl Synced {
    /// Rows written; 0 on a rerun.
    pub fn changes(&self) -> usize {
        self.categories_created
            + self.products_created
            + self.products_updated
            + self.variants_created
            + self.variants_updated
            + self.variants_deleted
            + self.settings_changed
            + self.photos_added
            + usize::from(self.app_key.is_some())
    }
}

// The key never reaches a log line through `{:?}`.
impl std::fmt::Debug for Synced {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Synced")
            .field("categories_created", &self.categories_created)
            .field("products_created", &self.products_created)
            .field("products_updated", &self.products_updated)
            .field("variants_created", &self.variants_created)
            .field("variants_updated", &self.variants_updated)
            .field("variants_deleted", &self.variants_deleted)
            .field("settings_changed", &self.settings_changed)
            .field("photos_added", &self.photos_added)
            .field("photos_missing", &self.photos_missing)
            .field("app_key", &self.app_key.as_ref().map(|_| "***"))
            .finish()
    }
}

fn refuse(message: String) -> AppError {
    AppError::Unprocessable(message)
}

/// Sync AFM's catalogue into `workspace`; `photos` stores missing product photos, `app_key`
/// makes the shop's key when none is live.
pub async fn sync(
    state: AppState,
    workspace: &str,
    photos: Option<&Photos>,
    app_key: bool,
) -> Result<Synced, AppError> {
    if !WORKSPACES.contains(&workspace) {
        return Err(refuse(format!(
            "seed-afm-catalog writes only to {WORKSPACES:?}, not {workspace}"
        )));
    }
    let ws = sqlx::query!(
        "SELECT id, created_by_user_id FROM workspaces WHERE name = $1 AND deleted_at IS NULL",
        workspace
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| refuse(format!("no workspace {workspace}: create it first")))?;
    // A short login as the owner, deleted afterwards, so every write goes through the routes.
    let login = issue_token(
        &state.db,
        ws.created_by_user_id,
        chrono::Duration::minutes(15),
    )
    .await?;
    let result = run(&state, ws.id, login.expose(), photos, app_key).await;
    sqlx::query!(
        "DELETE FROM access_tokens WHERE token_hash = $1",
        &token::hash(login.expose())[..]
    )
    .execute(&state.db)
    .await?;
    result
}

async fn run(
    state: &AppState,
    ws_id: Uuid,
    owner: &str,
    photos: Option<&Photos>,
    app_key: bool,
) -> Result<Synced, AppError> {
    let mut quick = state.clone();
    for limit in [
        &mut quick.rate_limit,
        &mut quick.private_rate_limit,
        &mut quick.private_ip_rate_limit,
    ] {
        limit.burst = 100_000;
    }
    let api = Api {
        app: build_app(quick),
        staff_password: String::new(),
        farmer_password: String::new(),
    };
    let at = |tail: &str| format!("/workspace/{ws_id}{tail}");
    let mut done = Synced::default();

    // USD prices: USD is the default currency.
    let currencies = api
        .call(Method::GET, &at("/currencies"), None, Some(owner))
        .await?;
    let usd = currencies["data"]
        .as_array()
        .and_then(|c| {
            c.iter().find(|c| {
                c["asset_type"]
                    .as_str()
                    .is_some_and(|t| t.eq_ignore_ascii_case("usd"))
            })
        })
        .ok_or_else(|| AppError::Internal("seed-afm-catalog: no USD".into()))?;
    if usd["default_currency"].as_bool() != Some(true) {
        let id = text(usd, "id").or_else(|_| text(usd, "currency_id"))?;
        api.post(
            &at(&format!("/currencies/{id}")),
            json!({ "default_currency": true }),
            owner,
        )
        .await?;
        done.settings_changed += 1;
    }
    let named = sqlx::query!(
        "UPDATE workspaces SET profile = coalesce(profile, '{}'::jsonb) || jsonb_build_object('company_name', $2::text)
         WHERE id = $1 AND profile->>'company_name' IS DISTINCT FROM $2",
        ws_id,
        COMPANY
    )
    .execute(&state.db)
    .await?;
    done.settings_changed += usize::try_from(named.rows_affected()).unwrap_or_default();

    let mut categories: HashMap<String, Uuid> = sqlx::query!(
        "SELECT id, name FROM product_categories WHERE workspace_id = $1 AND deleted_at IS NULL",
        ws_id
    )
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|r| (r.name, r.id))
    .collect();
    for (name, description) in CATEGORIES {
        if categories.contains_key(*name) {
            continue;
        }
        let made = api
            .post(
                &at("/erp/product-categories"),
                json!({ "name": name, "description": description }),
                owner,
            )
            .await?;
        let id = text(&made, "product_category_id")?
            .parse()
            .map_err(|_| AppError::Internal("seed-afm-catalog: category id".into()))?;
        categories.insert((*name).to_owned(), id);
        done.categories_created += 1;
    }

    let mut existing: HashMap<String, Existing> = sqlx::query_as!(
        Existing,
        r#"SELECT id, reference AS "reference!", name, round(price * 100)::bigint AS "cents!",
                  description, short_description, category_id
           FROM products WHERE workspace_id = $1 AND deleted_at IS NULL AND reference LIKE 'AFM-%'"#,
        ws_id
    )
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|p| (p.reference.clone(), p))
    .collect();

    for p in catalog::afm()? {
        let category = *categories
            .get(&p.category)
            .ok_or_else(|| AppError::Internal(format!("no category {}", p.category)))?;
        let cents = p.variants.iter().map(|v| v.cents).min().unwrap_or(0);
        let id = match existing.remove(&p.reference) {
            None => {
                let made = api
                    .post(
                        &at("/products"),
                        json!({ "name": p.title, "price": dollars(cents), "category_id": category,
                                "description": p.description, "short_description": p.short_description,
                                "reference": p.reference, "store_only": false, "has_stock": false }),
                        owner,
                    )
                    .await?;
                done.products_created += 1;
                text(&made, "product_id")?
            }
            Some(old) => {
                let mut patch = serde_json::Map::new();
                if old.name != p.title {
                    patch.insert("name".into(), json!(p.title));
                }
                if old.cents != cents {
                    patch.insert("price".into(), json!(dollars(cents)));
                }
                if old.description.as_deref() != Some(p.description.as_str()) {
                    patch.insert("description".into(), json!(p.description));
                }
                if old.short_description.as_deref() != Some(p.short_description.as_str()) {
                    patch.insert("short_description".into(), json!(p.short_description));
                }
                if old.category_id != Some(category) {
                    patch.insert("category_id".into(), json!(category));
                }
                if !patch.is_empty() {
                    api.post(
                        &at(&format!("/products/{}", old.id)),
                        Value::Object(patch),
                        owner,
                    )
                    .await?;
                    done.products_updated += 1;
                }
                old.id.to_string()
            }
        };
        if !p.options.is_empty() {
            variants(&api, state, ws_id, owner, &id, &p, &mut done).await?;
        }
    }

    if let Some(photos) = photos {
        add_photos(&api, state, ws_id, owner, photos, &mut done).await?;
    }

    if app_key {
        let live = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM app_keys
               WHERE workspace_id = $1 AND name = $2 AND kind = 'shop' AND revoked_at IS NULL"#,
            ws_id,
            KEY_NAME
        )
        .fetch_one(&state.db)
        .await?;
        if live == 0 {
            let key = api
                .post(
                    &at("/app-keys"),
                    json!({ "name": KEY_NAME, "kind": "shop", "scopes": ["catalog:read", "checkout:write"] }),
                    owner,
                )
                .await?;
            done.app_key = Some(text(&key, "key")?);
        }
    }
    Ok(done)
}

/// An AFM product already in the workspace.
struct Existing {
    id: Uuid,
    reference: String,
    name: String,
    cents: i64,
    description: Option<String>,
    short_description: Option<String>,
    category_id: Option<Uuid>,
}

/// Make the product's option variants match the catalogue: missing ones generated, prices fixed,
/// ones AFM no longer sells deleted.
async fn variants(
    api: &Api,
    state: &AppState,
    ws_id: Uuid,
    owner: &str,
    product: &str,
    p: &AfmProduct,
    done: &mut Synced,
) -> Result<(), AppError> {
    let product_id: Uuid = product
        .parse()
        .map_err(|_| AppError::Internal("seed-afm-catalog: product id".into()))?;
    let rows = sqlx::query!(
        r#"SELECT v.id, round(v.price * 100)::bigint AS "cents!",
                  array_agg(ov.display_value ORDER BY pot.sort_order) AS "values!"
           FROM product_variants v
           JOIN variant_option_values vov ON vov.product_variant_id = v.id
           JOIN option_values ov ON ov.id = vov.option_value_id
           JOIN product_option_types pot
             ON pot.product_id = v.product_id AND pot.option_type_id = ov.option_type_id
           WHERE v.workspace_id = $1 AND v.product_id = $2 AND v.deleted_at IS NULL
             AND NOT v.is_default
           GROUP BY v.id"#,
        ws_id,
        product_id
    )
    .fetch_all(&state.db)
    .await?;
    let at = |tail: &str| format!("/workspace/{ws_id}/products/{product}/variants{tail}");
    let mut have: HashMap<Vec<String>, (Uuid, i64)> = rows
        .into_iter()
        .map(|r| (r.values, (r.id, r.cents)))
        .collect();
    let mut missing: Vec<AfmVariant> = Vec::new();
    for v in &p.variants {
        match have.remove(&v.values) {
            None => missing.push(AfmVariant {
                values: v.values.clone(),
                cents: v.cents,
            }),
            Some((id, cents)) if cents != v.cents => {
                api.call(
                    Method::PUT,
                    &at(&format!("/{id}")),
                    Some(json!({ "price": dollars(v.cents) })),
                    Some(owner),
                )
                .await?;
                done.variants_updated += 1;
            }
            Some(_) => {}
        }
    }
    for (id, _) in have.into_values() {
        api.call(Method::DELETE, &at(&format!("/{id}")), None, Some(owner))
            .await?;
        done.variants_deleted += 1;
    }
    if !missing.is_empty() {
        api.post(&at("/generate"), catalog::generate_body(p, &missing), owner)
            .await?;
        done.variants_created += missing.len();
    }
    Ok(())
}

/// AFM's photos for every AFM product that has none yet (cached in the bucket, as the demo seed).
async fn add_photos(
    api: &Api,
    state: &AppState,
    ws_id: Uuid,
    owner_token: &str,
    photos: &Photos,
    done: &mut Synced,
) -> Result<(), AppError> {
    let store = files::store(state)?;
    let bare: HashMap<String, Uuid> = sqlx::query!(
        r#"SELECT p.id, p.reference AS "reference!" FROM products p
           WHERE p.workspace_id = $1 AND p.deleted_at IS NULL AND p.reference LIKE 'AFM-%'
             AND NOT EXISTS (SELECT 1 FROM product_images i
                             WHERE i.workspace_id = $1 AND i.product_id = p.id)"#,
        ws_id
    )
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|r| (r.reference, r.id))
    .collect();
    let owner = sqlx::query_scalar!(
        "SELECT created_by_user_id FROM workspaces WHERE id = $1",
        ws_id
    )
    .fetch_one(&state.db)
    .await?;
    for p in catalog::afm()? {
        let Some(product) = bare.get(&p.reference) else {
            continue;
        };
        let mut ids = Vec::new();
        for (n, url) in p.images.iter().enumerate() {
            let Some(bytes) = photos::fetch(store, photos, url).await? else {
                done.photos_missing += 1;
                continue;
            };
            let name = format!("{}-{}", p.slug, n + 1);
            match files::create(
                state,
                ws_id,
                owner,
                NewFile {
                    document_name: &name,
                    description: None,
                    meta: None,
                    bytes,
                    expires_at: None,
                },
            )
            .await
            {
                Ok(id) => ids.push(json!({ "file_id": id })),
                Err(AppError::Validation(_)) => done.photos_missing += 1,
                Err(err) => return Err(err),
            }
        }
        if !ids.is_empty() {
            done.photos_added += ids.len();
            api.call(
                Method::PUT,
                &format!("/workspace/{ws_id}/products/{product}/images"),
                Some(json!({ "images": ids })),
                Some(owner_token),
            )
            .await?;
        }
    }
    Ok(())
}
