//! Market rules. Buyers and demand are an ops surface: ops and admin read and write, everyone
//! else 403 (RUAHA mvp_security). Opportunities are read by staff of their village and written
//! by ops and admin only; status follows `rules::legal`.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::{
        farm_market::{
            dto::{NewLine, NewOpportunity},
            repo::{
                self, AvailableRow, BuyerRow, DemandRow, LineRow, MatchRow, NewBuyer, NewDemand,
                NewLineRow, OppPick, OppRow, SupplyRow,
            },
            rules::{
                ATTACH_ONLY, BAD_HARVEST, CROP_MISMATCH, MISMATCH, MUST_START, NOT_LIVE,
                WRITE_ONLY, legal, live,
            },
        },
        farm_surveys::repo::currency_exists,
    },
};

const ID_TAKEN: &str = "This id is already in use.";

fn require_manager(access: &FarmAccess) -> Result<(), AppError> {
    if access.manages() {
        Ok(())
    } else {
        Err(AppError::Forbidden(NOT_ALLOWED))
    }
}

fn invalid(field: &str) -> AppError {
    let f = field.replace('_', " ");
    AppError::field(field, format!("The selected {f} is invalid."))
}

fn pkey(e: &sqlx::Error, constraint: &str) -> bool {
    e.as_database_error().and_then(|d| d.constraint()) == Some(constraint)
}

// ---- buyers ----

pub async fn buyers(
    state: &AppState,
    access: &FarmAccess,
    active: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<BuyerRow>, i64), AppError> {
    require_manager(access)?;
    Ok(repo::buyers(&state.db, access.workspace_id, active, limit, offset).await?)
}

/// Add a buyer. False = replayed.
pub async fn create_buyer(
    state: &AppState,
    access: &FarmAccess,
    b: NewBuyer,
) -> Result<(BuyerRow, bool), AppError> {
    require_manager(access)?;
    let ws = access.workspace_id;
    if let Some(id) = b.id
        && let Some(row) = repo::find_buyer(&state.db, ws, id).await?
    {
        let same =
            (&row.name, &row.channel, &row.contact_note) == (&b.name, &b.channel, &b.contact_note);
        return if same {
            Ok((row, false))
        } else {
            Err(AppError::Conflict(ID_TAKEN))
        };
    }
    let mut tx = access.begin(&state.db).await?;
    let row = repo::insert_buyer(&mut *tx, ws, &b).await.map_err(|e| {
        if pkey(&e, "farm_buyers_pkey") {
            AppError::Conflict(ID_TAKEN)
        } else {
            AppError::unique(
                e,
                "farm_buyers_name_key",
                "name",
                "The name has already been taken.",
            )
        }
    })?;
    tx.commit().await?;
    Ok((row, true))
}

// ---- demand ----

pub async fn demands(
    state: &AppState,
    access: &FarmAccess,
    statuses: Option<Vec<String>>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<DemandRow>, i64), AppError> {
    require_manager(access)?;
    let ws = access.workspace_id;
    Ok(repo::demands(&state.db, ws, None, statuses.as_deref(), limit, offset).await?)
}

async fn find_demand(state: &AppState, ws: Uuid, id: Uuid) -> Result<Option<DemandRow>, AppError> {
    Ok(repo::demands(&state.db, ws, Some(id), None, 1, 0)
        .await?
        .0
        .pop())
}

pub async fn demand(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
) -> Result<DemandRow, AppError> {
    require_manager(access)?;
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    find_demand(state, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)
}

/// True when a stored demand holds what `d` asks for.
fn same_demand(row: &DemandRow, d: &NewDemand) -> bool {
    (
        row.buyer_id,
        row.crop_id,
        row.quantity_kg,
        &row.quality_note,
        row.window_start,
        row.window_end,
        &row.delivery_point,
        row.indicative_price_per_kg_minor,
        row.currency_id,
        &row.confidence,
    ) == (
        d.buyer_id,
        d.crop_id,
        d.quantity_kg,
        &d.quality_note,
        d.window_start,
        d.window_end,
        &d.delivery_point,
        d.indicative_price_per_kg_minor,
        d.currency_id,
        &d.confidence,
    )
}

