//! farm_plot_documents SQL. Every read filters workspace and plot; the caller's staff scope is
//! checked on the plot first.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// A filed document with its uploader's name.
pub struct DocRow {
    pub id: Uuid,
    pub village_id: Uuid,
    pub plot_id: Uuid,
    pub storage_key: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub uploaded_by: Uuid,
    pub uploaded_by_name: String,
    pub uploaded_at: DateTime<Utc>,
}

/// The village of a live plot in the staff scope.
pub async fn plot_village(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    all: bool,
    villages: &[Uuid],
    plot: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT village_id FROM farm_plots
         WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL AND ($3 OR village_id = ANY($4))",
        ws,
        plot,
        all,
        villages,
    )
    .fetch_optional(db)
    .await
}

/// Documents of one plot, oldest first (`id` Some = just that one), and the count.
pub async fn list(
    db: impl PgExecutor<'_> + Copy,
    ws: Uuid,
    plot: Uuid,
    id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<DocRow>, i64), sqlx::Error> {
    let rows = sqlx::query_as!(
        DocRow,
        r#"SELECT d.id, d.village_id, d.plot_id, d.storage_key, d.file_name, d.content_type,
                  d.size_bytes, d.uploaded_by,
                  concat_ws(' ', u.first_name, u.last_name) AS "uploaded_by_name!", d.uploaded_at
           FROM farm_plot_documents d JOIN users u ON u.id = d.uploaded_by
           WHERE d.workspace_id = $1 AND d.plot_id = $2 AND ($3::uuid IS NULL OR d.id = $3)
           ORDER BY d.uploaded_at, d.id LIMIT $4 OFFSET $5"#,
        ws,
        plot,
        id,
        limit,
        offset,
    )
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM farm_plot_documents
           WHERE workspace_id = $1 AND plot_id = $2 AND ($3::uuid IS NULL OR id = $3)"#,
        ws,
        plot,
        id,
    )
    .fetch_one(db)
    .await?;
    Ok((rows, total))
}

/// A new document row.
pub struct NewDoc<'a> {
    pub id: Uuid,
    pub ws: Uuid,
    pub village: Uuid,
    pub plot: Uuid,
    pub storage_key: &'a str,
    pub file_name: &'a str,
    pub content_type: &'a str,
    pub size_bytes: i64,
    pub sha256: &'a [u8],
    pub uploaded_by: Uuid,
}

/// Insert; false when the id is already taken (a retry, or a race with one).
pub async fn insert(db: impl PgExecutor<'_>, d: &NewDoc<'_>) -> Result<bool, sqlx::Error> {
    let n = sqlx::query!(
        "INSERT INTO farm_plot_documents (id, workspace_id, village_id, plot_id, storage_key, file_name,
                                          content_type, size_bytes, sha256, uploaded_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (id) DO NOTHING",
        d.id,
        d.ws,
        d.village,
        d.plot,
        d.storage_key,
        d.file_name,
        d.content_type,
        d.size_bytes,
        d.sha256,
        d.uploaded_by,
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}
