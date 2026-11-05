//! Lot rules: the product must be a live stock-tracked product of the workspace, codes are
//! unique per workspace (any case), the public code is made here and never changes, dates
//! keep their order, and every change writes its trail row in the same transaction.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::{NOT_ALLOWED, WorkspaceAccess},
    idempotency,
    modules::{
        farm_supply,
        files::{repo as files, service as files_service},
        lots::{
            dto::{
                LotPatch, LotResource, LotStockLevel, NewLotInput, STATUSES, StockMutation,
                next_statuses,
            },
            passport::{FileUse, Passport},
            public::{Link, PublicFarmerGroup, PublicLot},
            repo::{self, EventRow, ListFilter, LotRow, NewEvent, PassportRow},
        },
        products::repo as products,
        stock::{
            repo::{self as stock, Cause, Floor, Mutated, Slot},
            reserve,
            service::tracks_stock,
        },
        warehouses::repo as warehouses,
        webhooks::events::{
            self, LOT_CREATED, LOT_STATUS_CHANGED, PASSPORT_PUBLISHED, SUPPLY_SOURCE_STATUS_CHANGED,
        },
    },
};

const NO_PRODUCT: &str = "The selected product id is invalid.";
const TAKEN: &str = "The code has already been taken.";
/// Files read permission (the files module's `file.get`).
const FILE_GET: &str = "file.get";
/// Crockford base32: no I, L, O, U.
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Fresh public codes tried before giving up (a collision is ~1 in 10^15 per lot).
const PUBLIC_CODE_TRIES: usize = 3;