/// Record a demand: open, unverified, captured by the caller. False = replayed.
pub async fn create_demand(
    state: &AppState,
    access: &FarmAccess,
    d: NewDemand,
) -> Result<(DemandRow, bool), AppError> {
    require_manager(access)?;
    let ws = access.workspace_id;
    if let Some(id) = d.id
        && let Some(row) = find_demand(state, ws, id).await?
    {
        return if same_demand(&row, &d) {
            Ok((row, false))
        } else {
            Err(AppError::Conflict(ID_TAKEN))
        };
    }
    if repo::find_buyer(&state.db, ws, d.buyer_id).await?.is_none() {
        return Err(invalid("buyer_id"));
    }
    if !repo::crop_exists(&state.db, ws, d.crop_id).await? {
        return Err(invalid("crop_id"));
    }
    if let Some(c) = d.currency_id
        && !currency_exists(&state.db, ws, c).await?
    {
        return Err(invalid("currency_id"));
    }
    let mut tx = access.begin(&state.db).await?;
    let id = repo::insert_demand(&mut *tx, ws, &d, access.user_id)
        .await
        .map_err(|e| {
            if pkey(&e, "farm_buyer_demands_pkey") {
                AppError::Conflict(ID_TAKEN)
            } else {
                AppError::Database(e)
            }
        })?;
    tx.commit().await?;
    let row = find_demand(state, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((row, true))
}

// ---- opportunities ----

/// An opportunity with the supply lines the caller may read.
pub type Opportunity = (OppRow, Vec<LineRow>);

/// Opportunities the caller may see: staff of their villages (ops and admin: all), and a
/// farmer's opportunities carrying their own supply.
pub async fn opportunities(
    state: &AppState,
    access: &FarmAccess,
    filter: OppFilter,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Opportunity>, i64), AppError> {
    let scope = access.staff_scope();
    let ws = access.workspace_id;
    let pick = OppPick {
        ws,
        all: scope.all,
        villages: &scope.villages,
        person: access.person_id,
        id: filter.id,
        village: filter.village,
        demand: filter.demand,
        statuses: filter.statuses,
        limit,
        offset,
    };
    let (rows, total) = repo::opportunities(&state.db, &pick).await?;
    let ids: Vec<Uuid> = rows.iter().map(|o| o.id).collect();
    let mut lines = repo::lines(
        &state.db,
        ws,
        scope.all,
        &scope.villages,
        access.person_id,
        &ids,
    )
    .await?;
    let out = rows
        .into_iter()
        .map(|o| {
            let (mine, rest) = lines.drain(..).partition(|l| l.opportunity_id == o.id);
            lines = rest;
            (o, mine)
        })
        .collect();
    Ok((out, total))
}

/// List filters.
#[derive(Default)]
pub struct OppFilter {
    pub id: Option<Uuid>,
    pub village: Option<Uuid>,
    pub demand: Option<Uuid>,
    pub statuses: Option<Vec<String>>,
}

async fn find(
    state: &AppState,
    access: &FarmAccess,
    id: Uuid,
) -> Result<Option<Opportunity>, AppError> {
    let filter = OppFilter {
        id: Some(id),
        ..OppFilter::default()
    };
    Ok(opportunities(state, access, filter, 1, 0).await?.0.pop())
}

pub async fn opportunity(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
) -> Result<Opportunity, AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    find(state, access, id).await?.ok_or(AppError::NotFound)
}

