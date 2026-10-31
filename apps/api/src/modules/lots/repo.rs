//! SQL for `lots` and the append-only `lot_events` trail. Writes run in the caller's
//! transaction so a change and its event commit together.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A lot row (passport columns excluded).
#[derive(Clone, Debug)]
pub struct LotRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub code: String,
    pub public_code: String,
    pub status: String,
    pub produced_on: Option<NaiveDate>,
    pub best_before: Option<NaiveDate>,
    pub expires_on: Option<NaiveDate>,
    pub origin: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A trail row.
pub struct EventRow {
    pub id: Uuid,
    pub kind: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub user_id: Option<Uuid>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
}

/// A trail row to append.
pub struct NewEvent<'a> {
    pub lot_id: Uuid,
    pub kind: &'static str,
    pub from_status: Option<&'a str>,
    pub to_status: Option<&'a str>,
    pub user_id: Uuid,
    pub meta: Option<Value>,
}

/// List filters; `q` is a code prefix (LIKE wildcards escaped by the caller).
pub struct ListFilter<'a> {
    pub product_id: Option<Uuid>,
    pub status: Option<&'a str>,
    pub q: Option<&'a str>,
}

/// Insert a lot; None when `public_code` collided (caller retries with a new one).
pub async fn insert(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    l: &LotRow,
    created_by: Uuid,
) -> Result<Option<LotRow>, sqlx::Error> {
    sqlx::query_as!(
        LotRow,
        "INSERT INTO lots (workspace_id, product_id, code, public_code, status, produced_on,
                           best_before, expires_on, origin, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (public_code) DO NOTHING
         RETURNING id, product_id, code, public_code, status, produced_on, best_before,
                   expires_on, origin, created_at, updated_at",
        workspace_id,
        l.product_id,
        l.code,
        l.public_code,
        l.status,
        l.produced_on,
        l.best_before,
        l.expires_on,
        l.origin,
        created_by,
    )
    .fetch_optional(conn)
    .await
}

