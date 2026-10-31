//! Lot rules: the product must be a live stock-tracked product of the workspace, codes are
//! unique per workspace (any case), the public code is made here and never changes, dates
//! keep their order, and every change writes its trail row in the same transaction.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde_json::json;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    modules::{
        lots::{
            dto::{LotPatch, NewLotInput, STATUSES},
            repo::{self, EventRow, ListFilter, LotRow, NewEvent},
        },
        products::repo as products,
        stock::service::tracks_stock,
        webhooks::events::{self, LOT_CREATED},
    },
};

const NO_PRODUCT: &str = "The selected product id is invalid.";
const TAKEN: &str = "The code has already been taken.";
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
    let mut lot = LotRow {
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
    let mut inserted = None;
    for _ in 0..PUBLIC_CODE_TRIES {
        lot.public_code = public_code()?;
        inserted = repo::insert(&mut tx, access.workspace_id, &lot, access.user_id)
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
        user_id: access.user_id,
        meta: None,
    };
    repo::add_event(&mut tx, access.workspace_id, &event).await?;
    let payload = json!({
        "lot_id": lot.id,
        "product_id": lot.product_id,
        "code": lot.code,
        "public_code": lot.public_code,
    });
    events::emit(&mut tx, access.workspace_id, LOT_CREATED, payload).await?;
    tx.commit().await?;
    Ok(lot)
}

/// One lot of the workspace, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<LotRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
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
        let event = NewEvent {
            lot_id: id,
            kind: "status_changed",
            from_status: Some(&before.status),
            to_status: Some(&after.status),
            user_id: access.user_id,
            meta: None,
        };
        repo::add_event(&mut tx, access.workspace_id, &event).await?;
    }
    tx.commit().await?;
    Ok(saved)
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
