//! PUE rules. A farmer requests for themselves; staff request for a person of their villages.
//! Provenance and decision fields are the server's. Ops and admin review. Village energy and
//! pipeline are staff reads of their own villages.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::{
        farm_catalogue::repo::find_equipment,
        farm_pue::{
            dto::{NewPreview, NewRequest},
            repo::{self, EnergyRow, NewRow, Pick, PipelineRow, PreviewRow, RequestRow, Seen},
            rules::{self, CREATABLE, CREATE_REFUSED, REVIEW_ONLY, Refusal},
        },
        farm_tower::service::staff_village,
    },
};

const ID_TAKEN: &str = "This id is already in use.";

fn invalid(field: &str) -> AppError {
    let f = field.replace('_', " ");
    AppError::field(field, format!("The selected {f} is invalid."))
}

/// One page of requests: a farmer's own, plus every request of the caller's staff villages.
pub async fn list(
    state: &AppState,
    access: &FarmAccess,
    pick: &Pick,
) -> Result<(Vec<RequestRow>, i64), AppError> {
    let scope = access.staff_scope();
    let seen = Seen {
        ws: access.workspace_id,
        all: scope.all,
        villages: &scope.villages,
        person: access.person_id,
    };
    Ok(repo::requests(&state.db, &seen, pick).await?)
}

async fn find(
    state: &AppState,
    access: &FarmAccess,
    id: Uuid,
) -> Result<Option<RequestRow>, AppError> {
    let pick = Pick {
        id: Some(id),
        limit: 1,
        ..Pick::default()
    };
    Ok(list(state, access, &pick).await?.0.pop())
}

/// One request the caller may see.
pub async fn get(state: &AppState, access: &FarmAccess, id: &str) -> Result<RequestRow, AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    find(state, access, id).await?.ok_or(AppError::NotFound)
}

/// True when a stored request holds what `r` asks for.
fn same(row: &RequestRow, r: &NewRequest, person: Uuid) -> bool {
    (
        row.person["id"].as_str() == Some(person.to_string().as_str()),
        row.equipment_id,
        row.farm_id,
        row.acquisition.as_str(),
        row.quantity,
        row.hours_per_day,
        row.days_per_week,
        row.purpose.as_deref(),
    ) == (
        true,
        r.equipment_id,
        r.farm_id,
        r.acquisition.as_str(),
        r.quantity,
        r.hours_per_day,
        r.days_per_week,
        r.purpose.as_deref(),
    )
}

/// The person and village a request is filed for. Staff naming a person file for them (it
/// must be in their villages); everyone else files for their own person.
async fn applicant(
    state: &AppState,
    access: &FarmAccess,
    named: Option<Uuid>,
) -> Result<(Uuid, Uuid), AppError> {
    let ws = access.workspace_id;
    let person = match (named, access.person_id) {
        (Some(p), _) if access.is_staff() => {
            let village = repo::person_village(&state.db, ws, p).await?;
            return match village {
                Some(v) if access.staff_in(v) => Ok((p, v)),
                _ => Err(invalid("person_id")),
            };
        }
        (_, Some(own)) => own,
        (_, None) if access.is_staff() => {
            return Err(AppError::field(
                "person_id",
                "The person id field is required.",
            ));
        }
        (_, None) => return Err(AppError::Forbidden(NOT_ALLOWED)),
    };
    let village = repo::person_village(&state.db, ws, person)
        .await?
        .ok_or(AppError::Forbidden(NOT_ALLOWED))?;
    Ok((person, village))
}