/// One lot of the workspace.
pub async fn find(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<LotRow>, sqlx::Error> {
    sqlx::query_as!(
        LotRow,
        "SELECT id, product_id, code, public_code, status, produced_on, best_before, expires_on,
                origin, created_at, updated_at
         FROM lots WHERE id = $1 AND workspace_id = $2",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// One lot of the workspace under `FOR NO KEY UPDATE`.
pub async fn locked(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<LotRow>, sqlx::Error> {
    sqlx::query_as!(
        LotRow,
        "SELECT id, product_id, code, public_code, status, produced_on, best_before, expires_on,
                origin, created_at, updated_at
         FROM lots WHERE id = $1 AND workspace_id = $2
         FOR NO KEY UPDATE",
        id,
        workspace_id,
    )
    .fetch_optional(conn)
    .await
}

/// Write the editable fields of a lot the caller holds locked.
pub async fn update(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    l: &LotRow,
) -> Result<LotRow, sqlx::Error> {
    sqlx::query_as!(
        LotRow,
        "UPDATE lots SET code = $3, status = $4, produced_on = $5, best_before = $6,
                         expires_on = $7, origin = $8, updated_at = now()
         WHERE id = $1 AND workspace_id = $2
         RETURNING id, product_id, code, public_code, status, produced_on, best_before,
                   expires_on, origin, created_at, updated_at",
        l.id,
        workspace_id,
        l.code,
        l.status,
        l.produced_on,
        l.best_before,
        l.expires_on,
        l.origin,
    )
    .fetch_one(conn)
    .await
}

/// One page, newest first, with the total.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<LotRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, product_id, code, public_code, status, produced_on, best_before,
                  expires_on, origin, created_at, updated_at, count(*) OVER () AS "total!"
           FROM lots
           WHERE workspace_id = $1
             AND ($2::uuid IS NULL OR product_id = $2)
             AND ($3::text IS NULL OR status = $3)
             AND ($4::text IS NULL OR lower(code) LIKE lower($4) || '%')
           ORDER BY id DESC
           LIMIT $5 OFFSET $6"#,
        workspace_id,
        f.product_id,
        f.status,
        f.q,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let lots = rows
        .into_iter()
        .map(|r| LotRow {
            id: r.id,
            product_id: r.product_id,
            code: r.code,
            public_code: r.public_code,
            status: r.status,
            produced_on: r.produced_on,
            best_before: r.best_before,
            expires_on: r.expires_on,
            origin: r.origin,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    Ok((lots, total))
}

/// Append one trail row in the caller's transaction.
pub async fn add_event(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    e: &NewEvent<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO lot_events (workspace_id, lot_id, kind, from_status, to_status, user_id, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        workspace_id,
        e.lot_id,
        e.kind,
        e.from_status,
        e.to_status,
        e.user_id,
        e.meta,
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// One page of a lot's trail, newest first, with the total.
pub async fn events(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    lot_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EventRow>, i64), sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, kind, from_status, to_status, user_id, meta, created_at,
                  count(*) OVER () AS "total!"
           FROM lot_events
           WHERE lot_id = $1 AND workspace_id = $2
           ORDER BY created_at DESC, id DESC
           LIMIT $3 OFFSET $4"#,
        lot_id,
        workspace_id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().map_or(0, |r| r.total);
    let events = rows
        .into_iter()
        .map(|r| EventRow {
            id: r.id,
            kind: r.kind,
            from_status: r.from_status,
            to_status: r.to_status,
            user_id: r.user_id,
            meta: r.meta,
            created_at: r.created_at,
        })
        .collect();
    Ok((events, total))
}

/// A lot's passport columns.
pub struct PassportRow {
    pub draft: Value,
    pub published: Option<Value>,
    pub version: i32,
    pub published_at: Option<DateTime<Utc>>,
}

/// The passport columns of one lot of the workspace.
pub async fn passport(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<PassportRow>, sqlx::Error> {
    sqlx::query_as!(
        PassportRow,
        "SELECT passport_draft AS draft, passport AS published, passport_version AS version,
                published_at
         FROM lots WHERE id = $1 AND workspace_id = $2",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}

/// Replace the draft; None when the lot is not this workspace's.
pub async fn set_draft(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    draft: &Value,
) -> Result<Option<PassportRow>, sqlx::Error> {
    sqlx::query_as!(
        PassportRow,
        "UPDATE lots SET passport_draft = $3, updated_at = now()
         WHERE id = $1 AND workspace_id = $2
         RETURNING passport_draft AS draft, passport AS published,
                   passport_version AS version, published_at",
        id,
        workspace_id,
        draft,
    )
    .fetch_optional(db)
    .await
}

/// The draft becomes the published copy with the next version (lot locked by the caller).
pub async fn publish(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<PassportRow, sqlx::Error> {
    sqlx::query_as!(
        PassportRow,
        "UPDATE lots SET passport = passport_draft, passport_version = passport_version + 1,
                         published_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2
         RETURNING passport_draft AS draft, passport AS published,
                   passport_version AS version, published_at",
        id,
        workspace_id,
    )
    .fetch_one(conn)
    .await
}

/// Drop the published copy; the draft and version stay (lot locked by the caller).
pub async fn unpublish(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<PassportRow, sqlx::Error> {
    sqlx::query_as!(
        PassportRow,
        "UPDATE lots SET passport = NULL, published_at = NULL, updated_at = now()
         WHERE id = $1 AND workspace_id = $2
         RETURNING passport_draft AS draft, passport AS published,
                   passport_version AS version, published_at",
        id,
        workspace_id,
    )
    .fetch_one(conn)
    .await
}

/// A published lot for the public page, with its product's public fields.
pub struct PublicRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub code: String,
    pub public_code: String,
    pub status: String,
    pub produced_on: Option<NaiveDate>,
    pub best_before: Option<NaiveDate>,
    pub expires_on: Option<NaiveDate>,
    pub origin: Option<String>,
    pub passport: Value,
    pub version: i32,
    pub published_at: DateTime<Utc>,
    pub product_name: String,
    pub brand: Option<String>,
    pub short_description: Option<String>,
}

/// The published lot with this public code (of `workspace_id` when given): one statement,
/// so unknown, unpublished and foreign codes cost the same.
pub async fn public(
    db: impl PgExecutor<'_>,
    public_code: &str,
    workspace_id: Option<Uuid>,
) -> Result<Option<PublicRow>, sqlx::Error> {
    sqlx::query_as!(
        PublicRow,
        r#"SELECT l.id, l.workspace_id, l.code, l.public_code, l.status, l.produced_on, l.best_before,
                  l.expires_on, l.origin, l.passport AS "passport!", l.passport_version AS version,
                  l.published_at AS "published_at!", p.name AS product_name, p.brand,
                  p.short_description
           FROM lots l
           JOIN products p ON p.workspace_id = l.workspace_id AND p.id = l.product_id
                AND p.deleted_at IS NULL
           WHERE l.public_code = $1 AND l.passport IS NOT NULL
             AND ($2::uuid IS NULL OR l.workspace_id = $2)"#,
        public_code,
        workspace_id,
    )
    .fetch_optional(db)
    .await
}
