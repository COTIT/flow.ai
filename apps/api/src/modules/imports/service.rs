//! Import batches: create, chunk upload, synchronous validation, queued apply, reads.
use std::collections::HashMap;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

use super::{
    dto::{CHUNK_BYTES, ListQuery, MAX_CELL, MAX_INDEX, NewBatch},
    profiles,
};
use crate::{
    AppState,
    error::AppError,
    http::{
        json::Rules,
        list::{Page, Paginated},
        workspace::WorkspaceAccess,
    },
    idempotency::{self, Answer, Command},
    jobs::{self, kinds, repo::NewJob},
    modules::accounting::{
        banking,
        dto::{Action, Create},
        opening,
        service::{created, ok, stale},
    },
};

const STATUSES: &[&str] = &["staged", "valid", "invalid", "applied", "failed"];

/// One batch as the API shows it, with current-revision row counts and allowed actions.
pub async fn batch(conn: &mut PgConnection, workspace: Uuid, id: Uuid) -> Result<Value, AppError> {
    let mut b: Value = sqlx::query_scalar(
        "SELECT to_jsonb(b) - 'workspace_id' - 'client_ref' - 'applied_by'
             || jsonb_build_object('counts', (
                 SELECT jsonb_build_object(
                     'staged', count(*) FILTER (WHERE status = 'staged'),
                     'valid', count(*) FILTER (WHERE status = 'valid'),
                     'invalid', count(*) FILTER (WHERE status = 'invalid'),
                     'applied', count(*) FILTER (WHERE status = 'applied'),
                     'failed', count(*) FILTER (WHERE status = 'failed'),
                     'received', count(*))
                 FROM import_rows r WHERE r.batch_id = b.id AND r.revision = b.revision))
         FROM import_batches b WHERE b.workspace_id = $1 AND b.id = $2",
    )
    .bind(workspace)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let actions: &[&str] = match b["state"].as_str() {
        Some("uploading") => &["upload", "validate"],
        Some("validated") if b["counts"]["valid"].as_i64().unwrap_or(0) > 0 => {
            &["validate", "apply"]
        }
        Some("validated") => &["validate"],
        _ => &[],
    };
    b["allowed_actions"] = json!(actions);
    Ok(b)
}

