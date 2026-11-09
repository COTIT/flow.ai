//! Supply correction rules (docs/farm-supply-corrections.md, D2-a..f): ops or admin file a kg
//! correction of a batch's share lines; a different ops or admin approves or rejects it.
//! Approval, one transaction: the decision, the raw lot's stock by the net kg, every past sale
//! and return attribution re-split by the corrected kg (splits rows), one passbook line per
//! farmer whose money moved, and `supply.corrected`. A paid-out farmer may go negative; the
//! payout desk already refuses payouts above the balance. Nothing earlier is changed.
//! shortcut: the "season closed" window (D2-b) waits for a season-close feature; only the
//! sold-out rule applies until then.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rust_decimal::Decimal;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        farm::{FarmAccess, Scope},
        workspace::NOT_ALLOWED,
    },
    modules::{
        farm_supply::{
            credit::{hundredths, split},
            repo as supply,
        },
        farm_supply_corrections::{
            dto::{
                NewCorrection, SupplyCorrectionLine, SupplyCorrectionResource, SupplyFarmerEffect,
            },
            repo::{self, ATTRIBUTIONS_MAX, BatchLock, CorrectionRow},
        },
        farm_surveys::dto::Money,
        stock::repo::{self as stock, Cause, Floor, Mutated, Slot},
        webhooks::events::{self, SUPPLY_CORRECTED},
    },
};

const REF_TAKEN: &str = "This client_ref belongs to another supply correction.";
const OWN_REQUEST: &str = "A correction you filed must be decided by another ops or admin.";
const DECIDED: &str = "This correction was already approved or rejected.";
const SOLD_OUT: &str = "The raw lot is sold out: its deliveries can no longer be corrected.";
const DISPATCHED: &str =
    "The raw lot was dispatched over a supply link: its kg can no longer be cut.";
const TOO_MANY: &str = "This batch has too many sales to re-split in one correction.";

fn kg(d: Decimal) -> String {
    format!("{d:.2}")
}

fn overflow() -> AppError {
    AppError::Internal("farmer credit out of range".into())
}

