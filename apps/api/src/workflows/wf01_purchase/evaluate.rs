//! WF-01 intake per source record, shared by poll, sweep and full scan (PLAN §6a, R3-§4,
//! R3-§5, contracts §3.1, §5.4): mappings, requester, money → snapshot + intake blocking codes,
//! then the revision rules (new, re-evaluate, invalidate, skip decided) and the incidents a source
//! change raises once an execution exists. Runs inside the caller's page transaction.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{fingerprint, money};
use crate::{
    error::AppError,
    integrations::{
        capability::{PurchaseRequest, SourceState},
        pairs::repo::PairRow,
    },
    modules::approvals::requests::{self, NewRevision},
    workflows::{
        events::{self, Actor, EventRef},
        incidents::{self, IncidentRefs},
    },
};

/// `approval_requests.workflow_key` of WF-01.
pub const WORKFLOW_KEY: &str = "wf01_purchase";

/// What intake needs about the pair.
#[derive(Debug, Clone)]
pub struct PairCtx {
    pub ws: Uuid,
    pub id: Uuid,
    pub source: Uuid,
    pub target: Uuid,
    /// Target connection status is `active`.
    pub target_active: bool,
    pub config: Value,
    /// Backfill anchor (first poll starts here minus `backfill_days`).
    pub activated_at: Option<DateTime<Utc>>,
}

impl PairCtx {
    /// Context of a pair row.
    pub fn new(pair: &PairRow, target_active: bool) -> Self {
        Self {
            ws: pair.workspace_id,
            id: pair.id,
            source: pair.source_connection_id,
            target: pair.target_connection_id,
            target_active,
            config: pair.config.clone(),
            activated_at: pair.activated_at,
        }
    }

    /// Integer setting, or the contract default (§2.1).
    pub fn setting(&self, key: &str, default: i64) -> i64 {
        self.config
            .get(key)
            .and_then(Value::as_i64)
            .unwrap_or(default)
    }

    fn location(&self, subsidiary: &str) -> Option<String> {
        self.config
            .get("default_locations")?
            .get(subsidiary)?
            .as_str()
            .map(str::to_owned)
    }

    /// `odoo:<connection>:purchase.order:<id>`.
    pub fn subject(&self, source_id: &str) -> String {
        format!("odoo:{}:purchase.order:{source_id}", self.source)
    }
}

/// What the source says about one record.
pub enum Seen<'a> {
    Found(&'a PurchaseRequest),
    Gone,
}

/// RFQ states WF-01 takes (PLAN §1b).
pub fn eligible(state: &SourceState) -> bool {
    matches!(state, SourceState::Draft | SourceState::Sent)
}

fn state_name(state: &SourceState) -> &str {
    match state {
        SourceState::Draft => "draft",
        SourceState::Sent => "sent",
        SourceState::ToApprove => "to approve",
        SourceState::Confirmed => "purchase",
        SourceState::Cancelled => "cancel",
        SourceState::Other(s) => s,
    }
}

/// Evaluation of one source revision.
pub struct Evaluated {
    pub revision: NewRevision,
}

struct Mapping {
    id: Uuid,
    target_id: String,
}

