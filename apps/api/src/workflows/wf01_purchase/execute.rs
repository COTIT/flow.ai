//! `wf01_execute` (C4b, R3-§5b, P-1, P-2): claim or take over, W1 gate, pre-checks, cheap
//! existence guard, write intent (attempt row) committed before the one POST, fenced outcome,
//! read-back verify. A worker that lost ownership only leaves `stale_worker_result` evidence.

use std::{collections::BTreeMap, str::FromStr};

use rust_decimal::Decimal;
use serde_json::{Map, Value, json};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::{conn_ctx, connection, evaluate, fingerprint, money, reconcile, verify};
use crate::{
    error::AppError,
    integrations::{
        capability::{
            CreateOutcome, ErrorClass, NewPo, NewPoLine, ProviderError, PurchaseOrderSink,
            PurchaseRequestSource, SourceState,
        },
        connections,
        pairs::repo as pairs,
        providers::{SinkClient, SourceClient},
    },
    jobs::{
        kinds::{Ctx, WF01_EXECUTE},
        repo::{self as jobs, Claimed, NewJob},
        runner::Step,
    },
    workflows::{
        events::{self, Actor, EventRef},
        executions::{self, Patch, Status},
        incidents::{self, IncidentRefs},
        recovery::{category, expectations},
    },
};

/// Readable details (no raw codes on screen).
const OWNER_LOST: &str =
    "A run stopped while sending the order. Flow is checking whether NetSuite created it.";
const AMBIGUOUS: &str =
    "NetSuite did not answer in time. Flow is checking whether the order was created.";
const BUSY: &str =
    "NetSuite was busy and blocked the request; a reconciler can allow one more attempt.";
const NOT_SENT: &str = "The order was not sent: NetSuite could not be reached. A reconciler can allow one more attempt.";
const CHANGED_AFTER_SEND: &str = "The Odoo order changed after an order was sent to NetSuite. Flow is checking NetSuite before anything else happens.";
const WINDOW: &str =
    "Waiting until the earlier request can no longer land in NetSuite before trying again.";

/// One execute or reconcile run: the job's lease token is the owner token (P-2).
#[derive(Debug, Clone, Copy)]
pub struct Job {
    pub ws: Uuid,
    pub execution_id: Uuid,
    pub token: Uuid,
    pub attempt: i32,
    pub max_attempts: i32,
}

impl Job {
    /// From a claimed `wf01_execute` / `wf01_reconcile` job.
    pub fn of(job: &Claimed) -> Result<Self, AppError> {
        let ws = job
            .workspace_id
            .ok_or_else(|| AppError::Internal("wf01 job without workspace".into()))?;
        let execution_id = job
            .payload
            .get("execution_id")
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| AppError::Internal("wf01 job without execution_id".into()))?;
        Ok(Self {
            ws,
            execution_id,
            token: job.token,
            attempt: job.attempts,
            max_attempts: job.max_attempts,
        })
    }
}

/// `wf01_execute` job: dedupe on the execution id (contracts §6).
pub fn job(ws: Uuid, execution_id: Uuid) -> NewJob {
    NewJob {
        workspace_id: Some(ws),
        payload: json!({"execution_id": execution_id}),
        dedupe_key: Some(execution_id.to_string()),
        max_attempts: 5,
        ..NewJob::new(WF01_EXECUTE)
    }
}

/// Execution + its approved request, pair and newest attempt.
pub(super) struct Loaded {
    pub id: Uuid,
    pub status: String,
    pub phase: String,
    pub correlation_id: String,
    pub remote_attempts: i16,
    pub approval_id: Uuid,
    pub fingerprint: String,
    pub snapshot: Value,
    pub required_permission: Option<String>,
    pub decided_by: Option<Uuid>,
    pub subject_ref: String,
    pub pair_id: Uuid,
    pub source_connection_id: Uuid,
    pub target_connection_id: Uuid,
    pub latest_no: Option<i16>,
    pub latest_outcome: Option<String>,
}

impl Loaded {
    pub fn at(&self) -> EventRef {
        EventRef {
            execution_id: Some(self.id),
            ..EventRef::default()
        }
    }

    pub fn target_ref(&self, po_id: &str) -> String {
        format!(
            "netsuite:{}:purchaseOrder:{po_id}",
            self.target_connection_id
        )
    }

    pub fn refs(&self) -> IncidentRefs {
        IncidentRefs {
            pair_id: Some(self.pair_id),
            connection_id: Some(self.target_connection_id),
            approval_request_id: Some(self.approval_id),
            execution_id: Some(self.id),
        }
    }
}

