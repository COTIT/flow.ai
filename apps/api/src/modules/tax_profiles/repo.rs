//! SQL for tax_profiles, tax_rules, tax_exemptions. Every statement takes `workspace_id`.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::tax_profiles::dto::{Draft, TaxComponent, TaxExemption};

/// One profile row (without components / exemptions).
pub struct ProfileRow {
    pub id: Uuid,
    pub lineage_id: Uuid,
    pub revision: i32,
    pub corrects_id: Option<Uuid>,
    pub superseded_by_id: Option<Uuid>,
    pub state: String,
    pub version: i64,
    pub country: String,
    pub subdivision: Option<String>,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub transaction_types: Vec<String>,
    pub product_category_ids: Vec<Uuid>,
    pub tax_point: String,
    pub evidence_ref: Option<String>,
    pub verified_by: Option<Uuid>,
    pub verified_at: Option<DateTime<Utc>>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Profiles of `ws`: one by id (`id`), else a filtered page newest first.
pub async fn profiles(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Option<Uuid>,
    country: Option<&str>,
    state: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ProfileRow>, sqlx::Error> {
    sqlx::query_as!(
        ProfileRow,
        r#"SELECT id, lineage_id, revision, corrects_id, superseded_by_id, state, version, country,
                  subdivision, effective_from, effective_to, transaction_types,
                  product_category_ids, tax_point, evidence_ref, verified_by, verified_at,
                  created_by, created_at, updated_at
           FROM tax_profiles
           WHERE workspace_id = $1 AND ($2::uuid IS NULL OR id = $2)
             AND ($3::text IS NULL OR country = $3) AND ($4::text IS NULL OR state = $4)
           ORDER BY created_at DESC, id DESC LIMIT $5 OFFSET $6"#,
        ws,
        id,
        country,
        state,
        limit,
        offset,
    )
    .fetch_all(db)
    .await
}

/// Total for the list filters.
pub async fn count(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    country: Option<&str>,
    state: Option<&str>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM tax_profiles
           WHERE workspace_id = $1 AND ($2::text IS NULL OR country = $2)
             AND ($3::text IS NULL OR state = $3)"#,
        ws,
        country,
        state,
    )
    .fetch_one(db)
    .await
}

/// Lock one profile row: (state, version, corrects_id, superseded_by_id).
pub async fn lock(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<(String, i64, Option<Uuid>, Option<Uuid>)>, sqlx::Error> {
    Ok(sqlx::query!(
        "SELECT state, version, corrects_id, superseded_by_id FROM tax_profiles
         WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
        ws,
        id,
    )
    .fetch_optional(conn)
    .await?
    .map(|r| (r.state, r.version, r.corrects_id, r.superseded_by_id)))
}

/// Components of `ids`, keyed by profile, in line order.
pub async fn components(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, TaxComponent)>, sqlx::Error> {
    Ok(sqlx::query!(
        "SELECT profile_id, code, name, rate, recoverable FROM tax_rules
         WHERE workspace_id = $1 AND profile_id = ANY($2) ORDER BY profile_id, line_no",
        ws,
        ids,
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|r| {
        (
            r.profile_id,
            TaxComponent {
                code: r.code,
                name: r.name,
                kind: "percentage",
                rate: r.rate,
                recoverable: r.recoverable,
            },
        )
    })
    .collect())
}

/// Exemptions of `ids`, keyed by profile, in insert order.
pub async fn exemptions(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, TaxExemption)>, sqlx::Error> {
    Ok(sqlx::query!(
        "SELECT profile_id, relation_id, product_category_id, reason, evidence_ref
         FROM tax_exemptions WHERE workspace_id = $1 AND profile_id = ANY($2)
         ORDER BY profile_id, id",
        ws,
        ids,
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|r| {
        (
            r.profile_id,
            TaxExemption {
                relation_id: r.relation_id,
                product_category_id: r.product_category_id,
                reason: r.reason,
                evidence_ref: r.evidence_ref,
            },
        )
    })
    .collect())
}

/// Which of `ids` are live product categories of `ws`.
pub async fn known_categories(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM product_categories
         WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        ws,
        ids,
    )
    .fetch_all(db)
    .await
}

/// Which of `ids` are live relations of `ws`.
pub async fn known_relations(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM relations WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
        ws,
        ids,
    )
    .fetch_all(db)
    .await
}

/// Insert a draft; `lineage` + `revision` for a correction, else a new lineage at 1.
#[allow(clippy::too_many_arguments)]
pub async fn insert(
    conn: &mut PgConnection,
    ws: Uuid,
    d: &Draft,
    lineage: Option<Uuid>,
    revision: i32,
    corrects: Option<Uuid>,
    user: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "WITH n AS (SELECT uuidv7() AS id)
         INSERT INTO tax_profiles (id, workspace_id, lineage_id, revision, corrects_id, country,
                                   subdivision, effective_from, effective_to, transaction_types,
                                   product_category_ids, tax_point, evidence_ref, created_by)
         SELECT n.id, $1, coalesce($2, n.id), $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
         FROM n RETURNING id",
        ws,
        lineage,
        revision,
        corrects,
        d.country,
        d.subdivision,
        d.effective_from,
        d.effective_to,
        &d.transaction_types,
        &d.product_categories,
        d.tax_point,
        d.evidence_ref,
        user,
    )
    .fetch_one(conn)
    .await
}