fn manages(access: &FarmAccess) -> Result<(), AppError> {
    if access.manages() {
        Ok(())
    } else {
        Err(AppError::Forbidden(NOT_ALLOWED))
    }
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Sold-out and dispatched windows for a correction moving the batch by `net` kg.
async fn check_window(
    conn: &mut PgConnection,
    ws: Uuid,
    b: &BatchLock,
    net: Decimal,
) -> Result<Option<(Uuid, Decimal)>, AppError> {
    let s = repo::lot_stock(&mut *conn, ws, b.lot_id).await?;
    if s.total <= Decimal::ZERO {
        return Err(AppError::field("batch", SOLD_OUT));
    }
    if net < Decimal::ZERO {
        if repo::dispatched(&mut *conn, ws, b.lot_id).await? {
            return Err(AppError::field("lines", DISPATCHED));
        }
        let free = s.warehouse.map_or(Decimal::ZERO, |w| w.1);
        if -net > free {
            return Err(AppError::field(
                "lines",
                format!(
                    "The cut of {} kg is more than the raw lot's free stock of {} kg.",
                    kg(-net),
                    kg(free)
                ),
            ));
        }
    }
    Ok(s.warehouse)
}

/// The new split of every attribution of the batch, with each farmer's kg plus `extra`
/// `(person, kg)`: rows `(attribution, person, amount)` that bring what each farmer holds per
/// attribution to `split(total, kg)`. Every attribution's rows sum to 0.
// shortcut: a return attribution is re-split by its own total; a later return evens out the
// at most one minor unit this may leave (like partial returns already do).
// shortcut: a sale committing while an approval runs keeps the old split (sales don't take the
// batch lock: they lock stock first, so sharing it could deadlock); the batch's next approved
// correction re-splits that sale too. Lock the batch in `on_sale` if this ever shows up.
async fn resplit(
    conn: &mut PgConnection,
    ws: Uuid,
    batch_id: Uuid,
    extra: &[(Uuid, Decimal)],
) -> Result<Vec<(Uuid, Uuid, i64)>, AppError> {
    let mut weights: BTreeMap<Uuid, Decimal> = BTreeMap::new();
    for w in supply::weights(&mut *conn, ws, batch_id).await? {
        weights.insert(w.person_id, w.kg);
    }
    for (p, d) in extra {
        *weights.entry(*p).or_default() += d;
    }
    let attributions = repo::attributions(&mut *conn, ws, batch_id).await?;
    if attributions.len() as i64 > ATTRIBUTIONS_MAX {
        return Err(AppError::field("batch", TOO_MANY));
    }
    let ids: Vec<Uuid> = attributions.iter().map(|a| a.0).collect();
    let mut held: HashMap<Uuid, BTreeMap<Uuid, i64>> = HashMap::new();
    for (a, p, m) in repo::held(&mut *conn, ws, &ids).await? {
        held.entry(a).or_default().insert(p, m);
    }
    let mut out = Vec::new();
    for (a, total) in attributions {
        let has = held.remove(&a).unwrap_or_default();
        let persons: BTreeSet<Uuid> = weights.keys().chain(has.keys()).copied().collect();
        let list = persons
            .iter()
            .map(|p| {
                let w = weights
                    .get(p)
                    .copied()
                    .unwrap_or_default()
                    .max(Decimal::ZERO);
                hundredths(w).map(|h| (*p, h))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(overflow)?;
        for ((p, _), want) in list.iter().zip(split(total, &list)) {
            let diff = want - has.get(p).copied().unwrap_or_default();
            if diff != 0 {
                out.push((a, *p, diff));
            }
        }
    }
    Ok(out)
}

/// Sum per person of re-split rows, persons with a zero sum left out.
fn per_person(rows: &[(Uuid, Uuid, i64)]) -> Vec<(Uuid, i64)> {
    let mut by: BTreeMap<Uuid, i64> = BTreeMap::new();
    for (_, p, m) in rows {
        *by.entry(*p).or_default() += m;
    }
    by.into_iter().filter(|(_, m)| *m != 0).collect()
}

/// The `(share, person, kg change)` lines of a request on the locked batch `b`, after every
/// filing rule (422 otherwise). Shared by filing and the voice review card.
pub async fn plan(
    conn: &mut PgConnection,
    ws: Uuid,
    b: &BatchLock,
    input: &NewCorrection,
) -> Result<Vec<(Uuid, Uuid, Decimal)>, AppError> {
    let shares = repo::shares(&mut *conn, ws, b.id).await?;
    let now = repo::share_kg(&mut *conn, ws, b.id).await?;
    let mut errors = BTreeMap::new();
    let mut lines: Vec<(Uuid, Uuid, Decimal)> = Vec::new();
    let mut up: BTreeMap<Uuid, Decimal> = BTreeMap::new();
    for (n, (share_id, person, target)) in input.lines.iter().enumerate() {
        let Some(share) = shares.iter().find(|s| s.id == *share_id) else {
            errors.insert(
                format!("lines.{n}.share_id"),
                vec!["The share line is not one of this batch.".to_owned()],
            );
            continue;
        };
        let person = person.unwrap_or(share.person_id);
        if person != share.person_id
            && !repo::person_in_village(&mut *conn, ws, b.village_id, person).await?
        {
            errors.insert(
                format!("lines.{n}.person_id"),
                vec!["The farmer must be a person of the batch's village.".to_owned()],
            );
            continue;
        }
        if lines.iter().any(|l| l.0 == share.id && l.1 == person) {
            errors.insert(
                format!("lines.{n}.share_id"),
                vec!["This share and farmer are listed twice.".to_owned()],
            );
            continue;
        }
        let before = now
            .iter()
            .find(|k| k.0 == share.id && k.1 == person)
            .map_or(Decimal::ZERO, |k| k.2);
        let delta = target - before;
        if delta.is_zero() {
            errors.insert(
                format!("lines.{n}.kg"),
                vec![format!(
                    "The farmer already has {} kg on this share.",
                    kg(before)
                )],
            );
            continue;
        }
        *up.entry(share.harvest_report_id).or_default() += delta;
        lines.push((share.id, person, delta));
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    let net: Decimal = lines.iter().map(|l| l.2).sum();
    check_window(&mut *conn, ws, b, net).await?;
    check_harvests(&mut *conn, ws, &up).await?;
    Ok(lines)
}

/// POST /supply-batches/{batch}/corrections. True = filed, false = replayed.
pub async fn file(
    state: &AppState,
    access: &FarmAccess,
    batch: &str,
    input: NewCorrection,
) -> Result<(SupplyCorrectionResource, bool), AppError> {
    manages(access)?;
    let ws = access.workspace_id;
    let batch_id = parse(batch)?;
    let hash = input.hash(batch_id);
    let replay = |r: repo::RefRow| {
        if r.requested_by != access.user_id || r.request_hash != hash {
            Err(AppError::Conflict(REF_TAKEN))
        } else {
            Ok(r.id)
        }
    };
    if let Some(r) = repo::find_ref(&state.db, ws, input.client_ref).await? {
        let id = replay(r)?;
        return Ok((get_one(state, ws, id, &access.staff_scope()).await?, false));
    }
    let mut tx = access.begin(&state.db).await?;
    let b = repo::lock_batch(&mut tx, ws, batch_id, &access.staff_scope())
        .await?
        .ok_or(AppError::NotFound)?;
    // A copy of this request that held the batch lock first has committed: replay it.
    if let Some(r) = repo::find_ref(&mut *tx, ws, input.client_ref).await? {
        drop(tx);
        let id = replay(r)?;
        return Ok((get_one(state, ws, id, &access.staff_scope()).await?, false));
    }
    let lines = plan(&mut tx, ws, &b, &input).await?;
    let row = repo::NewCorrection {
        village_id: b.village_id,
        batch_id,
        reason: &input.reason,
        client_ref: input.client_ref,
        request_hash: &hash,
        requested_by: access.user_id,
    };
    let id = repo::insert(&mut tx, ws, &row, &lines).await?;
    tx.commit().await?;
    Ok((get_one(state, ws, id, &access.staff_scope()).await?, true))
}

/// Harvests whose kg grow: committed plus the growth must stay within the harvest figure.
async fn check_harvests(
    conn: &mut PgConnection,
    ws: Uuid,
    up: &BTreeMap<Uuid, Decimal>,
) -> Result<(), AppError> {
    let ids: Vec<Uuid> = up
        .iter()
        .filter(|u| *u.1 > Decimal::ZERO)
        .map(|u| *u.0)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let harvests = supply::lock_harvests(&mut *conn, ws, &ids).await?;
    let committed = supply::committed(&mut *conn, &ids).await?;
    for h in harvests {
        let done = committed
            .iter()
            .find(|c| c.0 == h.id)
            .map_or(Decimal::ZERO, |c| c.1);
        let more = up.get(&h.id).copied().unwrap_or_default();
        if done + more > h.quantity_kg {
            return Err(AppError::field(
                "lines",
                format!(
                    "over-commitment: {} kg available, {} kg already committed, {} kg more requested",
                    kg(h.quantity_kg),
                    kg(done),
                    kg(more)
                ),
            ));
        }
    }
    Ok(())
}

/// POST /supply-corrections/{id}/approve or /reject.
pub async fn decide(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    approve: bool,
) -> Result<SupplyCorrectionResource, AppError> {
    manages(access)?;
    let ws = access.workspace_id;
    let id = parse(id)?;
    let scope = access.staff_scope();
    let mut tx = access.begin(&state.db).await?;
    let c = repo::find(&mut tx, ws, id, &scope, true)
        .await?
        .ok_or(AppError::NotFound)?;
    if c.decision.is_some() {
        return Err(AppError::Conflict(DECIDED));
    }
    if c.requested_by == access.user_id {
        return Err(AppError::Forbidden(OWN_REQUEST));
    }
    if !approve {
        repo::decide(&mut tx, ws, &c, "rejected", access.user_id).await?;
        tx.commit().await?;
        return get_one(state, ws, id, &scope).await;
    }
    let b = repo::lock_batch(&mut tx, ws, c.batch_id, &scope)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&mut tx, ws, &[id]).await?;
    let net: Decimal = lines.iter().map(|l| l.kg_delta).sum();
    let warehouse = check_window(&mut tx, ws, &b, net).await?;
    let shares = repo::shares(&mut tx, ws, b.id).await?;
    let mut up: BTreeMap<Uuid, Decimal> = BTreeMap::new();
    for l in &lines {
        if let Some(s) = shares.iter().find(|s| s.id == l.share_id) {
            *up.entry(s.harvest_report_id).or_default() += l.kg_delta;
        }
    }
    check_harvests(&mut tx, ws, &up).await?;
    repo::decide(&mut tx, ws, &c, "approved", access.user_id).await?;
    if repo::below_zero(&mut tx, ws, b.id).await? {
        return Err(AppError::field(
            "lines",
            "A farmer's kg on a share would go below 0: an earlier correction changed it.",
        ));
    }
    if !net.is_zero() {
        let Some((warehouse_id, _)) = warehouse else {
            return Err(AppError::field("batch", SOLD_OUT));
        };
        let slot = Slot {
            product_id: b.product_id,
            warehouse_id,
            bin: None,
            lot_id: Some(b.lot_id),
        };
        let cause = Cause {
            reason: Some("corrected"),
            is_correction: true,
            user_id: Some(access.user_id),
            related: Some(("farm_supply_correction", id)),
            ..Cause::default()
        };
        match stock::mutate(&mut tx, ws, slot, net, Floor::Available, &cause).await? {
            Mutated::Done { .. } => {}
            _ => {
                return Err(AppError::field(
                    "lines",
                    "The raw lot's free stock is too low.",
                ));
            }
        }
    }
    let splits = resplit(&mut tx, ws, b.id, &[]).await?;
    let farmers = per_person(&splits);
    if !splits.is_empty() {
        repo::insert_splits(&mut tx, ws, id, &splits).await?;
    }
    if !farmers.is_empty() {
        repo::insert_lines(&mut tx, ws, b.village_id, b.currency_id, id, &farmers).await?;
    }
    let payload = json!({
        "correction_id": id,
        "batch_id": b.id,
        // Thin (outbox payloads stay under 16 KiB): each farmer's amount is in the passbook.
        "farmers": farmers.len(),
        "net_kg": kg(net),
    });
    events::emit(&mut tx, ws, SUPPLY_CORRECTED, payload).await?;
    tx.commit().await?;
    get_one(state, ws, id, &scope).await
}

/// One correction with its lines and money effect.
async fn get_one(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
    scope: &Scope,
) -> Result<SupplyCorrectionResource, AppError> {
    let mut conn = state.db.acquire().await?;
    let c = repo::find(&mut conn, ws, id, scope, false)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&mut conn, ws, &[id]).await?;
    resource(&mut conn, ws, c, lines).await
}

async fn resource(
    conn: &mut PgConnection,
    ws: Uuid,
    c: CorrectionRow,
    lines: Vec<repo::LineRow>,
) -> Result<SupplyCorrectionResource, AppError> {
    let net: Decimal = lines.iter().map(|l| l.kg_delta).sum();
    let pending = c.decision.is_none();
    let now = if pending {
        repo::share_kg(&mut *conn, ws, c.batch_id).await?
    } else {
        Vec::new()
    };
    let farmers = match c.decision.as_deref() {
        None => preview(&mut *conn, ws, &c, &lines).await?,
        Some("approved") => {
            let cur = currency_of(&mut *conn, ws, c.batch_id).await?;
            repo::posted(&mut *conn, ws, c.id)
                .await?
                .into_iter()
                .map(|(person_id, name, m)| SupplyFarmerEffect {
                    person_id,
                    name,
                    change: money(&cur, m),
                    balance_after: None,
                })
                .collect()
        }
        Some(_) => Vec::new(),
    };
    Ok(SupplyCorrectionResource {
        id: c.id,
        batch_id: c.batch_id,
        village_id: c.village_id,
        status: c.decision.clone().unwrap_or_else(|| "pending".into()),
        reason: c.reason,
        requested_by: c.requested_by,
        requested_at: c.requested_at,
        decided_by: c.decided_by,
        decided_at: c.decided_at,
        net_kg: kg(net),
        lines: lines
            .into_iter()
            .map(|l| {
                let before = pending.then(|| {
                    now.iter()
                        .find(|k| k.0 == l.share_id && k.1 == l.person_id)
                        .map_or(Decimal::ZERO, |k| k.2)
                });
                SupplyCorrectionLine {
                    share_id: l.share_id,
                    person_id: l.person_id,
                    farmer_name: l.name,
                    kg_delta: kg(l.kg_delta),
                    kg_before: before.map(kg),
                    kg_after: before.map(|b| kg(b + l.kg_delta)),
                }
            })
            .collect(),
        farmers,
    })
}

/// (currency id, code, decimals) of a batch.
type Currency = (Uuid, String, i16);

async fn currency_of(conn: &mut PgConnection, ws: Uuid, batch: Uuid) -> Result<Currency, AppError> {
    repo::currency(&mut *conn, ws, batch)
        .await?
        .ok_or(AppError::NotFound)
}

fn money(c: &Currency, minor: i64) -> Money {
    Money {
        minor,
        currency: c.1.clone(),
        decimals: c.2,
        currency_id: c.0,
    }
}

/// What approving a pending correction would move per farmer, and their balance after.
async fn preview(
    conn: &mut PgConnection,
    ws: Uuid,
    c: &CorrectionRow,
    lines: &[repo::LineRow],
) -> Result<Vec<SupplyFarmerEffect>, AppError> {
    let extra: Vec<(Uuid, Decimal)> = lines.iter().map(|l| (l.person_id, l.kg_delta)).collect();
    let changes = per_person(&resplit(&mut *conn, ws, c.batch_id, &extra).await?);
    let cur = currency_of(&mut *conn, ws, c.batch_id).await?;
    let persons: Vec<Uuid> = changes.iter().map(|c| c.0).collect();
    let balances = repo::balances(&mut *conn, ws, cur.0, &persons).await?;
    let names = repo::names(&mut *conn, ws, &persons).await?;
    Ok(changes
        .into_iter()
        .map(|(p, m)| {
            let bal = balances.iter().find(|b| b.0 == p).map_or(0, |b| b.1);
            SupplyFarmerEffect {
                person_id: p,
                name: names
                    .iter()
                    .find(|n| n.0 == p)
                    .map(|n| n.1.clone())
                    .unwrap_or_default(),
                change: money(&cur, m),
                balance_after: Some(money(&cur, bal + m)),
            }
        })
        .collect())
}

/// One correction, for staff of its village.
pub async fn get(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
) -> Result<SupplyCorrectionResource, AppError> {
    if !access.is_staff() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    get_one(
        state,
        access.workspace_id,
        parse(id)?,
        &access.staff_scope(),
    )
    .await
}

/// One page of corrections of the caller's staff villages, newest first.
pub async fn list(
    state: &AppState,
    access: &FarmAccess,
    status: Option<&str>,
    batch: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SupplyCorrectionResource>, i64), AppError> {
    if !access.is_staff() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let status = status.map(str::trim).filter(|s| !s.is_empty());
    if status.is_some_and(|s| !["pending", "approved", "rejected"].contains(&s)) {
        return Err(AppError::field(
            "status",
            "The status must be pending, approved or rejected.",
        ));
    }
    let batch = match batch.map(str::trim).filter(|b| !b.is_empty()) {
        Some(b) => Some(
            Uuid::parse_str(b)
                .map_err(|_| AppError::field("batch_id", "The batch id must be a valid UUID."))?,
        ),
        None => None,
    };
    let ws = access.workspace_id;
    let scope = access.staff_scope();
    let (rows, total) = repo::list(&state.db, ws, &scope, status, batch, limit, offset).await?;
    let mut conn = state.db.acquire().await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut by: HashMap<Uuid, Vec<repo::LineRow>> = HashMap::new();
    for l in repo::lines(&mut conn, ws, &ids).await? {
        by.entry(l.correction_id).or_default().push(l);
    }
    let mut out = Vec::with_capacity(rows.len());
    // shortcut: a pending row's preview reads the batch's attributions (one page ≤ 100 rows,
    // each a few indexed reads); a batch-wide cache when pages get slow.
    for r in rows {
        let lines = by.remove(&r.id).unwrap_or_default();
        out.push(resource(&mut conn, ws, r, lines).await?);
    }
    Ok((out, total))
}

/// Voice review facts of a correction request: the batch by lot code, each farmer's kg before
/// and after, the net kg, and each farmer's money change and balance after. Runs every filing
/// rule first (422 before a card exists). The caller checked the role.
pub async fn review(
    conn: &mut PgConnection,
    ws: Uuid,
    batch_id: Uuid,
    input: &NewCorrection,
) -> Result<serde_json::Value, AppError> {
    let all = Scope {
        all: true,
        villages: Vec::new(),
    };
    let b = repo::lock_batch(&mut *conn, ws, batch_id, &all)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = plan(&mut *conn, ws, &b, input).await?;
    let now = repo::share_kg(&mut *conn, ws, b.id).await?;
    let extra: Vec<(Uuid, Decimal)> = lines.iter().map(|l| (l.1, l.2)).collect();
    let changes = per_person(&resplit(&mut *conn, ws, b.id, &extra).await?);
    let cur = currency_of(&mut *conn, ws, b.id).await?;
    let mut persons: Vec<Uuid> = lines.iter().map(|l| l.1).collect();
    persons.extend(changes.iter().map(|c| c.0));
    let names = repo::names(&mut *conn, ws, &persons).await?;
    let name = |p: Uuid| names.iter().find(|n| n.0 == p).map(|n| n.1.clone());
    let balances = repo::balances(&mut *conn, ws, cur.0, &persons).await?;
    let net: Decimal = lines.iter().map(|l| l.2).sum();
    Ok(json!({
        "kind": "supply_correction",
        "locked": {"label": b.lot_code, "batch_id": b.id},
        "lines": lines.iter().map(|(share, p, d)| {
            let before = now.iter().find(|k| k.0 == *share && k.1 == *p).map_or(Decimal::ZERO, |k| k.2);
            json!({"farmer": name(*p), "kg_before": kg(before), "kg_after": kg(before + d)})
        }).collect::<Vec<_>>(),
        "net_kg": kg(net),
        "reason": input.reason,
        // Exact minor units; the card formats `*_minor` with this currency.
        "currency": {"code": cur.1, "decimals": cur.2},
        "farmers": changes.iter().map(|(p, m)| {
            let bal = balances.iter().find(|b| b.0 == *p).map_or(0, |b| b.1);
            json!({"name": name(*p), "change_minor": m, "balance_after_minor": bal + m})
        }).collect::<Vec<_>>(),
        "approval": "office",
    }))
}