/// Propose `demand` to a village. Born proposed, with the demand's crop. False = replayed.
pub async fn propose(
    state: &AppState,
    access: &FarmAccess,
    demand: &str,
    o: NewOpportunity,
) -> Result<(Opportunity, bool), AppError> {
    if !access.manages() {
        return Err(AppError::Forbidden(WRITE_ONLY));
    }
    let ws = access.workspace_id;
    let demand = Uuid::parse_str(demand).map_err(|_| AppError::NotFound)?;
    let d = find_demand(state, ws, demand)
        .await?
        .ok_or(AppError::NotFound)?;
    if o.status.as_deref().is_some_and(|s| s != "proposed") {
        return Err(unprocessable(MUST_START));
    }
    if o.crop_id.is_some_and(|c| c != d.crop_id) {
        return Err(unprocessable(CROP_MISMATCH));
    }
    if let Some(id) = o.id
        && let Some((at_demand, village, note)) = repo::opportunity_of(&state.db, ws, id).await?
    {
        return if (at_demand, village, note) == (demand, o.village_id, o.note.clone()) {
            let row = find(state, access, id).await?.ok_or(AppError::NotFound)?;
            Ok((row, false))
        } else {
            Err(AppError::Conflict(ID_TAKEN))
        };
    }
    if !repo::village_exists(&state.db, ws, o.village_id).await? {
        return Err(invalid("village_id"));
    }
    let mut tx = access.begin(&state.db).await?;
    let id = repo::insert_opportunity(
        &mut *tx,
        ws,
        o.id,
        demand,
        o.village_id,
        d.crop_id,
        o.note.as_deref(),
        access.user_id,
    )
    .await
    .map_err(|e| {
        if pkey(&e, "farm_opportunities_pkey") {
            AppError::Conflict(ID_TAKEN)
        } else {
            AppError::unique(
                e,
                "farm_opportunities_village_key",
                "village_id",
                "This village already has an opportunity for this demand.",
            )
        }
    })?;
    tx.commit().await?;
    let row = find(state, access, id).await?.ok_or(AppError::NotFound)?;
    Ok((row, true))
}

/// The opportunity id if the caller may see it (404 otherwise) and manage it (`refusal`, 403).
async fn writable(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    refusal: &'static str,
) -> Result<Uuid, AppError> {
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    if find(state, access, id).await?.is_none() {
        return Err(AppError::NotFound);
    }
    if !access.manages() {
        return Err(AppError::Forbidden(refusal));
    }
    Ok(id)
}