pub(super) async fn load(
    db: impl PgExecutor<'_>,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<Loaded>, AppError> {
    Ok(sqlx::query_as!(
        Loaded,
        r#"SELECT e.id, e.status, e.phase, e.correlation_id, e.remote_attempts,
                  r.id AS approval_id, r.fingerprint, r.snapshot, r.required_permission,
                  r.decided_by, r.subject_ref, p.id AS pair_id, p.source_connection_id,
                  p.target_connection_id,
                  a.attempt_no AS "latest_no?", a.outcome AS "latest_outcome?"
           FROM workflow_executions e
           JOIN approval_requests r ON r.workspace_id = e.workspace_id AND r.id = e.approval_request_id
           JOIN integration_pairs p ON p.workspace_id = r.workspace_id AND p.id = r.pair_id
           LEFT JOIN LATERAL (SELECT x.attempt_no, x.outcome FROM workflow_execution_attempts x
                              WHERE x.workspace_id = e.workspace_id AND x.execution_id = e.id ORDER BY x.attempt_no DESC LIMIT 1) a ON true
           WHERE e.workspace_id = $1 AND e.id = $2"#,
        ws,
        id,
    )
    .fetch_optional(db)
    .await?)
}

/// DB text of an error class (contracts §2.2 `error_class`).
pub(super) fn class_str(c: ErrorClass) -> &'static str {
    match c {
        ErrorClass::Transient => "transient",
        ErrorClass::Ambiguous => "ambiguous",
        ErrorClass::Auth => "auth",
        ErrorClass::Permission => "permission",
        ErrorClass::NotFound => "not_found",
        ErrorClass::Validation => "validation",
        ErrorClass::Conflict => "conflict",
        ErrorClass::Unknown => "unknown",
    }
}

fn detail(text: &str) -> String {
    text.chars().take(1000).collect()
}

/// Job entry: build both clients from the pair's connections, then execute.
pub async fn run(ctx: &Ctx, job: &Claimed) -> Result<Step, AppError> {
    let j = Job::of(job)?;
    let Some(e) = load(&ctx.db, j.ws, j.execution_id).await? else {
        return Ok(Step::Done);
    };
    let cc = conn_ctx(ctx)?;
    let src = connections::load(&ctx.db, j.ws, e.source_connection_id).await?;
    let tgt = connections::load(&ctx.db, j.ws, e.target_connection_id).await?;
    let source = SourceClient::for_connection(&cc, &src);
    let sink = SinkClient::for_connection(&cc, &tgt);
    execute(
        &ctx.db,
        &j,
        source.as_ref().map_err(Clone::clone),
        sink.as_ref().map_err(Clone::clone),
    )
    .await
}

