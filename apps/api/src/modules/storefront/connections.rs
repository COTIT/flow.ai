//! Stable public Shop URLs; only storefront projections and checkout use this identity.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{FromRequestParts, Path, Query, State},
    http::request::Parts,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        app_key::AppKey,
        json::{Rules, Validate, ValidatedJson},
        list::Paginated,
        response::{Data, Envelope, MessageBody},
        workspace::{Can, WorkspaceAccess, permissions},
    },
    idempotency::{Answer, Keyed},
    modules::{
        app_keys::{repo as keys, secret},
        lots::{self, public::PublicLot},
        orders::{
            self,
            dto::{CheckoutRequest, PaymentIntentResource, PaymentRequest, StorefrontOrder},
        },
        storefront::{
            dto::{ListQuery, StorefrontCategory, StorefrontProduct},
            handlers,
        },
        storefront_design::{self, dto::StorefrontPublishedDesign},
        workspaces,
    },
};

permissions! { Manage => "workspace_admin" }

/// Safe workspace connection metadata; the identifier is public, never a bearer secret.
#[derive(Serialize, ToSchema)]
pub struct StorefrontConnection {
    #[schema(required = true)]
    pub id: Option<Uuid>,
    pub active: bool,
    pub name: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ConnectionRequest {
    #[schema(required = true)]
    active: Option<bool>,
}

impl Validate for ConnectionRequest {
    type Valid = bool;

    fn validate(self, rules: &mut Rules) -> Option<bool> {
        if self.active.is_none() {
            rules.missing("active");
        }
        self.active
    }
}

async fn metadata(state: &AppState, ws: Uuid) -> Result<StorefrontConnection, AppError> {
    let name = workspaces::repo::find(&state.db, ws)
        .await?
        .ok_or(AppError::NotFound)?
        .display_name;
    let row: Option<(Uuid, bool)> =
        sqlx::query_as("SELECT id, active FROM storefront_connections WHERE workspace_id=$1")
            .bind(ws)
            .fetch_optional(&state.db)
            .await?;
    Ok(StorefrontConnection {
        id: row.map(|r| r.0),
        active: row.is_some_and(|r| r.1),
        name,
    })
}

pub fn private() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_connection, update_connection))
}

#[utoipa::path(get, path="/workspace/{workspace}/storefront/connection", tag="storefront-connection",
    operation_id="storefront_connection_get", summary="Read this workspace's public Shop connection",
    params(("workspace"=Uuid,Path)),
    responses((status=200,body=Envelope<StorefrontConnection>),(status=401,body=MessageBody),(status=404,body=MessageBody)),
    security(("bearer"=[])))]
pub async fn get_connection(
    State(state): State<AppState>,
    access: WorkspaceAccess,
) -> Result<Data<StorefrontConnection>, AppError> {
    Ok(Data(metadata(&state, access.workspace_id).await?))
}

#[utoipa::path(put, path="/workspace/{workspace}/storefront/connection", tag="storefront-connection",
    operation_id="storefront_connection_update", summary="Connect or disconnect this workspace's Shop",
    params(("workspace"=Uuid,Path)),request_body=ConnectionRequest,
    responses((status=200,body=Envelope<StorefrontConnection>),(status=401,body=MessageBody),
        (status=403,body=MessageBody),(status=404,body=MessageBody),(status=422,body=UnprocessableBody)),
    security(("bearer"=[])))]