/// Overwrite a draft's fields and bump its version.
pub async fn update(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    d: &Draft,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE tax_profiles SET country = $3, subdivision = $4, effective_from = $5,
                effective_to = $6, transaction_types = $7, product_category_ids = $8,
                tax_point = $9, evidence_ref = $10, version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        d.country,
        d.subdivision,
        d.effective_from,
        d.effective_to,
        &d.transaction_types,
        &d.product_categories,
        d.tax_point,
        d.evidence_ref,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Replace a draft's components and exemptions.
pub async fn replace_parts(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    d: &Draft,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM tax_rules WHERE workspace_id = $1 AND profile_id = $2",
        ws,
        id
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "DELETE FROM tax_exemptions WHERE workspace_id = $1 AND profile_id = $2",
        ws,
        id
    )
    .execute(&mut *conn)
    .await?;
    let c = &d.components;
    sqlx::query!(
        "INSERT INTO tax_rules (workspace_id, profile_id, line_no, code, name, rate, recoverable)
         SELECT $1, $2, n::smallint, code, name, rate, recoverable
         FROM unnest($3::text[], $4::text[], $5::numeric[], $6::bool[])
              WITH ORDINALITY AS t(code, name, rate, recoverable, n)",
        ws,
        id,
        &c.iter().map(|x| x.code.clone()).collect::<Vec<_>>(),
        &c.iter().map(|x| x.name.clone()).collect::<Vec<_>>(),
        &c.iter().map(|x| x.rate).collect::<Vec<Decimal>>(),
        &c.iter().map(|x| x.recoverable).collect::<Vec<_>>(),
    )
    .execute(&mut *conn)
    .await?;
    let e = &d.exemptions;
    sqlx::query!(
        "INSERT INTO tax_exemptions (workspace_id, profile_id, relation_id, product_category_id,
                                     reason, evidence_ref)
         SELECT $1, $2, rel, cat, reason, evidence
         FROM unnest($3::uuid[], $4::uuid[], $5::text[], $6::text[])
              WITH ORDINALITY AS t(rel, cat, reason, evidence, n)
         ORDER BY n",
        ws,
        id,
        &e.iter().map(|x| x.relation_id).collect::<Vec<_>>() as &[Option<Uuid>],
        &e.iter().map(|x| x.product_category_id).collect::<Vec<_>>() as &[Option<Uuid>],
        &e.iter().map(|x| x.reason.clone()).collect::<Vec<_>>(),
        &e.iter().map(|x| x.evidence_ref.clone()).collect::<Vec<_>>(),
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Serialize verifies per workspace (overlap check, §R3): `erp.tax|<ws>` advisory lock.
pub async fn verify_gate(conn: &mut PgConnection, ws: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('erp.tax|' || $1::text, 0))")
        .bind(ws)
        .execute(conn)
        .await?;
    Ok(())
}

/// A current verified profile of another lineage overlaps `id` in jurisdiction, transaction
/// type, category (empty = all) and dates.
pub async fn overlaps(conn: &mut PgConnection, ws: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM tax_profiles p JOIN tax_profiles o
                 ON o.workspace_id = p.workspace_id AND o.lineage_id <> p.lineage_id
                AND o.state = 'verified' AND o.superseded_by_id IS NULL
                AND o.country = p.country AND o.subdivision IS NOT DISTINCT FROM p.subdivision
                AND o.transaction_types && p.transaction_types
                AND (cardinality(o.product_category_ids) = 0
                     OR cardinality(p.product_category_ids) = 0
                     OR o.product_category_ids && p.product_category_ids)
                AND daterange(o.effective_from, o.effective_to)
                    && daterange(p.effective_from, p.effective_to)
               WHERE p.workspace_id = $1 AND p.id = $2) AS "x!""#,
        ws,
        id,
    )
    .fetch_one(conn)
    .await
}

/// Mark a draft verified (version + 1); a correction supersedes its predecessor.
pub async fn verify(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    evidence_ref: &str,
    fingerprint: &[u8],
    user: Uuid,
    corrects: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE tax_profiles SET state = 'verified', evidence_ref = $3, verify_fingerprint = $4,
                verified_by = $5, verified_at = now(), version = version + 1, updated_at = now()
         WHERE workspace_id = $1 AND id = $2",
        ws,
        id,
        evidence_ref,
        fingerprint,
        user,
    )
    .execute(&mut *conn)
    .await?;
    if let Some(prev) = corrects {
        sqlx::query!(
            "UPDATE tax_profiles SET superseded_by_id = $3, updated_at = now()
             WHERE workspace_id = $1 AND id = $2",
            ws,
            prev,
            id,
        )
        .execute(conn)
        .await?;
    }
    Ok(())
}