/// Move an opportunity. Those who see it but are not ops or admin get RUAHA's 403.
pub async fn set_status(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    to: &str,
) -> Result<Opportunity, AppError> {
    let id = writable(state, access, id, WRITE_ONLY).await?;
    let ws = access.workspace_id;
    let mut tx = access.begin(&state.db).await?;
    let (_, _, from) = repo::lock_opportunity(&mut tx, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !legal(&from, to) {
        return Err(AppError::Unprocessable(format!(
            "illegal opportunity transition {from} -> {to}"
        )));
    }
    repo::set_status(&mut tx, ws, id, to).await?;
    tx.commit().await?;
    find(state, access, id).await?.ok_or(AppError::NotFound)
}

fn unprocessable(message: &str) -> AppError {
    AppError::Unprocessable(message.into())
}

/// Attach a harvest to an opportunity (RUAHA opportunity_supply_guard v2). Lock order: the
/// harvest report row, then the opportunity, then the committed figure in its own statement,
/// so two attaches on one harvest never both pass. False = replayed.
pub async fn attach(
    state: &AppState,
    access: &FarmAccess,
    opportunity: &str,
    line: NewLine,
) -> Result<(Opportunity, bool), AppError> {
    let opp = writable(state, access, opportunity, ATTACH_ONLY).await?;
    let ws = access.workspace_id;
    if let Some(id) = line.id
        && let Some(stored) = repo::find_line(&state.db, ws, id).await?
    {
        return if stored == (opp, line.harvest_report_id, line.contributed_kg) {
            Ok((
                find(state, access, opp).await?.ok_or(AppError::NotFound)?,
                false,
            ))
        } else {
            Err(AppError::Conflict(ID_TAKEN))
        };
    }
    let mut tx = access.begin(&state.db).await?;
    let harvest = repo::lock_harvest(&mut tx, ws, line.harvest_report_id).await?;
    let (village, crop, status) = repo::lock_opportunity(&mut tx, ws, opp)
        .await?
        .ok_or(AppError::NotFound)?;
    let h = harvest
        .filter(|h| !h.deleted && h.is_current && h.kind == "expected")
        .ok_or_else(|| unprocessable(BAD_HARVEST))?;
    if !live(&status) {
        return Err(unprocessable(NOT_LIVE));
    }
    if h.village_id != village || h.crop_id != crop {
        return Err(unprocessable(MISMATCH));
    }
    let done = repo::committed(&mut tx, line.harvest_report_id).await?;
    let kg = line.contributed_kg;
    if done + kg > h.quantity_kg {
        // 4b's format, shared with supply batches.
        return Err(AppError::Unprocessable(format!(
            "over-commitment: {:.2} kg available, {done:.2} kg already committed, {kg:.2} kg requested",
            h.quantity_kg
        )));
    }
    let new = NewLineRow {
        id: line.id,
        village_id: village,
        opportunity_id: opp,
        harvest_report_id: line.harvest_report_id,
        crop_cycle_id: h.crop_cycle_id,
        contributed_kg: kg,
        captured_by: access.user_id,
    };
    repo::insert_line(&mut tx, ws, &new).await.map_err(|e| {
        if pkey(&e, "farm_opportunity_supply_pkey") {
            AppError::Conflict(ID_TAKEN)
        } else {
            AppError::unique(
                e,
                "farm_opportunity_supply_line_key",
                "harvest_report_id",
                "This harvest is already in this opportunity.",
            )
        }
    })?;
    tx.commit().await?;
    Ok((
        find(state, access, opp).await?.ok_or(AppError::NotFound)?,
        true,
    ))
}

/// Current expected figures with what is promised from them: staff villages, plus a farmer's
/// own cycles.
pub async fn available(
    state: &AppState,
    access: &FarmAccess,
    village: Option<Uuid>,
    crop: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<AvailableRow>, i64), AppError> {
    let scope = access.staff_scope();
    Ok(repo::available(
        &state.db,
        access.workspace_id,
        scope.all,
        &scope.villages,
        access.person_id,
        village,
        crop,
        limit,
        offset,
    )
    .await?)
}

/// A village staff read: farmers 403, other villages and unknown ids 404.
async fn staff_village(
    state: &AppState,
    access: &FarmAccess,
    village: &str,
) -> Result<Uuid, AppError> {
    if !access.is_staff() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let village = Uuid::parse_str(village).map_err(|_| AppError::NotFound)?;
    if !access.staff_in(village)
        || !repo::village_exists(&state.db, access.workspace_id, village).await?
    {
        return Err(AppError::NotFound);
    }
    Ok(village)
}

/// Expected, committed and available kg by crop and month for one village.
pub async fn village_supply(
    state: &AppState,
    access: &FarmAccess,
    village: &str,
) -> Result<Vec<SupplyRow>, AppError> {
    let village = staff_village(state, access, village).await?;
    Ok(repo::village_supply(&state.db, access.workspace_id, village).await?)
}

/// Villages that could cover one open demand (ops and admin: demand is an ops surface).
pub async fn demand_matches(
    state: &AppState,
    access: &FarmAccess,
    demand: &str,
) -> Result<Vec<MatchRow>, AppError> {
    let d = self::demand(state, access, demand).await?;
    Ok(repo::matches(&state.db, access.workspace_id, Some(d.id), None).await?)
}

/// Open demands one village could cover (ops and admin).
pub async fn village_matches(
    state: &AppState,
    access: &FarmAccess,
    village: &str,
) -> Result<Vec<MatchRow>, AppError> {
    require_manager(access)?;
    let village = staff_village(state, access, village).await?;
    Ok(repo::matches(&state.db, access.workspace_id, None, Some(village)).await?)
}