pub async fn update_connection(
    State(state): State<AppState>,
    access: Can<Manage>,
    ValidatedJson(active): ValidatedJson<ConnectionRequest>,
) -> Result<Data<StorefrontConnection>, AppError> {
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    // The workspace row exists before the first connection; it serializes first connects too.
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE id=$1 AND deleted_at IS NULL FOR NO KEY UPDATE",
    )
    .bind(ws)
    .fetch_optional(&mut *tx)
    .await?;
    if found.is_none() {
        return Err(AppError::NotFound);
    }
    let previous: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, active FROM storefront_connections WHERE workspace_id=$1 FOR UPDATE",
    )
    .bind(ws)
    .fetch_optional(&mut *tx)
    .await?;
    let changed = previous.map(|r| r.1) != Some(active) && (active || previous.is_some());
    let id = match previous {
        Some((id, _)) => Some(id),
        None if active => {
            let (_plain, hash, prefix) = secret::generate()?;
            let scopes = vec![
                secret::CATALOG_READ.to_owned(),
                secret::CHECKOUT_WRITE.to_owned(),
            ];
            let key = keys::insert(
                &mut *tx,
                &keys::NewKey {
                    workspace_id: ws,
                    name: "Managed Shop connection",
                    kind: "shop",
                    scopes: &scopes,
                    key_hash: &hash,
                    prefix: &prefix,
                    created_by: Some(access.user_id),
                },
            )
            .await?;
            sqlx::query("INSERT INTO storefront_connections(id,workspace_id,active,created_by,updated_by) VALUES($1,$2,true,$3,$3)")
                .bind(key.id).bind(ws).bind(access.user_id).execute(&mut *tx).await?;
            Some(key.id)
        }
        None => None,
    };
    if changed && let Some(id) = id {
        sqlx::query("UPDATE storefront_connections SET active=$3,updated_by=$4,updated_at=now() WHERE workspace_id=$1 AND id=$2")
            .bind(ws).bind(id).bind(active).bind(access.user_id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO storefront_connection_events(workspace_id,connection_id,active,user_id) VALUES($1,$2,$3,$4)")
            .bind(ws).bind(id).bind(active).bind(access.user_id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Data(metadata(&state, ws).await?))
}

#[derive(FromRow)]
struct ShopRow {
    id: Uuid,
    workspace_id: Uuid,
    scopes: Vec<String>,
    active: bool,
    name: String,
}

/// Restricted public identity, scoped by a database-owned connection rather than a header.
pub struct Shop {
    key: AppKey,
    active: bool,
    name: String,
}

impl Shop {
    fn require_active(&self) -> Result<(), AppError> {
        if self.active {
            Ok(())
        } else {
            Err(AppError::NotFound)
        }
    }

    async fn require_order(&self, state: &AppState, order: &str) -> Result<(), AppError> {
        let id = Uuid::parse_str(order).map_err(|_| AppError::NotFound)?;
        let owned: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM orders WHERE workspace_id=$1 AND id=$2 AND source='storefront' AND app_key_id=$3)")
            .bind(self.key.workspace_id).bind(id).bind(self.key.id).fetch_one(&state.db).await?;
        if owned {
            Ok(())
        } else {
            Err(AppError::NotFound)
        }
    }
}

impl FromRequestParts<AppState> for Shop {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let Path(params) = Path::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::NotFound)?;
        let id = params
            .get("connection")
            .and_then(|id| Uuid::parse_str(id).ok())
            .ok_or(AppError::NotFound)?;
        let row: ShopRow = sqlx::query_as("SELECT c.id,c.workspace_id,k.scopes,c.active,coalesce(nullif(btrim(w.profile->>'company_name'),''),w.name) AS name FROM storefront_connections c JOIN app_keys k ON k.id=c.id AND k.workspace_id=c.workspace_id JOIN workspaces w ON w.id=c.workspace_id WHERE c.id=$1 AND k.revoked_at IS NULL AND w.deleted_at IS NULL")
            .bind(id).fetch_optional(&state.db).await?.ok_or(AppError::NotFound)?;
        Ok(Self {
            key: AppKey::for_connection(state, row.id, row.workspace_id, row.scopes).await?,
            active: row.active,
            name: row.name,
        })
    }
}

/// Mounted with the app-key-equivalent per-connection/IP limiter; no bearer required.
pub fn public() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_shop))
        .routes(routes!(products, product, categories, lot, design))
        .routes(routes!(create_order, order, payment))
}

#[utoipa::path(get,path="/storefront/shops/{connection}",tag="storefront",operation_id="storefront_shop_get",
    params(("connection"=Uuid,Path)),responses((status=200,body=Envelope<StorefrontConnection>),(status=404,body=MessageBody)),security())]
pub async fn get_shop(shop: Shop) -> Data<StorefrontConnection> {
    Data(StorefrontConnection {
        id: Some(shop.key.id),
        active: shop.active,
        name: shop.name,
    })
}

#[utoipa::path(get,path="/storefront/shops/{connection}/products",tag="storefront",operation_id="storefront_shop_products_list",
    params(("connection"=Uuid,Path),ListQuery),responses((status=200,body=Paginated<StorefrontProduct>),(status=404,body=MessageBody)),security())]
pub async fn products(
    State(state): State<AppState>,
    shop: Shop,
    query: Query<ListQuery>,
) -> Result<Json<Paginated<StorefrontProduct>>, AppError> {
    shop.require_active()?;
    handlers::list(State(state), shop.key, query).await
}

#[utoipa::path(get,path="/storefront/shops/{connection}/products/{product}",tag="storefront",operation_id="storefront_shop_products_get",
    params(("connection"=Uuid,Path),("product"=Uuid,Path)),responses((status=200,body=Envelope<StorefrontProduct>),(status=404,body=MessageBody)),security())]