/// Snapshot (§5.4 v1), money and intake reasons for one eligible request.
pub async fn evaluate(
    tx: &mut PgConnection,
    pair: &PairCtx,
    req: &PurchaseRequest,
) -> Result<Evaluated, AppError> {
    let mut wanted: Vec<(&str, &str)> = vec![
        ("company", &req.company_id),
        ("vendor", &req.vendor_id),
        ("currency", &req.currency_id),
    ];
    // Down-payment lines block the request anyway; their product/uom lookups would only add noise.
    for l in req.lines.iter().filter(|l| !l.downpayment) {
        wanted.push(("product", &l.product_id));
        wanted.push(("uom", &l.uom_id));
    }
    if let Some(b) = &req.buyer_id {
        wanted.push(("user", b));
    }
    let entities: Vec<String> = wanted.iter().map(|(e, _)| (*e).to_owned()).collect();
    let sources: Vec<String> = wanted.iter().map(|(_, s)| (*s).to_owned()).collect();
    let found: BTreeMap<(String, String), Mapping> = sqlx::query!(
        "SELECT m.id, m.entity, m.source_id, m.target_id FROM integration_mappings m
         JOIN unnest($3::text[], $4::text[]) AS w(entity, source_id)
           ON w.entity = m.entity AND w.source_id = m.source_id
         WHERE m.workspace_id = $1 AND m.pair_id = $2",
        pair.ws,
        pair.id,
        &entities,
        &sources,
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|r| {
        (
            (r.entity, r.source_id),
            Mapping {
                id: r.id,
                target_id: r.target_id,
            },
        )
    })
    .collect();

    let mut reasons: Vec<String> = Vec::new();
    let mut mapping_ids = serde_json::Map::new();
    let mut target = |entity: &str, source: &str| -> Option<String> {
        match found.get(&(entity.to_owned(), source.to_owned())) {
            Some(m) => {
                mapping_ids.insert(format!("{entity}:{source}"), json!(m.id));
                Some(m.target_id.clone())
            }
            None => {
                let code = format!("unmapped:{entity}:{source}");
                if !reasons.contains(&code) {
                    reasons.push(code);
                }
                None
            }
        }
    };
    let label = |kind: &str, id: &str| req.labels.get(&format!("{kind}:{id}")).cloned();

    let company_t = target("company", &req.company_id);
    let vendor_t = target("vendor", &req.vendor_id);
    let currency_t = target("currency", &req.currency_id);
    let checked = money::check(req);
    let mut lines = Vec::with_capacity(req.lines.len());
    for (l, subtotal_minor) in req.lines.iter().zip(&checked.subtotals_minor) {
        let (product_t, uom_t) = if l.downpayment {
            (None, None)
        } else {
            (target("product", &l.product_id), target("uom", &l.uom_id))
        };
        lines.push(json!({
            "source_line_id": l.id,
            "product": {"source_id": l.product_id, "target_id": product_t,
                        "label": label("product", &l.product_id)},
            "uom": {"source_id": l.uom_id, "target_id": uom_t,
                    "label": label("uom", &l.uom_id)},
            "quantity": format!("{:.8}", l.quantity),
            "price_unit": format!("{:.8}", l.price_unit),
            "subtotal_minor": subtotal_minor,
            "date_planned": l.date_planned,
        }));
    }
    // A missing or non-uuid user mapping leaves no requester (C2 adds `no_requester`).
    let requester_user_id = match &req.buyer_id {
        Some(b) => target("user", b).and_then(|t| Uuid::parse_str(&t).ok()),
        None => None,
    };
    if req.lines.iter().any(|l| l.downpayment) {
        reasons.push("downpayment_line".into());
    }
    reasons.extend(checked.reasons.iter().cloned());
    if !pair.target_active {
        reasons.push(format!("connection_inactive:{}", pair.target));
    }

    let location = company_t
        .as_deref()
        .and_then(|s| pair.location(s))
        .map(|t| json!({"source_id": null, "target_id": t, "label": null}));
    let snapshot = json!({
        "v": 1,
        "source": {"connection_id": pair.source, "model": "purchase.order", "id": req.id,
                   "name": null, "state": state_name(&req.state)},
        "target": {"connection_id": pair.target, "record_type": "purchaseOrder"},
        "company": {"source_id": req.company_id, "target_id": company_t,
                    "label": label("company", &req.company_id)},
        "vendor": {"source_id": req.vendor_id, "target_id": vendor_t,
                   "label": label("vendor", &req.vendor_id)},
        "currency": {"source_id": req.currency_id, "target_id": currency_t,
                     "ref": req.currency_ref, "decimals": req.currency_decimals},
        "location": location,
        "requester": req.buyer_id.as_ref().map(|b| json!({"source_id": b, "user_id": requester_user_id})),
        "lines": lines,
        "amount_untaxed_minor": checked.untaxed_minor,
        "amount_tax_minor": checked.tax_minor,
        "amount_total_minor": checked.total_minor,
        "tax_note": "odoo_estimate_target_calculates",
        "mapping_ids": mapping_ids,
    });
    Ok(Evaluated {
        revision: NewRevision {
            workspace_id: pair.ws,
            workflow_key: WORKFLOW_KEY.into(),
            pair_id: pair.id,
            subject_ref: pair.subject(&req.id),
            fingerprint: fingerprint::of(req),
            snapshot,
            amount_minor: checked.untaxed_minor,
            currency_ref: req.currency_ref.clone(),
            currency_decimals: i16::from(req.currency_decimals),
            requester_user_id,
            blocked_reasons: reasons,
        },
    })
}