/// One execute run (contracts §3.2-§3.4). Testable with fake clients.
pub async fn execute(
    db: &PgPool,
    j: &Job,
    source: Result<&SourceClient, ProviderError>,
    sink: Result<&SinkClient, ProviderError>,
) -> Result<Step, AppError> {
    if !own(db, j).await? {
        return Ok(Step::Done);
    }
    let Some(e) = load(db, j.ws, j.execution_id).await? else {
        return Ok(Step::Done);
    };
    // W1: a write may be in flight or done; never POST again, reconcile.
    if e.phase != "prechecking" || (e.latest_no.is_some() && e.latest_outcome.is_none()) {
        return owner_lost(db, j, &e).await;
    }
    let Some(pair) = pairs::find(db, j.ws, e.pair_id).await? else {
        return Ok(Step::Done);
    };
    if pair.status != "active" {
        return block(db, j, &e, "pair_paused", None, None).await;
    }
    let src = connection(db, j.ws, e.source_connection_id).await?;
    let tgt = connection(db, j.ws, e.target_connection_id).await?;
    for c in [&src, &tgt] {
        if let Some(reason) = conn_reason(&c.status) {
            return block(db, j, &e, reason, None, None).await;
        }
    }
    let (source, sink) = match (source, sink) {
        (Ok(s), Ok(t)) => (s, t),
        (Err(err), _) | (_, Err(err)) => {
            let class = Some(class_str(err.class));
            return block(db, j, &e, "connection_inactive", class, Some(err.detail)).await;
        }
    };

    // Source re-read (R3-§5 row 2): only the owning executor cancels before write intent.
    let src_id = e.snapshot["source"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let found = match source.read(std::slice::from_ref(&src_id)).await {
        Ok((found, _)) => found.into_iter().find(|r| r.id == src_id),
        Err(err) => return provider_fail(db, j, &e, e.source_connection_id, err).await,
    };
    let changed = match &found {
        None => Some("source_gone"),
        Some(r) if !evaluate::eligible(&r.state) => Some(match r.state {
            SourceState::Cancelled => "source_cancelled",
            _ => "source_ineligible",
        }),
        Some(r) if fingerprint::of(r) != e.fingerprint => Some("source_changed"),
        Some(_) => None,
    };
    let pair_ctx = evaluate::PairCtx::new(&pair, true);
    let Some(req) = found.as_ref().filter(|_| changed.is_none()) else {
        let seen = match &found {
            Some(r) => evaluate::Seen::Found(r),
            None => evaluate::Seen::Gone,
        };
        // I2: once a POST may have left (remote_attempts > 0), never cancel; reconcile decides.
        if e.remote_attempts > 0 {
            let kind = Some("source_changed_during_write");
            return awaiting(db, j, &e, "conflict", CHANGED_AFTER_SEND, kind).await;
        }
        let reason = changed.unwrap_or("source_changed");
        return cancel(db, j, &e, &pair_ctx, &src_id, seen, reason).await;
    };

    // C6-O1: the fingerprint leaves the flag out, so a line that became a down payment shows here.
    if req.lines.iter().any(|l| l.downpayment) {
        return block(db, j, &e, "downpayment_line", None, None).await;
    }
    // P1/P2 with C4a's verified code as it stands (P-3).
    let checked = money::check(req);
    if let Some(r) = checked.reasons.iter().find(|r| {
        matches!(
            r.as_str(),
            "rate_precision" | "untotalled_lines" | "amount_not_exact"
        )
    }) {
        // amount_not_exact has no execution code: it is a precision failure too.
        let code = if r == "untotalled_lines" {
            "untotalled_lines"
        } else {
            "rate_precision"
        };
        return block(db, j, &e, code, None, None).await;
    }
    // Q3: the snapshot values are what we send; they must equal the exact source values.
    if !snapshot_exact(&e.snapshot, req) {
        return block(db, j, &e, "rate_precision", None, None).await;
    }
    if mapping_changed(db, j.ws, &e, &pair.config).await? {
        return block(db, j, &e, "mapping_changed", None, None).await;
    }
    if !approver_holds(db, j.ws, &e).await? {
        return block(db, j, &e, "approver_revoked", None, None).await;
    }

    // Cheap guard; correctness rests on the write intent below, not on this lookup.
    match sink.find_by_external_id(&e.correlation_id).await {
        Ok(Some(po)) => return verify::settle(db, j, &e, po).await,
        Ok(None) => {}
        Err(err) if err.class == ErrorClass::Conflict => {
            return awaiting(db, j, &e, "conflict", &err.detail, Some("target_conflict")).await;
        }
        Err(err) => return provider_fail(db, j, &e, e.target_connection_id, err).await,
    }

    // Granted attempt: wait out NetSuite's 15-minute request cap (facts N5 l.244).
    if e.remote_attempts > 0
        && let Some(earliest) = window_closed(db, j.ws, e.id).await?
    {
        return wait_window(db, j, &e, earliest).await;
    }

    let po = new_po(&e).ok_or_else(|| AppError::Internal("wf01 snapshot incomplete".into()))?;
    let attempt = {
        let mut tx = db.begin().await?;
        let grant = if e.remote_attempts > 0 {
            executions::grant_for(&mut *tx, j.ws, e.id, e.remote_attempts + 1).await?
        } else {
            None
        };
        let Some(n) = executions::begin_attempt(&mut tx, j.ws, e.id, j.token, grant).await? else {
            drop(tx);
            return owner_lost(db, j, &e).await;
        };
        let watch = expectations::New {
            workflow_key: "wf01_purchase",
            correlation_id: &e.correlation_id,
            execution_id: Some(e.id),
            source_ref: &e.subject_ref,
            target_kind: "purchaseOrder",
            target_external_id: &e.correlation_id,
        };
        expectations::register(&mut tx, j.ws, &watch).await?;
        let at = EventRef {
            attempt_no: Some(n),
            ..e.at()
        };
        events::record(
            &mut *tx,
            j.ws,
            at,
            "write_intent",
            json!({"attempt_no": n}),
            Actor::System,
        )
        .await?;
        tx.commit().await?;
        n
    };
    {
        let mut tx = db.begin().await?;
        let sent = Patch {
            phase: Some("write_sent".into()),
            ..Patch::default()
        };
        let running = &[Status::Running];
        if !executions::transition(
            &mut *tx,
            j.ws,
            e.id,
            j.token,
            running,
            Status::Running,
            sent,
        )
        .await?
        {
            return Ok(Step::Done); // lost ownership before sending: no POST
        }
        executions::record_attempt_result(
            &mut *tx,
            j.ws,
            e.id,
            attempt,
            Some(chrono::Utc::now()),
            None,
            None,
        )
        .await?;
        let at = EventRef {
            attempt_no: Some(attempt),
            ..e.at()
        };
        let req = json!({"attempt_no": attempt, "method": "POST", "record": "purchaseOrder"});
        events::record(&mut *tx, j.ws, at, "api_request", req, Actor::System).await?;
        tx.commit().await?;
    }
    let outcome = sink.create(&po).await;
    after_create(db, j, &e, attempt, outcome, sink).await
}

/// Claim a queued row, or take over a running one whose owner lease is dead.
async fn own(db: &PgPool, j: &Job) -> Result<bool, AppError> {
    let mut tx = db.begin().await?;
    let how = if executions::claim(&mut *tx, j.ws, j.execution_id, j.token).await? {
        "claim"
    } else if executions::takeover(&mut *tx, j.ws, j.execution_id, j.token).await? {
        "takeover"
    } else {
        return Ok(false);
    };
    let at = EventRef {
        execution_id: Some(j.execution_id),
        ..EventRef::default()
    };
    events::record(
        &mut *tx,
        j.ws,
        at,
        how,
        json!({"job_attempt": j.attempt}),
        Actor::System,
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}

fn conn_reason(status: &str) -> Option<&'static str> {
    match status {
        "active" | "degraded" => None,
        "unauthorized" => Some("connection_unauthorized"),
        "expired" => Some("connection_expired"),
        _ => Some("connection_inactive"),
    }
}

/// W1 path: settle an open attempt as `ambiguous` (§3.4) and hand over to reconcile.
async fn owner_lost(db: &PgPool, j: &Job, e: &Loaded) -> Result<Step, AppError> {
    let mut tx = db.begin().await?;
    let patch = Patch {
        error_class: Some("ambiguous".into()),
        error_detail: Some(OWNER_LOST.into()),
        ..Patch::default()
    };
    let to = Status::AwaitingReconciliation;
    if !executions::transition(&mut *tx, j.ws, e.id, j.token, &[Status::Running], to, patch).await?
    {
        return Ok(Step::Done);
    }
    if let Some(n) = e.latest_no {
        executions::record_attempt_result(&mut *tx, j.ws, e.id, n, None, Some("ambiguous"), None)
            .await?;
    }
    jobs::enqueue(&mut tx, &reconcile::job(j.ws, e.id)).await?;
    tx.commit().await?;
    Ok(Step::Done)
}

/// Fenced `running → blocked(reason)` before any write intent, with a `precheck_failed` event.
async fn block(
    db: &PgPool,
    j: &Job,
    e: &Loaded,
    reason: &str,
    class: Option<&str>,
    text: Option<String>,
) -> Result<Step, AppError> {
    let mut tx = db.begin().await?;
    let patch = Patch {
        reason: Some(reason.into()),
        error_class: class.map(str::to_owned),
        error_detail: text.as_deref().map(detail),
        ..Patch::default()
    };
    let running = &[Status::Running];
    if executions::transition(
        &mut *tx,
        j.ws,
        e.id,
        j.token,
        running,
        Status::Blocked,
        patch,
    )
    .await?
    {
        let payload = json!({"reason": reason});
        events::record(
            &mut *tx,
            j.ws,
            e.at(),
            "precheck_failed",
            payload,
            Actor::System,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Step::Done)
}

/// Pre-write provider failure: Auth/Permission block (and mark the connection, §3.6);
/// anything else retries with backoff (nothing was written).
async fn provider_fail(
    db: &PgPool,
    j: &Job,
    e: &Loaded,
    conn: Uuid,
    err: ProviderError,
) -> Result<Step, AppError> {
    let class = Some(class_str(err.class));
    match err.class {
        ErrorClass::Auth => {
            connections::mark(db, j.ws, conn, "unauthorized", Some("auth_failed")).await?;
            block(db, j, e, "connection_unauthorized", class, Some(err.detail)).await
        }
        ErrorClass::Permission => {
            connections::mark(db, j.ws, conn, "degraded", Some("permission_denied")).await?;
            block(db, j, e, "permission_denied", class, Some(err.detail)).await
        }
        _ => retry_or_fail(db, j, e, err).await,
    }
}

/// `running → queued` and let the rail back off; on the job's last attempt `→ failed`.
async fn retry_or_fail(
    db: &PgPool,
    j: &Job,
    e: &Loaded,
    err: ProviderError,
) -> Result<Step, AppError> {
    let mut tx = db.begin().await?;
    let patch = Patch {
        error_class: Some(class_str(err.class).into()),
        error_detail: Some(detail(&err.detail)),
        ..Patch::default()
    };
    let queued = executions::transition(
        &mut *tx,
        j.ws,
        e.id,
        j.token,
        &[Status::Running],
        Status::Queued,
        patch.clone(),
    )
    .await?;
    if !queued {
        return Ok(Step::Done);
    }
    if j.attempt >= j.max_attempts {
        let failed = Patch {
            reason: Some("attempts_exhausted".into()),
            ..patch
        };
        executions::transition(
            &mut *tx,
            j.ws,
            e.id,
            j.token,
            &[Status::Queued],
            Status::Failed,
            failed,
        )
        .await?;
        tx.commit().await?;
        return Ok(Step::Done);
    }
    tx.commit().await?;
    Err(AppError::Internal(format!(
        "wf01 execute retry: {}",
        detail(&err.detail)
    )))
}

/// Pre-check source change (R3-§5 row 2): execution `cancelled`, approval `invalidated`, next
/// revision created now, one transaction.
async fn cancel(
    db: &PgPool,
    j: &Job,
    e: &Loaded,
    pair: &evaluate::PairCtx,
    src_id: &str,
    seen: evaluate::Seen<'_>,
    reason: &str,
) -> Result<Step, AppError> {
    let mut tx = db.begin().await?;
    let patch = Patch {
        reason: Some("source_changed".into()),
        ..Patch::default()
    };
    let running = &[Status::Running];
    if !executions::transition(
        &mut *tx,
        j.ws,
        e.id,
        j.token,
        running,
        Status::Cancelled,
        patch,
    )
    .await?
    {
        return Ok(Step::Done);
    }
    // C2 `requests::invalidate` refuses once an execution exists; this move is C4b's (§3.1).
    sqlx::query!(
        "UPDATE approval_requests SET status = 'invalidated', invalidated_reason = $3,
             decided_by = NULL, decided_at = NULL, updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status = 'approved'",
        j.ws,
        e.approval_id,
        reason,
    )
    .execute(&mut *tx)
    .await?;
    let payload = json!({"reason": "source_changed"});
    events::record(
        &mut *tx,
        j.ws,
        e.at(),
        "precheck_failed",
        payload,
        Actor::System,
    )
    .await?;
    let at = EventRef {
        approval_request_id: Some(e.approval_id),
        ..EventRef::default()
    };
    events::record(
        &mut *tx,
        j.ws,
        at,
        "invalidated",
        json!({"reason": reason}),
        Actor::System,
    )
    .await?;
    evaluate::intake(&mut tx, pair, src_id, seen).await?;
    tx.commit().await?;
    Ok(Step::Done)
}

fn dec(v: &Value) -> Option<Decimal> {
    Decimal::from_str(v.as_str()?).ok()
}

fn snapshot_exact(s: &Value, req: &crate::integrations::capability::PurchaseRequest) -> bool {
    let lines = s["lines"].as_array().map(Vec::as_slice).unwrap_or_default();
    lines.len() == req.lines.len()
        && lines.iter().all(|l| {
            req.lines
                .iter()
                .find(|r| Some(r.id.as_str()) == l["source_line_id"].as_str())
                .is_some_and(|r| {
                    dec(&l["quantity"]) == Some(r.quantity)
                        && dec(&l["price_unit"]) == Some(r.price_unit)
                })
        })
}

/// Target ids the snapshot holds per `entity:source_id`.
fn snapshot_targets(s: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut put = |entity: &str, v: &Value| {
        if let (Some(src), Some(t)) = (v["source_id"].as_str(), v["target_id"].as_str()) {
            out.insert(format!("{entity}:{src}"), t.to_owned());
        }
    };
    put("company", &s["company"]);
    put("vendor", &s["vendor"]);
    put("currency", &s["currency"]);
    for l in s["lines"].as_array().map(Vec::as_slice).unwrap_or_default() {
        put("product", &l["product"]);
        put("uom", &l["uom"]);
    }
    if let (Some(src), Some(u)) = (
        s["requester"]["source_id"].as_str(),
        s["requester"]["user_id"].as_str(),
    ) {
        out.insert(format!("user:{src}"), u.to_owned());
    }
    out
}

/// Any snapshot mapping deleted, re-keyed or retargeted, or the default location changed.
async fn mapping_changed(
    db: &PgPool,
    ws: Uuid,
    e: &Loaded,
    config: &Value,
) -> Result<bool, AppError> {
    let ids = &e.snapshot["mapping_ids"];
    let rows = sqlx::query!(
        r#"SELECT s.k AS "k!", m.entity || ':' || m.source_id AS "now?", m.target_id AS "target?"
           FROM jsonb_each_text($3::jsonb) AS s(k, v)
           LEFT JOIN integration_mappings m
             ON m.workspace_id = $1 AND m.pair_id = $2 AND m.id::text = s.v"#,
        ws,
        e.pair_id,
        ids,
    )
    .fetch_all(db)
    .await?;
    let want = snapshot_targets(&e.snapshot);
    let current: BTreeMap<&str, &str> = rows
        .iter()
        .filter(|r| r.now.as_deref() == Some(r.k.as_str()))
        .filter_map(|r| Some((r.k.as_str(), r.target.as_deref()?)))
        .collect();
    let moved = rows.len() != current.len()
        || want
            .iter()
            .any(|(k, t)| current.get(k.as_str()) != Some(&t.as_str()));
    let company = e.snapshot["company"]["target_id"].as_str().unwrap_or("");
    let location_now = config["default_locations"][company].as_str();
    let location_then = e.snapshot["location"]["target_id"].as_str();
    Ok(moved || location_now != location_then)
}

/// D8: the approver is still an active member who may decide with the request's permission
/// (owner, workspace_admin, the permission, or approve_high for approve), as C2 decides.
async fn approver_holds(db: &PgPool, ws: Uuid, e: &Loaded) -> Result<bool, AppError> {
    let (Some(user), Some(perm)) = (e.decided_by, e.required_permission.as_deref()) else {
        return Ok(false);
    };
    Ok(sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
             WHERE m.workspace_id = $1 AND m.user_id = $2 AND m.active AND w.deleted_at IS NULL
               AND (w.created_by_user_id = m.user_id OR EXISTS (
                    SELECT 1 FROM team_members tm
                    JOIN teams t ON t.id = tm.team_id AND t.workspace_id = $1 AND t.deleted_at IS NULL
                    JOIN team_permissions p ON p.team_id = t.id
                    WHERE tm.user_id = m.user_id
                      AND (p.permission IN ('workspace_admin', $3)
                           OR ($3 = 'integration.approve' AND p.permission = 'integration.approve_high'))))
           ) AS "x!""#,
        ws,
        user,
        perm,
    )
    .fetch_one(db)
    .await?)
}

