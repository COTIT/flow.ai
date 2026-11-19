//! Verified sale tax profiles for active-book tests (M6-A: active books tax only through a
//! verified profile). Seeded by SQL so fixtures keep their legacy VAT-group rates.

use sqlx::PgPool;

/// One verified NL sale profile; see `profile_in`.
pub async fn profile(
    pool: &PgPool,
    ws: &str,
    categories: &[uuid::Uuid],
    components: &[(&str, &str)],
) -> uuid::Uuid {
    profile_in(pool, ws, "NL", categories, components).await
}

/// One verified sale profile in `country` (from 2000-01-01) with `components` (code, rate)
/// over `categories` (empty = every category); its id.
pub async fn profile_in(
    pool: &PgPool,
    ws: &str,
    country: &str,
    categories: &[uuid::Uuid],
    components: &[(&str, &str)],
) -> uuid::Uuid {
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tax_profiles (workspace_id, lineage_id, country, transaction_types,
                                   product_category_ids, effective_from, tax_point, evidence_ref,
                                   created_by)
         SELECT $1::uuid, uuidv7(), $3, '{sale}', $2, '2000-01-01', 'invoice', 'test fixture',
                created_by_user_id FROM workspaces WHERE id = $1::uuid
         RETURNING id",
    )
    .bind(ws)
    .bind(categories)
    .bind(country)
    .fetch_one(pool)
    .await
    .unwrap();
    for (i, (code, rate)) in components.iter().enumerate() {
        sqlx::query(
            "INSERT INTO tax_rules (workspace_id, profile_id, line_no, code, name, rate, recoverable)
             VALUES ($1::uuid, $2, $3, $4, $4, $5::numeric, false)",
        )
        .bind(ws)
        .bind(id)
        .bind(i16::try_from(i + 1).unwrap())
        .bind(code)
        .bind(rate)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE tax_profiles SET lineage_id = id, state = 'verified', verified_at = now(),
                verified_by = created_by, verify_fingerprint = sha256(id::text::bytea)
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Cover every live product of `ws` at its legacy VAT-group rate (no group = 0 %): one profile
/// for all categories when the products share one rate, else one category per rate.
pub async fn seed_sale_profiles(pool: &PgPool, ws: &str) {
    let rates: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT trim_scale(coalesce(g.vat_amount, 0))::text FROM products p
         LEFT JOIN vat_groups g ON g.workspace_id = p.workspace_id AND g.id = p.vat_group_id
         WHERE p.workspace_id = $1::uuid AND p.deleted_at IS NULL ORDER BY 1",
    )
    .bind(ws)
    .fetch_all(pool)
    .await
    .unwrap();
    if rates.len() <= 1 {
        let rate = rates.first().map_or("0", String::as_str);
        profile(pool, ws, &[], &[("VAT", rate)]).await;
        return;
    }
    for rate in &rates {
        let category: uuid::Uuid = sqlx::query_scalar(
            "WITH c AS (INSERT INTO product_categories (workspace_id, name)
                        VALUES ($1::uuid, 'VAT ' || $2) RETURNING id)
             UPDATE products p SET category_id = c.id FROM c
             WHERE p.workspace_id = $1::uuid AND p.deleted_at IS NULL
               AND trim_scale(coalesce((SELECT g.vat_amount FROM vat_groups g
                     WHERE g.workspace_id = p.workspace_id AND g.id = p.vat_group_id), 0))::text = $2
             RETURNING c.id",
        )
        .bind(ws)
        .bind(rate)
        .fetch_one(pool)
        .await
        .unwrap();
        profile(pool, ws, &[category], &[("VAT", rate)]).await;
    }
}

/// Put one all-category profile with `components` in place of `ws`'s current ones (they are
/// superseded: snapshots may still reference them).
pub async fn replace_sale_profiles(
    pool: &PgPool,
    ws: &str,
    components: &[(&str, &str)],
) -> uuid::Uuid {
    let old: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM tax_profiles WHERE workspace_id = $1::uuid AND superseded_by_id IS NULL",
    )
    .bind(ws)
    .fetch_all(pool)
    .await
    .unwrap();
    let id = profile(pool, ws, &[], components).await;
    sqlx::query("UPDATE tax_profiles SET superseded_by_id = $2 WHERE id = ANY($1)")
        .bind(&old)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    id
}
