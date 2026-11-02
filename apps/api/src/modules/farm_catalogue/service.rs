//! Catalogue rules: every farm member reads; ops and admin add. A create with a client id
//! replays the stored row when the body matches it and is refused (409) when it differs.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::{
        farm_catalogue::repo::{
            self, CategoryRow, EquipmentRow, LoanRow, NewCategory, NewEquipment, NewLoan,
        },
        farm_surveys::repo::currency_exists,
    },
};

const CODE_TAKEN: &str = "The code has already been taken.";
const ID_TAKEN: &str = "This id is already in use.";

fn require_manager(access: &FarmAccess) -> Result<(), AppError> {
    if access.manages() {
        Ok(())
    } else {
        Err(AppError::Forbidden(NOT_ALLOWED))
    }
}

/// An insert error: the primary key (id used elsewhere) is 409, `code_key` a 422 on `code`.
fn insert_error(e: sqlx::Error, pkey: &str, code_key: &str) -> AppError {
    if e.as_database_error().and_then(|d| d.constraint()) == Some(pkey) {
        AppError::Conflict(ID_TAKEN)
    } else {
        AppError::unique(e, code_key, "code", CODE_TAKEN)
    }
}

/// One page of categories.
pub async fn list_categories(
    state: &AppState,
    access: &FarmAccess,
    active: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<CategoryRow>, i64), AppError> {
    let ws = access.workspace_id;
    Ok(repo::list_categories(&state.db, ws, active, limit, offset).await?)
}

/// Add a category. False = replayed.
pub async fn create_category(
    state: &AppState,
    access: &FarmAccess,
    c: NewCategory,
) -> Result<(CategoryRow, bool), AppError> {
    require_manager(access)?;
    let ws = access.workspace_id;
    if let Some(id) = c.id
        && let Some(row) = repo::find_category(&state.db, ws, id).await?
    {
        let same = (&row.code, &row.name_en, &row.name_sw) == (&c.code, &c.name_en, &c.name_sw);
        return if same {
            Ok((row, false))
        } else {
            Err(AppError::Conflict(ID_TAKEN))
        };
    }
    let mut tx = access.begin(&state.db).await?;
    let row = repo::insert_category(&mut *tx, ws, &c).await.map_err(|e| {
        insert_error(
            e,
            "farm_equipment_categories_pkey",
            "farm_equipment_categories_code_key",
        )
    })?;
    tx.commit().await?;
    Ok((row, true))
}

/// One page of machines.
pub async fn list_equipment(
    state: &AppState,
    access: &FarmAccess,
    active: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EquipmentRow>, i64), AppError> {
    let ws = access.workspace_id;
    Ok(repo::list_equipment(&state.db, ws, active, limit, offset).await?)
}

/// One machine of this workspace.
pub async fn get_equipment(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
) -> Result<EquipmentRow, AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    repo::find_equipment(&state.db, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)
}

/// True when a stored machine holds exactly what `e` asks for.
fn same(row: &EquipmentRow, e: &NewEquipment) -> bool {
    (
        row.category_id,
        &row.code,
        &row.name_en,
        &row.name_sw,
        row.rated_power_kw,
        row.typical_hours_per_day,
        row.typical_days_per_week,
    ) == (
        e.category_id,
        &e.code,
        &e.name_en,
        &e.name_sw,
        e.rated_power_kw,
        e.typical_hours_per_day,
        e.typical_days_per_week,
    ) && (
        row.can_rent,
        row.can_buy,
        row.indicative_price_minor,
        row.indicative_rent_per_day_minor,
        row.currency_id,
    ) == (
        e.can_rent,
        e.can_buy,
        e.indicative_price_minor,
        e.indicative_rent_per_day_minor,
        e.currency_id,
    )
}

/// Add a machine. False = replayed.
pub async fn create_equipment(
    state: &AppState,
    access: &FarmAccess,
    e: NewEquipment,
) -> Result<(EquipmentRow, bool), AppError> {
    require_manager(access)?;
    let ws = access.workspace_id;
    if let Some(id) = e.id
        && let Some(row) = repo::find_equipment(&state.db, ws, id).await?
    {
        return if same(&row, &e) {
            Ok((row, false))
        } else {
            Err(AppError::Conflict(ID_TAKEN))
        };
    }
    if repo::find_category(&state.db, ws, e.category_id)
        .await?
        .is_none()
    {
        return Err(AppError::field(
            "category_id",
            "no such category in this workspace",
        ));
    }
    if !currency_exists(&state.db, ws, e.currency_id).await? {
        return Err(AppError::field(
            "currency_id",
            "no such currency in this workspace",
        ));
    }
    let mut tx = access.begin(&state.db).await?;
    let id = repo::insert_equipment(&mut *tx, ws, &e)
        .await
        .map_err(|err| insert_error(err, "farm_equipment_pkey", "farm_equipment_code_key"))?;
    let row = repo::find_equipment(&mut *tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok((row, true))
}

/// One page of loan listings: every farm member reads them.
pub async fn list_loans(
    state: &AppState,
    access: &FarmAccess,
    active: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<LoanRow>, i64), AppError> {
    Ok(repo::loans(&state.db, access.workspace_id, None, active, limit, offset).await?)
}

/// Add a loan listing (ops, admin). False = replayed.
pub async fn create_loan(
    state: &AppState,
    access: &FarmAccess,
    l: NewLoan,
) -> Result<(LoanRow, bool), AppError> {
    require_manager(access)?;
    let ws = access.workspace_id;
    let find = |id| async move {
        Ok::<_, AppError>(
            repo::loans(&state.db, ws, Some(id), None, 1, 0)
                .await?
                .0
                .pop(),
        )
    };
    if let Some(id) = l.id
        && let Some(row) = find(id).await?
    {
        let same = (
            &row.code,
            &row.name_en,
            &row.name_sw,
            &row.description_en,
            &row.description_sw,
            row.indicative_min_minor,
            row.indicative_max_minor,
            row.currency_id,
        ) == (
            &l.code,
            &l.name_en,
            &l.name_sw,
            &l.description_en,
            &l.description_sw,
            l.indicative_min_minor,
            l.indicative_max_minor,
            l.currency_id,
        );
        return if same {
            Ok((row, false))
        } else {
            Err(AppError::Conflict(ID_TAKEN))
        };
    }
    if !currency_exists(&state.db, ws, l.currency_id).await? {
        return Err(AppError::field(
            "currency_id",
            "The selected currency id is invalid.",
        ));
    }
    let mut tx = access.begin(&state.db).await?;
    let id = repo::insert_loan(&mut *tx, ws, &l)
        .await
        .map_err(|e| insert_error(e, "farm_loan_products_pkey", "farm_loan_products_code_key"))?;
    tx.commit().await?;
    let row = find(id).await?.ok_or(AppError::NotFound)?;
    Ok((row, true))
}