/// Earliest time a granted attempt may write: the newest attempt's send time (write intent if
/// never marked sent) + NetSuite's 15-minute request cap (facts N5 l.244); None once past.
async fn window_closed(
    db: &PgPool,
    ws: Uuid,
    id: Uuid,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, AppError> {
    Ok(sqlx::query_scalar!(
        r#"SELECT t AS "t?" FROM (
               SELECT coalesce(sent_at, write_intent_at) + interval '15 minutes' AS t
               FROM workflow_execution_attempts WHERE workspace_id = $1 AND execution_id = $2
               ORDER BY attempt_no DESC LIMIT 1) x
           WHERE t > now()"#,
        ws,
        id,
    )
    .fetch_optional(db)
    .await?
    .flatten())
}

/// Too early: fenced `running → queued`, no write intent; the 1-minute tick backstop runs it
/// again. `retry_window_wait` is logged once per attempt, not per tick.
async fn wait_window(
    db: &PgPool,
    j: &Job,
    e: &Loaded,
    earliest: chrono::DateTime<chrono::Utc>,
) -> Result<Step, AppError> {
    let mut tx = db.begin().await?;
    let patch = Patch {
        error_detail: Some(WINDOW.into()),
        ..Patch::default()
    };
    let running = &[Status::Running];
    if !executions::transition(
        &mut *tx,
        j.ws,
        e.id,
        j.token,
        running,
        Status::Queued,
        patch,
    )
    .await?
    {
        return Ok(Step::Done);
    }
    let logged = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM workflow_events WHERE workspace_id = $1
               AND execution_id = $2 AND kind = 'retry_window_wait' AND attempt_no = $3) AS "x!""#,
        j.ws,
        e.id,
        e.latest_no,
    )
    .fetch_one(&mut *tx)
    .await?;
    if !logged {
        let at = EventRef {
            attempt_no: e.latest_no,
            ..e.at()
        };
        let p = json!({"earliest": earliest});
        events::record(&mut *tx, j.ws, at, "retry_window_wait", p, Actor::System).await?;
    }
    tx.commit().await?;
    Ok(Step::Done)
}