/// Latest revision of a subject with its execution, if any.
struct Latest {
    id: Uuid,
    status: String,
    fingerprint: String,
    exec_id: Option<Uuid>,
    exec_status: Option<String>,
    exec_phase: Option<String>,
    exec_resolution: Option<String>,
    exec_updated_at: Option<DateTime<Utc>>,
}

/// What intake did with one record (for tests and counters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Created,
    Unchanged,
    Reevaluated,
    Invalidated,
    Replaced,
    SkippedDecided,
    Incident(&'static str),
    Ignored,
}

/// Apply one source observation to the subject's request history (one transaction).
pub async fn intake(
    tx: &mut PgConnection,
    pair: &PairCtx,
    source_id: &str,
    seen: Seen<'_>,
) -> Result<Outcome, AppError> {
    let subject = pair.subject(source_id);
    let latest = sqlx::query_as!(
        Latest,
        r#"SELECT r.id, r.status, r.fingerprint, e.id AS "exec_id?", e.status AS "exec_status?",
               e.phase AS "exec_phase?", e.resolution AS "exec_resolution?",
               e.updated_at AS "exec_updated_at?"
           FROM approval_requests r
           LEFT JOIN workflow_executions e ON e.approval_request_id = r.id
           WHERE r.workspace_id = $1 AND r.workflow_key = $2 AND r.subject_ref = $3
           ORDER BY r.revision DESC LIMIT 1"#,
        pair.ws,
        WORKFLOW_KEY,
        subject,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let found = match &seen {
        Seen::Found(r) if eligible(&r.state) => Some(*r),
        _ => None,
    };
    let Some(l) = latest else {
        return match found {
            Some(req) => create(tx, pair, req).await,
            None => Ok(Outcome::Ignored),
        };
    };
    match l.status.as_str() {
        "blocked" | "pending" | "approved" if l.exec_id.is_some() => {
            watch_execution(tx, pair, &subject, &l, &seen).await
        }
        "blocked" | "pending" | "approved" => match found {
            Some(req) => {
                let ev = evaluate(tx, pair, req).await?;
                if ev.revision.fingerprint == l.fingerprint {
                    reevaluate(tx, pair, &l, ev).await
                } else {
                    invalidate(tx, pair, l.id, "source_changed").await?;
                    create_evaluated(tx, pair, ev).await?;
                    Ok(Outcome::Replaced)
                }
            }
            None => {
                let reason = match &seen {
                    Seen::Gone => "source_gone",
                    Seen::Found(r) if matches!(r.state, SourceState::Cancelled) => {
                        "source_cancelled"
                    }
                    Seen::Found(_) => "source_ineligible",
                };
                invalidate(tx, pair, l.id, reason).await?;
                Ok(Outcome::Invalidated)
            }
        },
        "rejected" | "returned" => match found {
            Some(req) if fingerprint::of(req) == l.fingerprint => {
                skip_decided(tx, pair, l.id).await?;
                Ok(Outcome::SkippedDecided)
            }
            Some(req) => create(tx, pair, req).await,
            None => Ok(Outcome::Ignored),
        },
        // invalidated: a new revision is allowed.
        _ => match found {
            Some(req) => create(tx, pair, req).await,
            None => Ok(Outcome::Ignored),
        },
    }
}

async fn create(
    tx: &mut PgConnection,
    pair: &PairCtx,
    req: &PurchaseRequest,
) -> Result<Outcome, AppError> {
    let ev = evaluate(tx, pair, req).await?;
    create_evaluated(tx, pair, ev).await?;
    Ok(Outcome::Created)
}

async fn create_evaluated(
    tx: &mut PgConnection,
    pair: &PairCtx,
    ev: Evaluated,
) -> Result<(), AppError> {
    let fp = ev.revision.fingerprint.clone();
    let row = requests::upsert_revision(tx, ev.revision).await?;
    events::record(
        &mut *tx,
        pair.ws,
        request_ref(row.id),
        "proposal",
        json!({"revision": row.revision, "status": row.status, "fingerprint": fp,
               "blocked_reasons": row.blocked_reasons}),
        Actor::System,
    )
    .await
}

/// Same fingerprint, still live: a blocked row takes the current mappings (its snapshot holds
/// mapped target ids, so it is refreshed) and the C2 checks decide blocked or pending.
async fn reevaluate(
    tx: &mut PgConnection,
    pair: &PairCtx,
    l: &Latest,
    ev: Evaluated,
) -> Result<Outcome, AppError> {
    if l.status != "blocked" {
        return Ok(Outcome::Unchanged);
    }
    let r = ev.revision;
    sqlx::query!(
        "UPDATE approval_requests SET snapshot = $3, amount_minor = $4, requester_user_id = $5,
             updated_at = now()
         WHERE workspace_id = $1 AND id = $2 AND status = 'blocked'",
        pair.ws,
        l.id,
        r.snapshot,
        r.amount_minor,
        r.requester_user_id,
    )
    .execute(&mut *tx)
    .await?;
    requests::set_blocked(tx, pair.ws, l.id, r.blocked_reasons).await?;
    Ok(Outcome::Reevaluated)
}

fn request_ref(id: Uuid) -> EventRef {
    EventRef {
        approval_request_id: Some(id),
        ..EventRef::default()
    }
}

async fn invalidate(
    tx: &mut PgConnection,
    pair: &PairCtx,
    id: Uuid,
    reason: &str,
) -> Result<(), AppError> {
    if requests::invalidate(tx, pair.ws, id, reason).await? {
        events::record(
            &mut *tx,
            pair.ws,
            request_ref(id),
            "invalidated",
            json!({"reason": reason}),
            Actor::System,
        )
        .await?;
    }
    Ok(())
}

/// One `skipped_previously_decided` event per decided request (re-reads don't repeat it).
async fn skip_decided(tx: &mut PgConnection, pair: &PairCtx, id: Uuid) -> Result<(), AppError> {
    let seen = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM workflow_events
                          WHERE approval_request_id = $1 AND kind = 'skipped_previously_decided') AS "x!""#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if !seen {
        events::record(
            &mut *tx,
            pair.ws,
            request_ref(id),
            "skipped_previously_decided",
            json!({}),
            Actor::System,
        )
        .await?;
    }
    Ok(())
}