/// Lock the batch row for a state change; 404 outside the workspace.
async fn lock(
    conn: &mut PgConnection,
    workspace: Uuid,
    id: Uuid,
) -> Result<(String, i64, i32, i32, String, Value), AppError> {
    let row = sqlx::query(
        "SELECT state, version, revision, expected_rows, profile, mapping FROM import_batches
         WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(workspace)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok((
        row.get(0),
        row.get(1),
        row.get(2),
        row.get(3),
        row.get(4),
        row.get(5),
    ))
}

/// POST /imports.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<super::dto::CreatePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "imports_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let NewBatch {
        profile,
        filename,
        file_hash,
        expected_rows,
        mapping,
    } = &cmd.input.payload;
    if profile == banking::PROFILE {
        banking::header(&mut tx, ws, mapping).await?;
    }
    if opening::is_profile(profile) {
        opening::header(&mut tx, ws).await?;
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO import_batches (workspace_id, created_by, client_ref, profile, filename, file_hash, expected_rows, mapping)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(ws)
    .bind(access.user_id)
    .bind(cmd.input.client_ref)
    .bind(profile)
    .bind(filename)
    .bind(file_hash)
    .bind(expected_rows)
    .bind(mapping)
    .fetch_one(&mut *tx)
    .await?;
    let answer = created(&batch(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Normalize and check one chunk's rows against the batch; refusals name `payload.rows.N.*`.
fn chunk_rows(rows: &[Value], profile: &str, expected_rows: i32) -> Result<Value, AppError> {
    let mut r = Rules::default();
    let mut out = Vec::with_capacity(rows.len());
    let mut seen = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let at = |f: &str| format!("payload.rows.{i}.{f}");
        let number = r.integer(
            &at("row_number"),
            &row["row_number"],
            1,
            i64::from(expected_rows),
        );
        if let Some(n) = number {
            r.check(
                &at("row_number"),
                !seen.contains(&n),
                "Each row number appears once.",
            );
            seen.push(n);
        }
        let key = match &row["external_key"] {
            Value::Null => None,
            Value::String(s) => {
                r.max_chars(&at("external_key"), s, MAX_CELL);
                Some(s.clone())
            }
            _ => {
                r.add(&at("external_key"), "The external key must be text.");
                None
            }
        };
        let Some(fields) = row["fields"].as_object() else {
            r.add(
                &at("fields"),
                "The fields must be an object of column text.",
            );
            continue;
        };
        for (name, value) in fields {
            let f = at(&format!("fields.{name}"));
            r.check(
                &f,
                profiles::allows(profile, name),
                format!("The column {name} cannot be imported here."),
            );
            match value.as_str() {
                Some(s) => r.max_chars(&f, s, MAX_CELL),
                None => r.add(&f, "Each cell must be text."),
            }
        }
        out.push(json!({"row_number": number, "external_key": key, "fields": fields}));
    }
    r.into_result()?;
    Ok(Value::Array(out))
}

/// PUT /imports/{id}/chunks/{index}: same rows again replay; other rows for a stored index are refused.
pub async fn upload(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    index: i64,
    cmd: Command<Action<super::dto::ChunkPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "imports_upload";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (status, version, revision, expected_rows, profile, _) = lock(&mut tx, ws, id).await?;
    if !(0..=MAX_INDEX).contains(&index) {
        return Err(AppError::field(
            "index",
            "The chunk index must be between 0 and 24.",
        ));
    }
    let rows = chunk_rows(&cmd.input.payload, &profile, expected_rows)?;
    let encoded = rows.to_string();
    if encoded.len() > CHUNK_BYTES {
        return Err(AppError::PayloadTooLarge);
    }
    let hash = hex::encode(Sha256::digest(encoded.as_bytes()));
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT canonical_hash FROM import_chunks WHERE batch_id = $1 AND revision = $2 AND chunk_index = $3",
    )
    .bind(id)
    .bind(revision)
    .bind(index as i32)
    .fetch_optional(&mut *tx)
    .await?;
    match stored {
        Some(h) if h == hash => {}
        Some(_) => {
            return Err(AppError::Conflict(
                "This chunk was already uploaded with other rows. Start a new import for changed content.",
            ));
        }
        None => {
            if status != "uploading" {
                return Err(stale());
            }
            if version != cmd.input.expected_version {
                return Err(stale());
            }
            let taken: Option<i32> = sqlx::query_scalar(
                "SELECT row_number FROM import_rows WHERE batch_id = $1 AND revision = $2
                   AND row_number IN (SELECT (r->>'row_number')::int FROM jsonb_array_elements($3) r) LIMIT 1",
            )
            .bind(id)
            .bind(revision)
            .bind(&rows)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(n) = taken {
                return Err(AppError::field(
                    "payload.rows",
                    format!("Row {n} was already uploaded in another chunk."),
                ));
            }
            sqlx::query(
                "INSERT INTO import_chunks (workspace_id, batch_id, revision, chunk_index, row_count, encoded_bytes, canonical_hash)
                 VALUES ($1, $2, $3, $4, jsonb_array_length($5), $6, $7)",
            )
            .bind(ws)
            .bind(id)
            .bind(revision)
            .bind(index as i32)
            .bind(&rows)
            .bind(encoded.len() as i32)
            .bind(&hash)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO import_rows (workspace_id, batch_id, revision, chunk_index, row_number, external_key, fields)
                 SELECT $1, $2, $3, $4, (r->>'row_number')::int, r->>'external_key', r->'fields'
                 FROM jsonb_array_elements($5) r",
            )
            .bind(ws)
            .bind(id)
            .bind(revision)
            .bind(index as i32)
            .bind(&rows)
            .execute(&mut *tx)
            .await?;
            sqlx::query("UPDATE import_batches SET updated_at = now() WHERE id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    let answer = ok(&batch(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// POST /imports/{id}/validate: complete revision required; every row decoded natively and matched exactly.
pub async fn validate(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<super::dto::ValidatePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "imports_validate";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (status, version, revision, expected_rows, profile, mapping) =
        lock(&mut tx, ws, id).await?;
    if !matches!(status.as_str(), "uploading" | "validated")
        || version != cmd.input.expected_version
    {
        return Err(stale());
    }
    let chunks = cmd.input.payload;
    let (count, max, rows): (i64, Option<i32>, Option<i64>) = sqlx::query_as(
        "SELECT count(*), max(chunk_index), sum(row_count) FROM import_chunks WHERE batch_id = $1 AND revision = $2",
    )
    .bind(id)
    .bind(revision)
    .fetch_one(&mut *tx)
    .await?;
    if count != i64::from(chunks)
        || max != Some(chunks - 1)
        || rows != Some(i64::from(expected_rows))
    {
        return Err(AppError::field(
            "payload.expected_chunks",
            format!(
                "Upload every chunk before validating: {} of {expected_rows} rows in {count} of {chunks} chunks received.",
                rows.unwrap_or(0)
            ),
        ));
    }
    let staged = sqlx::query("SELECT id, fields, external_key FROM import_rows WHERE batch_id = $1 AND revision = $2 ORDER BY row_number")
        .bind(id)
        .bind(revision)
        .fetch_all(&mut *tx)
        .await?;
    let (mut ids, mut statuses, mut issues, mut targets) = (vec![], vec![], vec![], vec![]);
    let mut keys: HashMap<String, Vec<usize>> = HashMap::new();
    if profile == banking::PROFILE {
        // One statement: rows are checked together (bank ids, reference, balance).
        let header = banking::header(&mut tx, ws, &mapping).await?;
        let rows: Vec<(Value, Option<String>)> =
            staged.iter().map(|r| (r.get(1), r.get(2))).collect();
        let (found, _) = banking::check_rows(&mut tx, ws, &header, &rows).await?;
        for (row, issue) in staged.iter().zip(found) {
            ids.push(row.get::<Uuid, _>(0));
            statuses.push(if issue.is_some() { "invalid" } else { "valid" });
            issues.push(json!(issue.into_iter().collect::<Vec<_>>()));
            targets.push(None);
        }
    } else if opening::is_profile(&profile) {
        // One opening schedule: rows are checked together (one row per source).
        let rows: Vec<Value> = staged.iter().map(|r| r.get(1)).collect();
        let (found, _) = opening::check_rows(&mut tx, ws, &profile, &mapping, &rows).await?;
        for (row, issue) in staged.iter().zip(found) {
            ids.push(row.get::<Uuid, _>(0));
            statuses.push(if issue.is_some() { "invalid" } else { "valid" });
            issues.push(json!(issue.into_iter().collect::<Vec<_>>()));
            targets.push(None);
        }
    }
    let whole = profile == banking::PROFILE || opening::is_profile(&profile);
    for row in staged.iter().filter(|_| !whole) {
        let (row_id, fields, key): (Uuid, Value, Option<String>) =
            (row.get(0), row.get(1), row.get(2));
        let outcome =
            profiles::check(&mut tx, ws, &profile, &mapping, &fields, key.as_deref()).await;
        if profile == "products"
            && let Some(k) = profiles::product_key(&mapping, &fields, key.as_deref())
        {
            keys.entry(k.to_owned()).or_default().push(ids.len());
        }
        ids.push(row_id);
        match outcome {
            Ok(target) => {
                statuses.push("valid");
                issues.push(json!([]));
                targets.push(target);
            }
            Err(e) => {
                statuses.push("invalid");
                issues.push(json!([profiles::issue(&e)]));
                targets.push(None);
            }
        }
    }
    for members in keys.values().filter(|m| m.len() > 1) {
        for &i in members {
            statuses[i] = "invalid";
            issues[i] = json!([{"code": "duplicate_key", "message": "Another row in this file uses the same key. Keep one row per product."}]);
            targets[i] = None;
        }
    }
    sqlx::query(
        "UPDATE import_rows r SET status = u.status, issues = u.issues, target_id = u.target
         FROM unnest($1::uuid[], $2::text[], $3::jsonb[], $4::uuid[]) AS u(id, status, issues, target)
         WHERE r.id = u.id",
    )
    .bind(&ids)
    .bind(&statuses)
    .bind(&issues)
    .bind(&targets)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE import_batches b SET state = 'validated', expected_chunks = $2, version = version + 1, updated_at = now(),
             validation_fingerprint = encode(sha256(convert_to(b.revision || ':' ||
                 (SELECT string_agg(canonical_hash, ',' ORDER BY chunk_index) FROM import_chunks c WHERE c.batch_id = b.id AND c.revision = b.revision)
                 || ':' ||
                 (SELECT string_agg(r.id || r.status || coalesce(r.target_id::text, '-'), ',' ORDER BY r.row_number) FROM import_rows r WHERE r.batch_id = b.id AND r.revision = b.revision),
             'UTF8')), 'hex')
         WHERE b.id = $1",
    )
    .bind(id)
    .bind(chunks)
    .execute(&mut *tx)
    .await?;
    let answer = ok(&batch(&mut tx, ws, id).await?);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Native permissions a batch's rows need (besides `import.apply`).
pub fn native_permission(profile: &str, mapping: &Value) -> &'static str {
    match (profile, mapping["operation"].as_str()) {
        ("relations", _) => "relation.create",
        (banking::PROFILE, _) => banking::PERMISSION,
        (p, _) if opening::is_profile(p) => opening::PERMISSION,
        (_, Some("update")) => "product.update",
        _ => "product.create",
    }
}

/// POST /imports/{id}/apply: fingerprint must match; queues the per-row job (202).
pub async fn apply(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<super::dto::ApplyPayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "imports_apply";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let (status, version, _, _, profile, mapping) = lock(&mut tx, ws, id).await?;
    if !access.allows(native_permission(&profile, &mapping)) {
        return Err(AppError::Forbidden(crate::http::workspace::NOT_ALLOWED));
    }
    let fingerprint: Option<String> =
        sqlx::query_scalar("SELECT validation_fingerprint FROM import_batches WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if status != "validated"
        || version != cmd.input.expected_version
        || fingerprint.as_deref() != Some(cmd.input.payload.as_str())
    {
        return Err(stale());
    }
    let current = batch(&mut tx, ws, id).await?;
    if (profile == banking::PROFILE || opening::is_profile(&profile))
        && current["counts"]["invalid"].as_i64().unwrap_or(0) > 0
    {
        return Err(AppError::Unprocessable(
            "Every row must be valid. Correct the file and start a new import.".into(),
        ));
    }
    if current["counts"]["valid"].as_i64().unwrap_or(0) == 0 {
        return Err(AppError::Unprocessable(
            "No valid rows to apply. Correct the file and start a new import.".into(),
        ));
    }
    sqlx::query("UPDATE import_batches SET state = 'applying', applied_by = $2, version = version + 1, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(access.user_id)
        .execute(&mut *tx)
        .await?;
    enqueue(&mut tx, ws, id).await?;
    let answer = Answer {
        status: StatusCode::ACCEPTED.as_u16(),
        body: json!({"data": batch(&mut tx, ws, id).await?})
            .to_string()
            .into_bytes(),
    };
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

async fn enqueue(conn: &mut PgConnection, workspace: Uuid, id: Uuid) -> Result<(), AppError> {
    jobs::repo::enqueue(
        conn,
        &NewJob {
            workspace_id: Some(workspace),
            payload: json!({"batch_id": id}),
            concurrency_key: Some(format!("import:{id}")),
            ..NewJob::new(kinds::IMPORT_APPLY)
        },
    )
    .await?;
    Ok(())
}

/// Queue the apply job again (restart after a crash; tests).
pub async fn enqueue_apply(db: &PgPool, workspace: Uuid, id: Uuid) -> Result<(), AppError> {
    let mut tx = db.begin().await?;
    enqueue(&mut tx, workspace, id).await?;
    tx.commit().await?;
    Ok(())
}

/// GET /imports: newest first.
pub async fn list(
    db: &PgPool,
    workspace: Uuid,
    q: &ListQuery,
) -> Result<Paginated<Value>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let mut conn = db.acquire().await?;
    let ids: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT id, count(*) OVER () FROM import_batches WHERE workspace_id = $1
         ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3",
    )
    .bind(workspace)
    .bind(page.size)
    .bind(page.offset())
    .fetch_all(&mut *conn)
    .await?;
    let total = match ids.first() {
        Some((_, t)) => *t,
        None => {
            sqlx::query_scalar("SELECT count(*) FROM import_batches WHERE workspace_id = $1")
                .bind(workspace)
                .fetch_one(&mut *conn)
                .await?
        }
    };
    let mut data = Vec::with_capacity(ids.len());
    for (id, _) in ids {
        data.push(batch(&mut conn, workspace, id).await?);
    }
    Ok(Paginated::new(data, total, page))
}

/// GET /imports/{id}/rows: current revision, optional status filter (review and results).
pub async fn rows(
    db: &PgPool,
    workspace: Uuid,
    id: Uuid,
    q: &ListQuery,
) -> Result<Paginated<Value>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    if let Some(s) = &q.status
        && !STATUSES.contains(&s.as_str())
    {
        return Err(AppError::field(
            "status",
            "Use staged, valid, invalid, applied or failed.",
        ));
    }
    let mut conn = db.acquire().await?;
    batch(&mut conn, workspace, id).await?;
    let found: Vec<(Value, i64)> = sqlx::query_as(
        "SELECT jsonb_build_object('id', r.id, 'row_number', r.row_number, 'external_key', r.external_key,
                    'fields', r.fields, 'status', r.status, 'issues', r.issues, 'target_id', r.target_id,
                    'result_id', r.result_id, 'applied_at', r.applied_at), count(*) OVER ()
         FROM import_rows r JOIN import_batches b ON b.id = r.batch_id AND b.revision = r.revision
         WHERE b.workspace_id = $1 AND b.id = $2 AND ($3::text IS NULL OR r.status = $3)
         ORDER BY r.row_number LIMIT $4 OFFSET $5",
    )
    .bind(workspace)
    .bind(id)
    .bind(&q.status)
    .bind(page.size)
    .bind(page.offset())
    .fetch_all(&mut *conn)
    .await?;
    let total = found.first().map_or(0, |(_, t)| *t);
    Ok(Paginated::new(
        found.into_iter().map(|(v, _)| v).collect(),
        total,
        page,
    ))
}
