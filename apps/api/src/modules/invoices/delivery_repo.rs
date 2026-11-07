//! Durable invoice delivery claims and bounded receipts; workspace predicates on every row.

use super::delivery::Receipt;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

/// One consumed delivery attempt; request is transient business input.
pub struct Attempt {
    pub invoice_id: Uuid,
    pub user_id: Uuid,
    pub action_id: Option<Uuid>,
    pub kind: String,
    pub request: Option<Value>,
    pub reserved_pdf_id: Uuid,
}

/// Numbering transaction also records intent and job before commit.
pub async fn insert(
    conn: &mut PgConnection,
    workspace: Uuid,
    invoice: Uuid,
    user: Uuid,
    kind: &str,
    request: &Value,
    action: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    let id=sqlx::query_scalar!("INSERT INTO invoice_deliveries(workspace_id,invoice_id,user_id,kind,request,action_id) VALUES($1,$2,$3,$4,$5,$6) RETURNING id",workspace,invoice,user,kind,request,action).fetch_one(&mut *conn).await?;
    let job = crate::jobs::repo::NewJob {
        workspace_id: Some(workspace),
        created_by: Some(user),
        payload: json!({"delivery_id":id}),
        dedupe_key: Some(format!("invoice_delivery:{id}")),
        ..crate::jobs::repo::NewJob::new(crate::jobs::kinds::INVOICE_DELIVERY)
    };
    crate::jobs::repo::enqueue(conn, &job).await?;
    if let Some(action) = action {
        sqlx::query!("UPDATE assistant_actions SET state='queued',evidence=jsonb_build_object('operation_id',operation_id,'delivery',jsonb_build_object('invoice_delivery_id',$3::uuid,'invoice_id',$4::uuid,'status','queued')),proposal=NULL,review_facts='{}'::jsonb WHERE workspace_id=$1 AND id=$2 AND state IN ('dispatching','unknown')",workspace,action,id,invoice).execute(&mut *conn).await?;
    }
    Ok(id)
}

/// Once-only pre-render CAS; interrupted claims are never automatically replayed.
pub async fn claim(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
) -> Result<Option<Attempt>, sqlx::Error> {
    sqlx::query_as!(Attempt,"UPDATE invoice_deliveries SET status='running',started_at=now() WHERE workspace_id=$1 AND id=$2 AND status='queued' RETURNING invoice_id,user_id,action_id,kind,request,reserved_pdf_id",workspace,id).fetch_optional(db).await
}

/// Log and fence share mail transaction before external transport.
pub async fn fence(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    log: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!("UPDATE invoice_deliveries SET status='sending',send_log_id=$3 WHERE workspace_id=$1 AND id=$2 AND status='running'",workspace,id,log).execute(db).await?.rows_affected()==1)
}

/// Owned bounded receipt, hiding internal rendering/sending details.
pub async fn receipt(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
) -> Result<Receipt, sqlx::Error> {
    sqlx::query_as!(Receipt,r#"SELECT id AS invoice_delivery_id,invoice_id,CASE WHEN status IN ('running','sending') THEN 'queued' ELSE status END AS "status!",send_log_id,pdf_file_id FROM invoice_deliveries WHERE workspace_id=$1 AND id=$2"#,workspace,id).fetch_one(db).await
}

/// Commit terminal delivery and action evidence together; no SMTP retry implied.
pub async fn finish(
    db: &PgPool,
    workspace: Uuid,
    id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    let row=sqlx::query!("UPDATE invoice_deliveries SET status=$3,request=NULL,finished_at=CASE WHEN $3='unknown' THEN NULL ELSE now() END,pdf_file_id=coalesce(pdf_file_id,(SELECT i.pdf_file_id FROM invoices i WHERE i.workspace_id=$1 AND i.id=invoice_deliveries.invoice_id)) WHERE workspace_id=$1 AND id=$2 AND status IN ('running','sending','unknown') RETURNING action_id",workspace,id,status).fetch_optional(&mut *tx).await?;
    if let Some(action) = row.and_then(|r| r.action_id) {
        let receipt = receipt(&mut *tx, workspace, id).await?;
        let evidence = json!({"delivery":receipt});
        sqlx::query!("UPDATE assistant_actions SET state=CASE WHEN $3='accepted' THEN 'queued' WHEN $3='unknown' THEN 'unknown' ELSE 'partial' END,finished_at=CASE WHEN $3='unknown' THEN NULL ELSE now() END,evidence=evidence || $4::jsonb,proposal=NULL,review_facts='{}'::jsonb WHERE workspace_id=$1 AND id=$2 AND state IN ('dispatching','queued','unknown')",workspace,action,status,evidence).execute(&mut *tx).await?;
    }
    tx.commit().await
}

/// Known mail evidence may resolve an interrupted delivery; no body capture.
pub async fn log_status(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT status FROM email_send_logs WHERE workspace_id=$1 AND id=$2",
        workspace,
        id
    )
    .fetch_optional(db)
    .await
}

/// Stale attempts become unknown; successful mail logs resolve without transport replay.
pub async fn stale(db: &PgPool) -> Result<Vec<(Uuid, Uuid)>, sqlx::Error> {
    let rows=sqlx::query!("SELECT workspace_id,id FROM invoice_deliveries WHERE (status IN ('running','sending') OR (status='unknown' AND EXISTS(SELECT 1 FROM email_send_logs l WHERE l.workspace_id=invoice_deliveries.workspace_id AND l.id=invoice_deliveries.send_log_id AND l.status IN ('accepted','blocked')))) AND started_at<now()-interval '10 minutes' ORDER BY started_at LIMIT 100").fetch_all(db).await?;
    Ok(rows.into_iter().map(|r| (r.workspace_id, r.id)).collect())
}

/// Active task list stays bounded and workspace-scoped.
pub async fn active(db: impl PgExecutor<'_>, workspace: Uuid) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',id,'invoice_delivery_id',id,'invoice_id',invoice_id,'status',CASE WHEN status IN ('running','sending') THEN 'queued' ELSE status END,'created_at',created_at) AS "task!" FROM invoice_deliveries WHERE workspace_id=$1 AND status IN ('queued','running','sending','unknown') ORDER BY created_at DESC LIMIT 100"#,workspace).fetch_all(db).await
}

/// Invoice lock serializes new sends with recipient/PDF snapshot replacement.
pub async fn pending(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    invoice: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM invoice_deliveries WHERE workspace_id=$1 AND invoice_id=$2 AND status IN ('queued','running','sending','unknown')) AS "pending!""#,workspace,invoice).fetch_one(db).await
}

/// Resolved transient delivery audit expires after 90 days; unknown work remains.
pub async fn prune(db: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("WITH expired AS (SELECT workspace_id,id FROM invoice_deliveries WHERE status IN ('accepted','blocked','manual','failed') AND finished_at<now()-interval '90 days' ORDER BY finished_at LIMIT 1000 FOR UPDATE SKIP LOCKED) DELETE FROM invoice_deliveries d USING expired e WHERE d.workspace_id=e.workspace_id AND d.id=e.id").execute(db).await?.rows_affected())
}
