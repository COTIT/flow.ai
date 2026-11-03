//! Cash rules: one open shift per register; expected cash = opening + sales ± transfers;
//! a transfer needs an open shift, a vault of the register's currency and enough cash at the
//! source. Drawer writes lock the register row, then the vault row.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::WorkspaceAccess,
    modules::{
        cash::{
            dto::{Count, NewVault, Transfer, TransferResource},
            repo::{self, Entry, ShiftFilter, ShiftRow, VaultRow},
        },
        currencies, products,
        till::repo::SessionRow,
    },
};

const ALREADY_OPEN: &str = "A shift is already open on this register.";
const NOT_OPEN: &str = "No shift is open on this register.";
const BAD_VAULT: &str = "The selected vault id is invalid.";
const OTHER_CURRENCY: &str = "The vault holds another currency than this register.";
const BAD_CURRENCY: &str = "The selected currency id is invalid.";
const NO_CURRENCY: &str = "The workspace has no main currency.";

fn drawer(s: &SessionRow, shift_id: Uuid, kind: &'static str, transfer_id: Option<Uuid>) -> Entry {
    Entry {
        register_id: Some(s.register_id),
        shift_id: Some(shift_id),
        vault_id: None,
        kind,
        transfer_id,
        till_user_id: s.till_user_id,
    }
}

/// The register's open shift, if any.
pub async fn current(state: &AppState, s: &SessionRow) -> Result<Option<ShiftRow>, AppError> {
    Ok(repo::open_shift(&state.db, s.workspace_id, s.register_id).await?)
}

/// Open a shift with the counted float; 409 when one is open.
pub async fn open(state: &AppState, s: &SessionRow, count: Count) -> Result<ShiftRow, AppError> {
    let ws = s.workspace_id;
    let mut tx = state.db.begin().await?;
    repo::lock_register(&mut tx, ws, s.register_id).await?;
    if repo::open_shift(&mut *tx, ws, s.register_id)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(ALREADY_OPEN));
    }
    let id = repo::insert_shift(&mut *tx, ws, s.register_id, s.till_user_id, count.total).await?;
    repo::book(
        &mut *tx,
        ws,
        &drawer(s, id, "open_count", None),
        &count.lines,
        1,
    )
    .await?;
    let row = repo::shift(&mut *tx, ws, id).await?;
    tx.commit().await?;
    Ok(row)
}

/// Close the open shift with the counted cash: stores expected, counted and the difference.
pub async fn close(state: &AppState, s: &SessionRow, count: Count) -> Result<ShiftRow, AppError> {
    let ws = s.workspace_id;
    let mut tx = state.db.begin().await?;
    repo::lock_register(&mut tx, ws, s.register_id).await?;
    let shift = repo::open_shift(&mut *tx, ws, s.register_id)
        .await?
        .ok_or(AppError::Conflict(NOT_OPEN))?;
    repo::book(
        &mut *tx,
        ws,
        &drawer(s, shift.id, "close_count", None),
        &count.lines,
        1,
    )
    .await?;
    repo::close_shift(
        &mut *tx,
        ws,
        shift.id,
        s.till_user_id,
        shift.expected,
        count.total,
    )
    .await?;
    let row = repo::shift(&mut *tx, ws, shift.id).await?;
    tx.commit().await?;
    Ok(row)
}

/// Move counted cash between the drawer and a vault, both sides in one transaction.
pub async fn transfer(
    state: &AppState,
    s: &SessionRow,
    input: Transfer,
) -> Result<TransferResource, AppError> {
    let ws = s.workspace_id;
    let amount = input.count.total;
    let mut tx = state.db.begin().await?;
    repo::lock_register(&mut tx, ws, s.register_id).await?;
    let shift = repo::open_shift(&mut *tx, ws, s.register_id)
        .await?
        .ok_or(AppError::Conflict(NOT_OPEN))?;
    let (currency_id, vault_balance) = repo::lock_vault(&mut tx, ws, input.vault_id)
        .await?
        .ok_or_else(|| AppError::field("vault_id", BAD_VAULT))?;
    if currency_id != s.currency_id {
        return Err(AppError::field("vault_id", OTHER_CURRENCY));
    }
    let (source, held) = if input.to_vault {
        ("drawer", shift.expected)
    } else {
        ("vault", vault_balance)
    };
    if held < amount {
        return Err(AppError::field(
            "count",
            format!("The {source} holds only {held} (minor units)."),
        ));
    }
    let transfer_id = Uuid::now_v7();
    let out = if input.to_vault { -1 } else { 1 };
    let lines = &input.count.lines;
    repo::book(
        &mut *tx,
        ws,
        &drawer(s, shift.id, "transfer", Some(transfer_id)),
        lines,
        out,
    )
    .await?;
    let vault = Entry {
        register_id: None,
        shift_id: None,
        vault_id: Some(input.vault_id),
        kind: "transfer",
        transfer_id: Some(transfer_id),
        till_user_id: s.till_user_id,
    };
    repo::book(&mut *tx, ws, &vault, lines, -out).await?;
    tx.commit().await?;
    Ok(TransferResource {
        transfer_id,
        vault_id: input.vault_id,
        direction: if input.to_vault {
            "to_vault"
        } else {
            "from_vault"
        },
        amount,
    })
}

/// New vault in the sent or the main currency.
pub async fn create_vault(
    state: &AppState,
    access: &WorkspaceAccess,
    input: NewVault,
) -> Result<VaultRow, AppError> {
    let ws = access.workspace_id;
    let currency_id = match input.currency_id {
        Some(id) => currencies::repo::find(&state.db, ws, id)
            .await?
            .map(|_| id)
            .ok_or_else(|| AppError::field("currency_id", BAD_CURRENCY))?,
        None => products::repo::main_currency(&state.db, ws)
            .await?
            .ok_or_else(|| AppError::field("currency_id", NO_CURRENCY))?,
    };
    let (id, created_at) =
        repo::insert_vault(&state.db, ws, &input.name, currency_id, access.user_id).await?;
    Ok(VaultRow {
        id,
        name: input.name,
        currency_id,
        balance: 0,
        created_at,
    })
}

/// Every vault with its balance.
pub async fn list_vaults(state: &AppState, workspace_id: Uuid) -> Result<Vec<VaultRow>, AppError> {
    Ok(repo::list_vaults(&state.db, workspace_id).await?)
}

/// One page of shifts and the total.
pub async fn list_shifts(
    state: &AppState,
    workspace_id: Uuid,
    f: &ShiftFilter,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ShiftRow>, i64), AppError> {
    Ok(repo::list_shifts(&state.db, workspace_id, f, limit, offset).await?)
}

/// Cash due for `total` (minor units) in steps of `step` minor units, half up: 1.02 → 1.00,
/// 1.03 → 1.05 at step 5. Step 1 = no rounding.
pub fn round_cash(total: i64, step: i64) -> i64 {
    // i128: `total * 2` can't overflow; the result is never above `total + step`.
    let (total, step) = (i128::from(total), i128::from(step.max(1)));
    i64::try_from((total * 2 + step) / (2 * step) * step).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cash_rounds_half_up_to_the_step() {
        for (total, step, due) in [
            (1002, 5, 1000),
            (1003, 5, 1005),
            (1005, 5, 1005),
            (1007, 5, 1005),
            (1008, 5, 1010),
            (1005, 10, 1010),
            (1004, 10, 1000),
            (12_345, 1, 12_345),
            (0, 5, 0),
            (2, 5, 0),
            (3, 5, 5),
        ] {
            assert_eq!(round_cash(total, step), due, "{total} step {step}");
        }
    }
}