/// The approved action (snapshot §5.4) as the PO to create. P3: values unchanged.
pub(super) fn new_po(e: &Loaded) -> Option<NewPo> {
    let s = &e.snapshot;
    let id = |v: &Value| v["target_id"].as_str().map(str::to_owned);
    let mut lines = vec![];
    for l in s["lines"].as_array()? {
        lines.push(NewPoLine {
            item_id: id(&l["product"])?,
            unit_id: id(&l["uom"]),
            quantity: dec(&l["quantity"])?,
            rate: dec(&l["price_unit"])?,
        });
    }
    Some(NewPo {
        external_id: e.correlation_id.clone(),
        vendor_id: id(&s["vendor"])?,
        subsidiary_id: id(&s["company"])?,
        currency_id: id(&s["currency"])?,
        location_id: id(&s["location"]),
        lines,
    })
}

/// Fenced `running → awaiting_reconciliation` + reconcile job (+ incident of `incident` kind).
pub(super) async fn awaiting(
    db: &PgPool,
    j: &Job,
    e: &Loaded,
    class: &str,
    text: &str,
    incident: Option<&str>,
) -> Result<Step, AppError> {
    let mut tx = db.begin().await?;
    let patch = Patch {
        error_class: Some(class.into()),
        error_detail: Some(detail(text)),
        ..Patch::default()
    };
    let to = Status::AwaitingReconciliation;
    if !executions::transition(&mut *tx, j.ws, e.id, j.token, &[Status::Running], to, patch).await?
    {
        return Ok(Step::Done);
    }
    if let Some(kind) = incident {
        let d = json!({"detail": detail(text)});
        incidents::open(&mut tx, j.ws, kind, &e.subject_ref, e.refs(), d).await?;
        events::record(
            &mut *tx,
            j.ws,
            e.at(),
            "incident_opened",
            json!({"kind": kind}),
            Actor::System,
        )
        .await?;
    }
    jobs::enqueue(&mut tx, &reconcile::job(j.ws, e.id)).await?;
    tx.commit().await?;
    Ok(Step::Done)
}