pub async fn product(
    State(state): State<AppState>,
    shop: Shop,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<StorefrontProduct>, AppError> {
    shop.require_active()?;
    handlers::get(State(state), shop.key, Path(id)).await
}

#[utoipa::path(get,path="/storefront/shops/{connection}/categories",tag="storefront",operation_id="storefront_shop_categories",
    params(("connection"=Uuid,Path)),responses((status=200,body=Envelope<Vec<StorefrontCategory>>),(status=404,body=MessageBody)),security())]
pub async fn categories(
    State(state): State<AppState>,
    shop: Shop,
) -> Result<Data<Vec<StorefrontCategory>>, AppError> {
    shop.require_active()?;
    handlers::categories(State(state), shop.key).await
}

#[utoipa::path(get,path="/storefront/shops/{connection}/lots/{code}",tag="storefront",operation_id="storefront_shop_lots_get",
    params(("connection"=Uuid,Path),("code"=String,Path)),responses((status=200,body=Envelope<PublicLot>),(status=404,body=MessageBody)),security())]
pub async fn lot(
    State(state): State<AppState>,
    shop: Shop,
    Path((_, code)): Path<(String, String)>,
) -> Result<Data<PublicLot>, AppError> {
    shop.require_active()?;
    lots::handlers::storefront_get(State(state), shop.key, Path(code)).await
}

#[utoipa::path(get,path="/storefront/shops/{connection}/pages/home",tag="storefront",operation_id="storefront_shop_design",
    params(("connection"=Uuid,Path)),responses((status=200,body=Envelope<Option<StorefrontPublishedDesign>>),(status=404,body=MessageBody)),security())]
pub async fn design(
    State(state): State<AppState>,
    shop: Shop,
) -> Result<Data<Option<StorefrontPublishedDesign>>, AppError> {
    shop.require_active()?;
    storefront_design::handlers::published(State(state), shop.key).await
}

#[utoipa::path(post,path="/storefront/shops/{connection}/orders",tag="storefront",operation_id="storefront_shop_orders_create",
    params(("connection"=Uuid,Path),("Idempotency-Key"=Option<String>,Header)),request_body=CheckoutRequest,
    responses((status=201,body=Envelope<StorefrontOrder>),(status=404,body=MessageBody),(status=422,body=UnprocessableBody)),security())]
pub async fn create_order(
    State(state): State<AppState>,
    shop: Shop,
    Keyed { input, key: idem }: Keyed<CheckoutRequest>,
) -> Result<Answer, AppError> {
    shop.require_active()?;
    shop.key.require(secret::CHECKOUT_WRITE)?;
    orders::checkout::create(&state, &shop.key, input, idem).await
}

#[utoipa::path(get,path="/storefront/shops/{connection}/orders/{order}",tag="storefront",operation_id="storefront_shop_orders_get",
    params(("connection"=Uuid,Path),("order"=Uuid,Path)),responses((status=200,body=Envelope<StorefrontOrder>),(status=404,body=MessageBody)),security())]
pub async fn order(
    State(state): State<AppState>,
    shop: Shop,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<StorefrontOrder>, AppError> {
    shop.require_order(&state, &id).await?;
    orders::storefront::get(State(state), shop.key, Path(id)).await
}

#[utoipa::path(post,path="/storefront/shops/{connection}/orders/{order}/payment",tag="storefront",operation_id="storefront_shop_orders_payment",
    params(("connection"=Uuid,Path),("order"=Uuid,Path)),request_body=PaymentRequest,
    responses((status=200,body=Envelope<PaymentIntentResource>),(status=404,body=MessageBody),(status=409,body=MessageBody),(status=422,body=UnprocessableBody),(status=503,body=MessageBody)),security())]
pub async fn payment(
    State(state): State<AppState>,
    shop: Shop,
    Path((_, id)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<PaymentRequest>,
) -> Result<Data<PaymentIntentResource>, AppError> {
    shop.require_order(&state, &id).await?;
    let origin = state.shop_public_url.as_ref().ok_or(AppError::Unavailable(
        "Shop return origin is not configured.",
    ))?;
    let expected = format!("{origin}/s/{}/orders/{id}", shop.key.id);
    if input.redirect_url != expected {
        return Err(AppError::field(
            "redirect_url",
            "Return to this Shop connection's order page.",
        ));
    }
    shop.key.require(secret::CHECKOUT_WRITE)?;
    Ok(Data(
        orders::checkout::intent(&state, &shop.key, &id, input).await?,
    ))
}