/// R3-§5: once an execution exists intake never changes it or the approval; it only opens
/// incidents. queued/prechecking stays the executor's call.
async fn watch_execution(
    tx: &mut PgConnection,
    pair: &PairCtx,
    subject: &str,
    l: &Latest,
    seen: &Seen<'_>,
) -> Result<Outcome, AppError> {
    let status = l.exec_status.as_deref().unwrap_or_default();
    let phase = l.exec_phase.as_deref().unwrap_or_default();
    let changed = match seen {
        Seen::Gone => true,
        Seen::Found(r) => !eligible(&r.state) || fingerprint::of(r) != l.fingerprint,
    };
    let source_state = match seen {
        Seen::Gone => None,
        Seen::Found(r) => Some(state_name(&r.state).to_owned()),
    };
    let done = matches!(status, "succeeded" | "succeeded_with_variance")
        || (status == "resolved_manual" && l.exec_resolution.as_deref() != Some("closed_no_po"));
    let watch_from = Utc::now() - chrono::Duration::days(pair.setting("succeeded_watch_days", 30));
    let kind = if done {
        if l.exec_updated_at.is_some_and(|t| t < watch_from) {
            None
        } else {
            match seen {
                Seen::Gone => Some("source_gone_after_execution"),
                Seen::Found(r)
                    if matches!(r.state, SourceState::ToApprove | SourceState::Confirmed) =>
                {
                    Some("duplicate_obligation_risk")
                }
                Seen::Found(r) if eligible(&r.state) && changed => {
                    Some("source_changed_after_execution")
                }
                Seen::Found(_) => None,
            }
        }
    } else if changed
        && (status == "awaiting_reconciliation"
            || matches!(phase, "write_intent" | "write_sent" | "verifying"))
    {
        Some("source_changed_during_write")
    } else {
        None
    };
    let Some(kind) = kind else {
        return Ok(Outcome::Unchanged);
    };
    let (_, created) = incidents::open(
        tx,
        pair.ws,
        kind,
        subject,
        IncidentRefs {
            pair_id: Some(pair.id),
            approval_request_id: Some(l.id),
            execution_id: l.exec_id,
            ..IncidentRefs::default()
        },
        json!({"source_state": source_state}),
    )
    .await?;
    if created {
        events::record(
            &mut *tx,
            pair.ws,
            EventRef {
                execution_id: l.exec_id,
                approval_request_id: Some(l.id),
                attempt_no: None,
            },
            "incident_opened",
            json!({"kind": kind}),
            Actor::System,
        )
        .await?;
    }
    Ok(Outcome::Incident(kind))
}