fn evidence(err: &ProviderError) -> Value {
    let mut m = Map::new();
    if let Some(s) = err.http_status {
        m.insert("http_status".into(), json!(s));
    }
    if let Some(c) = &err.code {
        m.insert("error_code".into(), json!(c));
    }
    Value::Object(m)
}

/// Where a settled attempt sends the execution.
enum Dest<'a> {
    Awaiting {
        class: &'a str,
        text: String,
    },
    Blocked {
        reason: &'a str,
        class: &'a str,
        text: String,
    },
}

/// Stale worker (P-2): fenced writes failed, so only append the evidence.
async fn stale(db: &PgPool, j: &Job, e: &Loaded, n: i16, ev: Value) -> Result<Step, AppError> {
    let at = EventRef {
        attempt_no: Some(n),
        ..e.at()
    };
    events::record(db, j.ws, at, "stale_worker_result", ev, Actor::System).await?;
    Ok(Step::Done)
}

/// Settle a non-created attempt: fenced move first, then outcome, event, reconcile job; a
/// permanent rejection also fails the WF-03 expectation in the same transaction.
#[allow(clippy::too_many_arguments)]
async fn settle_attempt(
    db: &PgPool,
    j: &Job,
    e: &Loaded,
    n: i16,
    outcome: &str,
    ev: Value,
    dest: Dest<'_>,
    permanent: Option<&ProviderError>,
) -> Result<Step, AppError> {
    let mut tx = db.begin().await?;
    let (to, patch) = match &dest {
        Dest::Awaiting { class, text } => (
            Status::AwaitingReconciliation,
            Patch {
                error_class: Some((*class).into()),
                error_detail: Some(detail(text)),
                ..Patch::default()
            },
        ),
        Dest::Blocked {
            reason,
            class,
            text,
        } => (
            Status::Blocked,
            Patch {
                reason: Some((*reason).into()),
                error_class: Some((*class).into()),
                error_detail: Some(detail(text)),
                ..Patch::default()
            },
        ),
    };
    if !executions::transition(&mut *tx, j.ws, e.id, j.token, &[Status::Running], to, patch).await?
    {
        drop(tx);
        return stale(db, j, e, n, ev).await;
    }
    executions::record_attempt_result(
        &mut *tx,
        j.ws,
        e.id,
        n,
        None,
        Some(outcome),
        Some(ev.clone()),
    )
    .await?;
    let at = EventRef {
        attempt_no: Some(n),
        ..e.at()
    };
    let mut payload = ev;
    payload["outcome"] = json!(outcome);
    events::record(&mut *tx, j.ws, at, "api_result", payload, Actor::System).await?;
    if matches!(dest, Dest::Awaiting { .. }) {
        jobs::enqueue(&mut tx, &reconcile::job(j.ws, e.id)).await?;
    }
    if let Some(err) = permanent {
        let c = category::classify(err.class, err.http_status, &err.detail);
        expectations::fail_for_execution(&mut tx, j.ws, e.id, c, class_str(err.class)).await?;
    }
    tx.commit().await?;
    Ok(Step::Done)
}

