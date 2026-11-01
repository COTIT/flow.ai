//! farm_equipment_categories and farm_equipment SQL. Every statement filters `workspace_id`.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgExecutor;
use uuid::Uuid;

/// A stored category.
pub struct CategoryRow {
    pub id: Uuid,
    pub code: String,
    pub name_en: String,
    pub name_sw: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A new category; `id` None = server picks.
pub struct NewCategory {
    pub id: Option<Uuid>,
    pub code: String,
    pub name_en: String,
    pub name_sw: String,
}

/// A stored machine with its category and currency.
pub struct EquipmentRow {
    pub id: Uuid,
    pub category_id: Uuid,
    pub category_code: String,
    pub category_name_en: String,
    pub category_name_sw: String,
    pub code: String,
    pub name_en: String,
    pub name_sw: String,
    pub rated_power_kw: Option<Decimal>,
    pub typical_hours_per_day: Option<Decimal>,
    pub typical_days_per_week: Option<Decimal>,
    pub can_rent: bool,
    pub can_buy: bool,
    pub indicative_price_minor: Option<i64>,
    pub indicative_rent_per_day_minor: Option<i64>,
    pub currency_id: Uuid,
    pub currency: String,
    pub decimals: i16,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A new machine; `id` None = server picks.
pub struct NewEquipment {
    pub id: Option<Uuid>,
    pub category_id: Uuid,
    pub code: String,
    pub name_en: String,
    pub name_sw: String,
    pub rated_power_kw: Option<Decimal>,
    pub typical_hours_per_day: Option<Decimal>,
    pub typical_days_per_week: Option<Decimal>,
    pub can_rent: bool,
    pub can_buy: bool,
    pub indicative_price_minor: Option<i64>,
    pub indicative_rent_per_day_minor: Option<i64>,
    pub currency_id: Uuid,
}

/// One page of categories by English name (`active` None = all), and the count.
pub async fn list_categories(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    active: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<CategoryRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        CategoryRow,
        "SELECT id, code, name_en, name_sw, is_active, created_at, updated_at
         FROM farm_equipment_categories
         WHERE workspace_id = $1 AND ($2::bool IS NULL OR is_active = $2)
         ORDER BY name_en, id LIMIT $3 OFFSET $4",
        workspace_id,
        active,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_equipment_categories
           WHERE workspace_id = $1 AND ($2::bool IS NULL OR is_active = $2)"#,
        workspace_id,
        active,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// One category of this workspace.
pub async fn find_category(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<CategoryRow>, sqlx::Error> {
    sqlx::query_as!(
        CategoryRow,
        "SELECT id, code, name_en, name_sw, is_active, created_at, updated_at
         FROM farm_equipment_categories WHERE workspace_id = $1 AND id = $2",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Insert a category.
pub async fn insert_category(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    c: &NewCategory,
) -> Result<CategoryRow, sqlx::Error> {
    sqlx::query_as!(
        CategoryRow,
        "INSERT INTO farm_equipment_categories (id, workspace_id, code, name_en, name_sw)
         VALUES (coalesce($1, uuidv7()), $2, $3, $4, $5)
         RETURNING id, code, name_en, name_sw, is_active, created_at, updated_at",
        c.id,
        workspace_id,
        c.code,
        c.name_en,
        c.name_sw,
    )
    .fetch_one(db)
    .await
}

/// One page of machines by English name (`active` None = all), and the count.
pub async fn list_equipment(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
    active: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EquipmentRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        EquipmentRow,
        "SELECT e.id, e.category_id, k.code AS category_code, k.name_en AS category_name_en,
                k.name_sw AS category_name_sw, e.code, e.name_en, e.name_sw, e.rated_power_kw,
                e.typical_hours_per_day, e.typical_days_per_week, e.can_rent, e.can_buy,
                e.indicative_price_minor, e.indicative_rent_per_day_minor, e.currency_id,
                c.asset_type AS currency, c.decimals, e.is_active, e.created_at, e.updated_at
         FROM farm_equipment e
         JOIN farm_equipment_categories k ON k.id = e.category_id
         JOIN currencies c ON c.id = e.currency_id
         WHERE e.workspace_id = $1 AND ($2::bool IS NULL OR e.is_active = $2)
         ORDER BY e.name_en, e.id LIMIT $3 OFFSET $4",
        workspace_id,
        active,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_equipment
           WHERE workspace_id = $1 AND ($2::bool IS NULL OR is_active = $2)"#,
        workspace_id,
        active,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// One machine of this workspace.
pub async fn find_equipment(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<EquipmentRow>, sqlx::Error> {
    sqlx::query_as!(
        EquipmentRow,
        "SELECT e.id, e.category_id, k.code AS category_code, k.name_en AS category_name_en,
                k.name_sw AS category_name_sw, e.code, e.name_en, e.name_sw, e.rated_power_kw,
                e.typical_hours_per_day, e.typical_days_per_week, e.can_rent, e.can_buy,
                e.indicative_price_minor, e.indicative_rent_per_day_minor, e.currency_id,
                c.asset_type AS currency, c.decimals, e.is_active, e.created_at, e.updated_at
         FROM farm_equipment e
         JOIN farm_equipment_categories k ON k.id = e.category_id
         JOIN currencies c ON c.id = e.currency_id
         WHERE e.workspace_id = $1 AND e.id = $2",
        workspace_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// Insert a machine; its id.
pub async fn insert_equipment(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    e: &NewEquipment,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO farm_equipment (id, workspace_id, category_id, code, name_en, name_sw,
             rated_power_kw, typical_hours_per_day, typical_days_per_week, can_rent, can_buy,
             indicative_price_minor, indicative_rent_per_day_minor, currency_id)
         VALUES (coalesce($1, uuidv7()), $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         RETURNING id",
        e.id,
        workspace_id,
        e.category_id,
        e.code,
        e.name_en,
        e.name_sw,
        e.rated_power_kw,
        e.typical_hours_per_day,
        e.typical_days_per_week,
        e.can_rent,
        e.can_buy,
        e.indicative_price_minor,
        e.indicative_rent_per_day_minor,
        e.currency_id,
    )
    .fetch_one(db)
    .await
}