/// 10 random Crockford chars (50 bits): the code in the QR link.
pub fn public_code() -> Result<String, AppError> {
    let mut bytes = [0u8; 10];
    getrandom::fill(&mut bytes).map_err(|e| AppError::Internal(format!("os rng: {e}")))?;
    Ok(bytes
        .iter()
        .map(|b| char::from(CROCKFORD[usize::from(b & 31)]))
        .collect())
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Made, best-before and expiry dates in order (422 per later date before `produced_on`).
fn check_dates(
    produced: Option<NaiveDate>,
    best_before: Option<NaiveDate>,
    expires: Option<NaiveDate>,
) -> Result<(), AppError> {
    let mut errors = BTreeMap::new();
    for (field, date) in [("best_before", best_before), ("expires_on", expires)] {
        if let (Some(p), Some(d)) = (produced, date)
            && d < p
        {
            let msg = format!(
                "The {} must not be before the produced on date.",
                field.replace('_', " ")
            );
            errors.insert(field.to_owned(), vec![msg]);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

/// POST /lots: the lot, its `created` event and `lot.created`, in one transaction.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    input: NewLotInput,
) -> Result<LotRow, AppError> {
    let product = products::find(&state.db, access.workspace_id, input.product_id).await?;
    if !product.as_ref().is_some_and(tracks_stock) {
        return Err(AppError::field("product_id", NO_PRODUCT));
    }
    check_dates(input.produced_on, input.best_before, input.expires_on)?;
    let mut tx = state.db.begin().await?;
    let lot = LotRow {
        id: Uuid::nil(),
        product_id: input.product_id,
        code: input.code,
        public_code: String::new(),
        status: input.status,
        produced_on: input.produced_on,
        best_before: input.best_before,
        expires_on: input.expires_on,
        origin: input.origin,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let lot = insert_lot(&mut tx, access.workspace_id, access.user_id, lot, None).await?;
    tx.commit().await?;
    Ok(lot)
}

/// Insert `lot` (its id and public code are made here), its `created` event (with `meta`) and
/// `lot.created`, in the caller's transaction. The caller has checked product and dates.
pub async fn insert_lot(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
    mut lot: LotRow,
    meta: Option<serde_json::Value>,
) -> Result<LotRow, AppError> {
    let mut inserted = None;
    for _ in 0..PUBLIC_CODE_TRIES {
        lot.public_code = public_code()?;
        inserted = repo::insert(conn, workspace_id, &lot, user_id)
            .await
            .map_err(|e| AppError::unique(e, "lots_code_unique", "code", TAKEN))?;
        if inserted.is_some() {
            break;
        }
    }
    let lot = inserted.ok_or_else(|| AppError::Internal("no free public code".into()))?;
    let event = NewEvent {
        lot_id: lot.id,
        kind: "created",
        from_status: None,
        to_status: Some(&lot.status),
        user_id,
        meta,
    };
    repo::add_event(conn, workspace_id, &event).await?;
    let payload = json!({
        "lot_id": lot.id,
        "product_id": lot.product_id,
        "code": lot.code,
        "public_code": lot.public_code,
    });
    events::emit(conn, workspace_id, LOT_CREATED, payload).await?;
    Ok(lot)
}

/// 409 when stock may not leave `lot` to be packed or sent: recalled or in quarantine.
pub fn check_sendable(lot: &LotRow) -> Result<(), AppError> {
    match lot.status.as_str() {
        "recalled" => Err(AppError::Conflict("lot_recalled")),
        "quarantine" => Err(AppError::Conflict("lot_quarantined")),
        _ => Ok(()),
    }
}

/// One lot of the workspace, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<LotRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// A lot's live stock per warehouse, after the same 404 as `get`.
pub async fn free_stock(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<Vec<repo::FreeRow>, AppError> {
    let lot = get(state, workspace_id, id).await?;
    Ok(repo::free_stock(&state.db, workspace_id, lot.id).await?)
}

/// One page, newest first. Unknown filter values match nothing.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    product_id: Option<&str>,
    status: Option<&str>,
    q: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<LotRow>, i64), AppError> {
    let product_id = product_id
        .filter(|p| !p.is_empty())
        .map(|p| Uuid::parse_str(p).unwrap_or(Uuid::nil()));
    let status = status.filter(|s| !s.is_empty());
    if status.is_some_and(|s| !STATUSES.contains(&s)) {
        return Ok((Vec::new(), 0));
    }
    // LIKE prefix: the caller's text is literal.
    let q = q.map(str::trim).filter(|q| !q.is_empty()).map(|q| {
        q.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    });
    let filter = ListFilter {
        product_id,
        status,
        q: q.as_deref(),
    };
    Ok(repo::list(&state.db, workspace_id, &filter, limit, offset).await?)
}

/// PATCH /lots/{lot}: changed fields write an `updated` event (`meta.changed`), a new
/// status a `status_changed` one; a patch that changes nothing writes nothing.
pub async fn update(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    patch: LotPatch,
) -> Result<LotRow, AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let before = repo::locked(&mut tx, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut after = before.clone();
    if let Some(c) = patch.code {
        after.code = c;
    }
    if let Some(s) = patch.status {
        after.status = s;
    }
    if let Some(d) = patch.produced_on {
        after.produced_on = d;
    }
    if let Some(d) = patch.best_before {
        after.best_before = d;
    }
    if let Some(d) = patch.expires_on {
        after.expires_on = d;
    }
    if let Some(o) = patch.origin {
        after.origin = o;
    }
    check_dates(after.produced_on, after.best_before, after.expires_on)?;
    // The code is printed on labels: once stock exists under it, it stays.
    if before.code != after.code && repo::has_stock(&mut *tx, access.workspace_id, id).await? {
        return Err(AppError::field(
            "code",
            "The code can't change once the lot holds stock.",
        ));
    }
    let changed: Vec<&str> = [
        ("code", before.code != after.code),
        ("produced_on", before.produced_on != after.produced_on),
        ("best_before", before.best_before != after.best_before),
        ("expires_on", before.expires_on != after.expires_on),
        ("origin", before.origin != after.origin),
    ]
    .into_iter()
    .filter_map(|(f, c)| c.then_some(f))
    .collect();
    let status_changed = before.status != after.status;
    if status_changed && !next_statuses(&before.status).contains(&after.status.as_str()) {
        return Err(AppError::Conflict("lot_status_final"));
    }
    if changed.is_empty() && !status_changed {
        return Ok(before);
    }
    let saved = repo::update(&mut tx, access.workspace_id, &after)
        .await
        .map_err(|e| AppError::unique(e, "lots_code_unique", "code", TAKEN))?;
    if !changed.is_empty() {
        let event = NewEvent {
            lot_id: id,
            kind: "updated",
            from_status: None,
            to_status: None,
            user_id: access.user_id,
            meta: Some(json!({ "changed": changed })),
        };
        repo::add_event(&mut tx, access.workspace_id, &event).await?;
    }
    if status_changed {
        status_moved(&mut tx, access.workspace_id, access.user_id, &saved, &before.status).await?;
    }
    tx.commit().await?;
    Ok(saved)
}

/// How far off sale a status is: released < quarantine < recalled.
fn rank(status: &str) -> u8 {
    match status {
        "released" => 0,
        "quarantine" => 1,
        _ => 2,
    }
}

/// After `lot` (locked, saved) moved from `from`: its trail row and `lot.status_changed`, and
/// each partner that got stock from it hears `supply.source_status_changed` (their data stays
/// theirs). Off sale (tighter): holds move to sellable lots, and every lot packed from it, down
/// the whole line, follows to the same status with its own row and event. Loosening never
/// cascades.
async fn status_moved(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
    lot: &LotRow,
    from: &str,
) -> Result<(), AppError> {
    let mut work = vec![(lot.clone(), from.to_owned(), None::<Uuid>)];
    while let Some((lot, from, because_of)) = work.pop() {
        let event = NewEvent {
            lot_id: lot.id,
            kind: "status_changed",
            from_status: Some(&from),
            to_status: Some(&lot.status),
            user_id,
            meta: because_of.map(|p| json!({ "because_of": p })),
        };
        repo::add_event(conn, workspace_id, &event).await?;
        let payload = json!({
            "lot_id": lot.id,
            "product_id": lot.product_id,
            "code": lot.code,
            "public_code": lot.public_code,
            "from": from,
            "to": lot.status,
        });
        events::emit(conn, workspace_id, LOT_STATUS_CHANGED, payload).await?;
        for t in repo::sent(&mut *conn, workspace_id, lot.id).await? {
            let payload = json!({
                "transfer_id": t.id,
                "to_lot_id": t.to_lot_id,
                "from_lot_code": lot.code,
                "status": lot.status,
            });
            events::emit(conn, t.to_workspace_id, SUPPLY_SOURCE_STATUS_CHANGED, payload).await?;
        }
        if rank(&lot.status) <= rank(&from) {
            continue;
        }
        reserve::move_off_lot(conn, workspace_id, lot.id, lot.product_id).await?;
        for mut child in repo::children_locked(conn, workspace_id, lot.id).await? {
            if rank(&child.status) >= rank(&lot.status) {
                continue;
            }
            let was = std::mem::replace(&mut child.status, lot.status.clone());
            repo::set_status(conn, workspace_id, child.id, &child.status).await?;
            work.push((child, was, Some(lot.id)));
        }
    }
    Ok(())
}

/// GET /lots/{lot}/holds: orders still holding this lot's stock, after the same 404 as `get`.
pub async fn holds(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<Vec<repo::HoldRow>, AppError> {
    let lot = get(state, workspace_id, id).await?;
    Ok(repo::holds(&state.db, workspace_id, lot.id).await?)
}

/// `rows` as resources, with where each came from (one statement for all).
pub async fn resources(
    state: &AppState,
    workspace_id: Uuid,
    rows: Vec<LotRow>,
) -> Result<Vec<LotResource>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|l| l.id).collect();
    let lineage = repo::lineage(&state.db, workspace_id, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|l| {
            let g = lineage.iter().find(|g| g.lot_id == l.id);
            LotResource::new(l, g)
        })
        .collect())
}

/// One lot as a resource, with where it came from.
pub async fn resource(
    state: &AppState,
    workspace_id: Uuid,
    row: LotRow,
) -> Result<LotResource, AppError> {
    let mut v = resources(state, workspace_id, vec![row]).await?;
    v.pop()
        .ok_or_else(|| AppError::Internal("lot resource vanished".into()))
}

/// GET /lots/{lot}/farmer-credits: a page of the farmer credits of this workspace's lot, the
/// total, and the farmer groups its public page shows.
pub async fn farmer_credits(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<repo::CreditRow>, i64, Vec<PublicFarmerGroup>), AppError> {
    let lot = get(state, workspace_id, id).await?;
    let (rows, total) =
        repo::farmer_credits(&state.db, workspace_id, lot.id, limit, offset).await?;
    let groups = farm_supply::service::public_farmers(&state.db, lot.id)
        .await?
        .into_iter()
        .map(|g| PublicFarmerGroup::new(g.village, g.farmers, g.harvest_start, g.harvest_end))
        .collect();
    Ok((rows, total, groups))
}

/// GET /lots/{lot}/events: one page of the trail, newest first.
pub async fn events(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EventRow>, i64), AppError> {
    let lot = get(state, workspace_id, id).await?;
    Ok(repo::events(&state.db, workspace_id, lot.id, limit, offset).await?)
}

/// Every referenced file must be a stored file of this workspace that suits its use
/// (422 on the field path otherwise), and the caller must be allowed to read files (403):
/// `product.update` alone must not publish files it can't see. One query.
async fn check_files(
    state: &AppState,
    access: &WorkspaceAccess,
    p: &Passport,
) -> Result<(), AppError> {
    let refs = p.files();
    if refs.is_empty() {
        return Ok(());
    }
    if !access.allows(FILE_GET) {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let workspace_id = access.workspace_id;
    let ids: Vec<Uuid> = refs.iter().map(|(_, id, _)| *id).collect();
    let found = files::find_many(&state.db, workspace_id, &ids).await?;
    let mut errors = BTreeMap::new();
    for (field, id, used) in refs {
        let ok = found
            .iter()
            .any(|f| f.id == id && used.allows(&f.content_type));
        if !ok {
            let msg = match used {
                FileUse::Image => "The file must be an image uploaded to this workspace.",
                FileUse::Document => "The file must be an image or PDF uploaded to this workspace.",
            };
            errors.insert(field, vec![msg.to_owned()]);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

/// GET /lots/{lot}/passport.
pub async fn passport(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
) -> Result<PassportRow, AppError> {
    repo::passport(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// PUT /lots/{lot}/passport: replace the draft; the published copy is untouched.
pub async fn save_draft(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    draft: Passport,
) -> Result<PassportRow, AppError> {
    let workspace_id = access.workspace_id;
    let id = parse(id)?;
    if repo::find(&state.db, workspace_id, id).await?.is_none() {
        return Err(AppError::NotFound);
    }
    check_files(state, access, &draft).await?;
    let value = serde_json::to_value(&draft).map_err(|e| AppError::Internal(e.to_string()))?;
    repo::set_draft(&state.db, workspace_id, id, &value)
        .await?
        .ok_or(AppError::NotFound)
}

/// POST /lots/{lot}/passport/publish: the draft (with a title, files still valid) becomes the
/// next published version; trail row and `passport.published` in the same transaction.
pub async fn publish(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
) -> Result<PassportRow, AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let lot = repo::locked(&mut tx, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let current = repo::passport(&mut *tx, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let draft = Passport::from_stored(current.draft)?;
    if draft.title.is_none() {
        return Err(AppError::field(
            "title",
            "The passport needs a title before it can be published.",
        ));
    }
    check_files(state, access, &draft).await?;
    let saved = repo::publish(&mut tx, access.workspace_id, id).await?;
    let event = NewEvent {
        lot_id: id,
        kind: "passport_published",
        from_status: None,
        to_status: None,
        user_id: access.user_id,
        meta: Some(json!({ "version": saved.version })),
    };
    repo::add_event(&mut tx, access.workspace_id, &event).await?;
    let payload = json!({
        "lot_id": id,
        "product_id": lot.product_id,
        "public_code": lot.public_code,
        "version": saved.version,
    });
    events::emit(&mut tx, access.workspace_id, PASSPORT_PUBLISHED, payload).await?;
    tx.commit().await?;
    Ok(saved)
}

/// DELETE /lots/{lot}/passport/publish: hide the published copy (no-op when hidden).
pub async fn unpublish(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
) -> Result<PassportRow, AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    repo::locked(&mut tx, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let current = repo::passport(&mut *tx, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if current.published.is_none() {
        return Ok(current);
    }
    let saved = repo::unpublish(&mut tx, access.workspace_id, id).await?;
    let event = NewEvent {
        lot_id: id,
        kind: "passport_unpublished",
        from_status: None,
        to_status: None,
        user_id: access.user_id,
        meta: Some(json!({ "version": saved.version })),
    };
    repo::add_event(&mut tx, access.workspace_id, &event).await?;
    tx.commit().await?;
    Ok(saved)
}

/// The public page of a published lot (`workspace_id`: the storefront key's workspace).
/// Unknown, unpublished and foreign codes are the same 404. Files deleted since publishing,
/// or a server without storage, give null links, not an error.
pub async fn public(
    state: &AppState,
    public_code: &str,
    workspace_id: Option<Uuid>,
) -> Result<PublicLot, AppError> {
    let row = repo::public(&state.db, public_code, workspace_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let passport = Passport::from_stored(row.passport.clone())?;
    let ids: Vec<Uuid> = passport.files().into_iter().map(|(_, id, _)| id).collect();
    let mut links = HashMap::new();
    if let Ok(store) = files_service::store(state) {
        for f in files::find_many(&state.db, row.workspace_id, &ids).await? {
            let (url, expires_at) = files_service::signed_link(store, &f)?;
            links.insert(f.id, Link::new(url, expires_at));
        }
    }
    let farmers = farm_supply::service::public_farmers(&state.db, row.id)
        .await?
        .into_iter()
        .map(|g| PublicFarmerGroup::new(g.village, g.farmers, g.harvest_start, g.harvest_end))
        .collect();
    Ok(PublicLot::new(row, passport, &links, farmers))
}

/// POST /lots/{lot}/stock-mutations: stock in or out of the lot at a warehouse (its
/// warehouse-level lot row). Out may not take more than the row has free (level − reserved).
pub async fn stock_mutation(
    state: &AppState,
    access: &WorkspaceAccess,
    id: &str,
    m: StockMutation,
    key: Option<idempotency::Key>,
) -> Result<idempotency::Answer, AppError> {
    let lot = get(state, access.workspace_id, id).await?;
    if !warehouses::exists(&state.db, access.workspace_id, m.warehouse_id).await? {
        return Err(AppError::field(
            "warehouse_id",
            "The selected warehouse id is invalid.",
        ));
    }
    let slot = Slot {
        product_id: lot.product_id,
        warehouse_id: m.warehouse_id,
        bin: None,
        lot_id: Some(lot.id),
    };
    let cause = Cause {
        reason: m.reason.as_deref(),
        is_correction: m.is_correction,
        user_id: Some(access.user_id),
        ..Cause::default()
    };
    let mut tx = state.db.begin().await?;
    let ws = access.workspace_id;
    let actor = idempotency::Actor::User(access.user_id);
    if let Some(a) = idempotency::replay(&mut tx, ws, actor, key.as_ref()).await? {
        return Ok(a);
    }
    // Under the lot lock, so a recall can't slip in between. Quarantine takes goods in for
    // inspection; a recalled lot only gives stock out (write-off).
    let lot = repo::locked(&mut tx, ws, lot.id)
        .await?
        .ok_or(AppError::NotFound)?;
    if lot.status == "recalled" && m.quantity > rust_decimal::Decimal::ZERO {
        return Err(AppError::Conflict("lot_recalled"));
    }
    match stock::mutate(
        &mut tx,
        access.workspace_id,
        slot,
        m.quantity,
        Floor::Available,
        &cause,
    )
    .await?
    {
        Mutated::Short { level, reserved } => Err(AppError::field(
            "quantity",
            format!(
                "Insufficient stock in this lot. Available: {}, Reserved: {}, Requested: {}",
                level.normalize(),
                reserved.normalize(),
                (-m.quantity).normalize()
            ),
        )),
        Mutated::OutOfRange => Err(AppError::field(
            "quantity",
            "The stock level would be out of range.",
        )),
        Mutated::Done { after, .. } => {
            let level = LotStockLevel {
                warehouse_id: m.warehouse_id,
                stock_lvl: format!("{after:.2}"),
            };
            let answer = idempotency::Answer {
                status: 200,
                body: json!({ "data": level }).to_string().into_bytes(),
            };
            idempotency::finish(&mut tx, ws, key.as_ref(), &answer).await?;
            tx.commit().await?;
            Ok(answer)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_codes_are_ten_crockford_chars_and_vary() {
        let a = public_code().unwrap();
        let b = public_code().unwrap();
        assert_eq!(a.len(), 10);
        assert!(a.bytes().all(|c| CROCKFORD.contains(&c)), "{a}");
        assert_ne!(a, b);
    }

    #[test]
    fn later_dates_must_not_precede_production() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
        assert!(check_dates(d("2026-07-01"), d("2026-07-01"), None).is_ok());
        assert!(check_dates(None, d("2020-01-01"), d("2020-01-01")).is_ok());
        let Err(AppError::Validation(e)) =
            check_dates(d("2026-07-01"), d("2026-06-30"), d("2026-06-01"))
        else {
            panic!("expected 422");
        };
        assert_eq!(e.keys().collect::<Vec<_>>(), ["best_before", "expires_on"]);
    }
}