/// File a request. False = replayed.
pub async fn create(
    state: &AppState,
    access: &FarmAccess,
    r: NewRequest,
) -> Result<(RequestRow, bool), AppError> {
    if !CREATABLE.contains(&r.status.as_str()) {
        return Err(AppError::Unprocessable(CREATE_REFUSED.into()));
    }
    let ws = access.workspace_id;
    let (person, village) = applicant(state, access, r.person_id).await?;
    if let Some(id) = r.id
        && let Some(row) = find(state, access, id).await?
    {
        return if same(&row, &r, person) {
            Ok((row, false))
        } else {
            Err(AppError::Conflict(ID_TAKEN))
        };
    }
    if let Some(farm) = r.farm_id
        && !repo::farm_in(&state.db, ws, village, farm).await?
    {
        return Err(invalid("farm_id"));
    }
    let machine = find_equipment(&state.db, ws, r.equipment_id)
        .await?
        .ok_or_else(|| invalid("equipment_id"))?;
    // RUAHA pue_request_acquisition_check: ask only for what the catalogue offers.
    match r.acquisition.as_str() {
        "rent" if !machine.can_rent => {
            return Err(AppError::Unprocessable(
                "this equipment is not offered for rent".into(),
            ));
        }
        "buy" if !machine.can_buy => {
            return Err(AppError::Unprocessable(
                "this equipment is not offered for sale".into(),
            ));
        }
        _ => {}
    }
    let mut tx = access.begin(&state.db).await?;
    let new = NewRow {
        id: r.id,
        village_id: village,
        person_id: person,
        farm_id: r.farm_id,
        equipment_id: r.equipment_id,
        acquisition: &r.acquisition,
        quantity: r.quantity,
        hours_per_day: r.hours_per_day,
        days_per_week: r.days_per_week,
        purpose: r.purpose.as_deref(),
        status: &r.status,
        captured_by: access.user_id,
    };
    let id = repo::insert(&mut tx, ws, &new).await.map_err(|e| {
        if e.as_database_error().and_then(|d| d.constraint()) == Some("farm_pue_requests_pkey") {
            AppError::Conflict(ID_TAKEN)
        } else {
            AppError::Database(e)
        }
    })?;
    tx.commit().await?;
    let row = find(state, access, id).await?.ok_or(AppError::NotFound)?;
    Ok((row, true))
}

/// Move a request (`rules::ACTIONS`). The applicant and village staff submit and withdraw;
/// review states need ops or admin.
pub async fn transition(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    to: &str,
    note: Option<&str>,
) -> Result<RequestRow, AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let mut tx = access.begin(&state.db).await?;
    let (village, person, from) = repo::lock(&mut tx, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if access.person_id != Some(person) && !access.staff_in(village) {
        return Err(AppError::NotFound);
    }
    match rules::check(&from, to, access.manages()) {
        Err(Refusal::Illegal) => {
            return Err(AppError::Unprocessable(format!(
                "illegal transition {from} -> {to}"
            )));
        }
        Err(Refusal::NotReviewer) => return Err(AppError::Forbidden(REVIEW_ONLY)),
        Ok(()) => {}
    }
    repo::move_to(&mut tx, access.workspace_id, id, to, access.user_id, note).await?;
    tx.commit().await?;
    find(state, access, id).await?.ok_or(AppError::NotFound)
}

/// Prospective and approved load against the village's current capacity.
pub async fn energy(
    state: &AppState,
    access: &FarmAccess,
    village: &str,
) -> Result<(Uuid, EnergyRow), AppError> {
    let village = staff_village(state, access, village).await?;
    Ok((
        village,
        repo::energy(&state.db, access.workspace_id, village).await?,
    ))
}

/// Requests per status with indicative value, per currency.
pub async fn pipeline(
    state: &AppState,
    access: &FarmAccess,
    village: &str,
) -> Result<Vec<PipelineRow>, AppError> {
    let village = staff_village(state, access, village).await?;
    Ok(repo::pipeline(&state.db, access.workspace_id, village).await?)
}

/// The estimate a request with these inputs would get, without writing anything. Any farm
/// member may ask; unknown machines are a 422.
pub async fn preview(
    state: &AppState,
    access: &FarmAccess,
    p: NewPreview,
) -> Result<Option<PreviewRow>, AppError> {
    let ws = access.workspace_id;
    if find_equipment(&state.db, ws, p.equipment_id)
        .await?
        .is_none()
    {
        return Err(invalid("equipment_id"));
    }
    Ok(repo::preview(
        &state.db,
        ws,
        p.equipment_id,
        p.quantity,
        p.hours_per_day,
        p.days_per_week,
    )
    .await?)
}