async fn after_create(
    db: &PgPool,
    j: &Job,
    e: &Loaded,
    n: i16,
    outcome: CreateOutcome,
    sink: &SinkClient,
) -> Result<Step, AppError> {
    let err = match outcome {
        CreateOutcome::Created { id, location } => {
            let ev = json!({"http_status": 204, "target_id": id, "location": location});
            let mut tx = db.begin().await?;
            let verifying = Patch {
                phase: Some("verifying".into()),
                target_ref: Some(e.target_ref(&id)),
                ..Patch::default()
            };
            let running = &[Status::Running];
            if !executions::transition(
                &mut *tx,
                j.ws,
                e.id,
                j.token,
                running,
                Status::Running,
                verifying,
            )
            .await?
            {
                drop(tx);
                return stale(db, j, e, n, ev).await;
            }
            executions::record_attempt_result(
                &mut *tx,
                j.ws,
                e.id,
                n,
                None,
                Some("created"),
                Some(ev.clone()),
            )
            .await?;
            let at = EventRef {
                attempt_no: Some(n),
                ..e.at()
            };
            let mut payload = ev;
            payload["outcome"] = json!("created");
            events::record(&mut *tx, j.ws, at, "api_result", payload, Actor::System).await?;
            tx.commit().await?;
            return match sink.read(&id).await {
                Ok(po) => verify::settle(db, j, e, po).await,
                Err(err) => awaiting(db, j, e, class_str(err.class), &err.detail, None).await,
            };
        }
        CreateOutcome::Ambiguous(err) => {
            let dest = Dest::Awaiting {
                class: "ambiguous",
                text: AMBIGUOUS.into(),
            };
            return settle_attempt(db, j, e, n, "ambiguous", evidence(&err), dest, None).await;
        }
        CreateOutcome::Conflict(err) => {
            let dest = Dest::Awaiting {
                class: "conflict",
                text: err.detail.clone(),
            };
            return settle_attempt(db, j, e, n, "conflict", evidence(&err), dest, None).await;
        }
        CreateOutcome::Rejected(err) => err,
    };
    // Rejected: NetSuite did not create it (C3b). No status means the POST never left.
    let outcome = if err.http_status.is_some() {
        "rejected"
    } else {
        "not_sent"
    };
    let class = class_str(err.class);
    let text = err.detail.clone();
    let dest = match err.class {
        ErrorClass::Auth => {
            connections::mark(
                db,
                j.ws,
                e.target_connection_id,
                "unauthorized",
                Some("auth_failed"),
            )
            .await?;
            Dest::Blocked {
                reason: "connection_unauthorized",
                class,
                text,
            }
        }
        ErrorClass::Permission => {
            connections::mark(
                db,
                j.ws,
                e.target_connection_id,
                "degraded",
                Some("permission_denied"),
            )
            .await?;
            Dest::Blocked {
                reason: "permission_denied",
                class,
                text,
            }
        }
        ErrorClass::Validation => Dest::Blocked {
            reason: "validation_rejected",
            class,
            text,
        },
        ErrorClass::NotFound => Dest::Blocked {
            reason: "mapping_stale",
            class,
            text,
        },
        // Q1: 429 "Request blocked." (facts l.256) or never sent: a human grants the retry.
        _ => Dest::Awaiting {
            class,
            text: match err.http_status {
                Some(429) => BUSY.into(),
                None => NOT_SENT.into(),
                Some(_) => text,
            },
        },
    };
    let permanent = category::permanent(err.class).then_some(&err);
    settle_attempt(db, j, e, n, outcome, evidence(&err), dest, permanent).await
}
