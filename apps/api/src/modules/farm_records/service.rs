//! Farm record rules (RUAHA `app_register_farmer`, `app_verify`, `app_update_observed_record`,
//! `app_supersede_harvest`). Reads: staff see their villages, a farmer sees what hangs off
//! their own person. Writes: staff only, in their villages, stamped with provenance.
//! Registration never verifies; verification is one-way, and a household needs a verifier
//! other than its registrar.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::farm_records::{
        dto::{
            CycleFields, CycleFix, NewFarm, NewHousehold, NewRegistration, NewRevision, PersonFix,
            PlaceFix, RegistrationResult,
        },
        repo::{self, CycleCols, Fix, HarvestRow, Pick, Seen, Stamp},
    },
};

const FIELD_VERIFIED: &str = "field_verified";
const FARMER_REPORTED: &str = "farmer_reported";

/// Record kinds with a verification route.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Person,
    Household,
    Farm,
    Plot,
    Cycle,
    Harvest,
}

/// The caller as a reader.
pub fn seen<'a>(access: &FarmAccess, villages: &'a [Uuid]) -> Seen<'a> {
    Seen {
        ws: access.workspace_id,
        all: access.manages(),
        villages,
        person: access.person_id,
    }
}

fn require_staff(access: &FarmAccess) -> Result<(), AppError> {
    if access.is_staff() {
        Ok(())
    } else {
        Err(AppError::Forbidden(NOT_ALLOWED))
    }
}

/// Path id -> uuid, junk is 404.
pub fn path_id(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// First row of a one-row read, else 404.
pub fn one<T>(rows: (Vec<T>, i64)) -> Result<T, AppError> {
    rows.0.into_iter().next().ok_or(AppError::NotFound)
}

// ---- registration ----

/// Measure rule for one cycle: the column the crop's measure needs must be filled.
fn measure_missing(measured_by: &str, f: &CycleFields) -> Option<&'static str> {
    match measured_by {
        "area" if f.area_ha.is_none() => Some("this crop is measured by area: area_ha is required"),
        "tree_count" if f.tree_count.is_none() => {
            Some("this crop is measured by tree count: tree_count is required")
        }
        "unit_count" if f.unit_count.is_none() => {
            Some("this crop is measured by unit count: unit_count is required")
        }
        _ => None,
    }
}

fn cols(crop_id: Uuid, f: &CycleFields) -> CycleCols<'_> {
    CycleCols {
        crop_id,
        season_label: f.season_label.as_deref(),
        area_ha: f.area_ha,
        tree_count: f.tree_count,
        unit_count: f.unit_count,
        planted_on: f.planted_on,
        harvest_start: f.harvest_start,
        harvest_end: f.harvest_end,
        status: &f.status,
    }
}

/// Provenance of a row an officer writes.
fn officer(ws: Uuid, village: Uuid, actor: Uuid, confidence: Option<&str>) -> Stamp<'_> {
    Stamp {
        ws,
        village,
        source: FIELD_VERIFIED,
        actor,
        confidence,
    }
}

/// Register a farmer in one transaction. False = replayed from the receipt.
pub async fn register(
    state: &AppState,
    access: &FarmAccess,
    r: NewRegistration,
) -> Result<(RegistrationResult, bool), AppError> {
    let ws = access.workspace_id;
    let db = &state.db;
    // A retry after a lost reply returns the first result and writes nothing.
    if let Some(receipt) = repo::receipt(db, ws, r.client_ref).await? {
        if receipt.created_by != access.user_id {
            return Err(AppError::Conflict(
                "This client_ref belongs to another registration.",
            ));
        }
        let mut result: RegistrationResult = serde_json::from_value(receipt.result)
            .map_err(|e| AppError::Internal(format!("registration receipt: {e}")))?;
        result.replayed = true;
        return Ok((result, false));
    }
    require_staff(access)?;
    if !repo::village_exists(db, ws, r.village_id).await? {
        return Err(AppError::field(
            "village_id",
            "no such village in this workspace",
        ));
    }
    if !access.staff_in(r.village_id) {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let crop_ids: Vec<Uuid> = r.cycles.iter().map(|c| c.crop_id).collect();
    let measures = repo::crop_measures(db, ws, &crop_ids).await?;
    for c in &r.cycles {
        let measured_by = measures
            .iter()
            .find(|(id, _)| *id == c.crop_id)
            .map(|(_, m)| m.as_str())
            .ok_or_else(|| AppError::field("cycles", "unknown crop"))?;
        if let Some(message) = measure_missing(measured_by, &c.fields) {
            return Err(AppError::field("cycles", message));
        }
    }

    let v = r.village_id;
    let me = access.user_id;
    let mut tx = access.begin(db).await?;
    let p = &r.person;
    let person = repo::insert_person(
        &mut *tx,
        &officer(ws, v, me, p.confidence.as_deref()),
        &p.given_name,
        &p.family_name,
        &p.phone,
    )
    .await?;

    let household = match &r.household {
        NewHousehold::Create { label, is_head } => {
            let label = label
                .clone()
                .unwrap_or_else(|| format!("{} household", p.family_name));
            let id = repo::insert_household(&mut *tx, &officer(ws, v, me, None), &label).await?;
            repo::insert_member(&mut *tx, ws, v, id, person, *is_head).await?;
            id
        }
        NewHousehold::Join { id, is_head } => {
            let has_head = repo::household_for_join(&mut *tx, ws, v, *id)
                .await?
                .ok_or_else(|| {
                    AppError::field(
                        "household.existing_household_id",
                        "no such household in this village",
                    )
                })?;
            if has_head && *is_head {
                return Err(AppError::field(
                    "household.is_head",
                    "this household already has a head",
                ));
            }
            repo::insert_member(&mut *tx, ws, v, *id, person, *is_head).await?;
            *id
        }
    };

    let farm = match &r.farm {
        NewFarm::Create {
            label,
            latitude,
            longitude,
            confidence,
        } => {
            let id = repo::insert_farm(
                &mut *tx,
                &officer(ws, v, me, confidence.as_deref()),
                household,
                label,
                *latitude,
                *longitude,
            )
            .await?;
            repo::insert_manager(&mut *tx, ws, v, id, person, true).await?;
            id
        }
        NewFarm::Join(id) => {
            let has_primary = repo::farm_for_join(&mut *tx, ws, v, *id)
                .await?
                .ok_or_else(|| {
                    AppError::field("farm.existing_farm_id", "no such farm in this village")
                })?;
            repo::insert_manager(&mut *tx, ws, v, *id, person, !has_primary).await?;
            *id
        }
    };

    let pl = &r.plot;
    let plot = repo::insert_plot(
        &mut *tx,
        &officer(ws, v, me, pl.confidence.as_deref()),
        farm,
        &pl.label,
        pl.area_ha,
        pl.latitude,
        pl.longitude,
    )
    .await?;

    let mut cycle_ids = Vec::with_capacity(r.cycles.len());
    let mut harvest_ids = Vec::new();
    for c in &r.cycles {
        let cycle = repo::insert_cycle(
            &mut *tx,
            &officer(ws, v, me, c.confidence.as_deref()),
            plot,
            &cols(c.crop_id, &c.fields),
        )
        .await?;
        cycle_ids.push(cycle);
        if let Some(h) = &c.harvest {
            // The farmer is the source of the expected figure, whoever typed it.
            let farmer = Stamp {
                source: FARMER_REPORTED,
                ..officer(ws, v, me, h.confidence.as_deref())
            };
            let id = repo::insert_harvest(
                &mut *tx,
                &farmer,
                cycle,
                "expected",
                h.quantity_kg,
                h.reported_for,
            )
            .await?;
            harvest_ids.push(id);
        }
    }

    let result = RegistrationResult {
        person_id: person,
        household_id: household,
        farm_id: farm,
        plot_id: plot,
        village_id: v,
        crop_cycle_ids: cycle_ids,
        harvest_report_ids: harvest_ids,
        replayed: false,
    };
    let stored = serde_json::to_value(&result).map_err(|e| AppError::Internal(e.to_string()))?;
    repo::insert_receipt(&mut *tx, ws, r.client_ref, access.user_id, &stored)
        .await
        .map_err(
            |e| match e.as_database_error().and_then(|d| d.constraint()) {
                // The same form submitted twice at once: the other request wins, retry replays.
                Some("farm_registration_receipts_pkey") => AppError::Conflict(
                    "This registration is already being saved; retry to get its result.",
                ),
                _ => AppError::Database(e),
            },
        )?;
    tx.commit().await?;
    Ok((result, true))
}

// ---- verification ----

/// Verify a record: staff of its village, one-way, four eyes on households.
pub async fn verify(
    state: &AppState,
    access: &FarmAccess,
    kind: Kind,
    id: &str,
) -> Result<Uuid, AppError> {
    require_staff(access)?;
    let id = path_id(id)?;
    let scope = access.staff_scope();
    let (ws, all, v) = (access.workspace_id, scope.all, scope.villages.as_slice());
    let mut tx = access.begin(&state.db).await?;
    let row = match kind {
        Kind::Person => repo::lock_person(&mut *tx, ws, all, v, id).await?,
        Kind::Household => repo::lock_household(&mut *tx, ws, all, v, id).await?,
        Kind::Farm => repo::lock_farm(&mut *tx, ws, all, v, id).await?,
        Kind::Plot => repo::lock_plot(&mut *tx, ws, all, v, id).await?,
        Kind::Cycle => repo::lock_cycle_for_verify(&mut *tx, ws, all, v, id).await?,
        Kind::Harvest => repo::lock_harvest(&mut *tx, ws, all, v, id).await?,
    }
    .ok_or(AppError::NotFound)?;
    if !matches!(row.verification.as_str(), "unverified" | "pending") {
        return Err(AppError::Unprocessable(
            "record is not awaiting verification".into(),
        ));
    }
    // A household is what an incentive is paid to: one person never creates and vouches alone.
    if kind == Kind::Household && row.captured_by == Some(access.user_id) {
        return Err(AppError::Unprocessable(
            "a household must be verified by someone other than the officer who registered it"
                .into(),
        ));
    }
    let me = access.user_id;
    match kind {
        Kind::Person => repo::verify_person(&mut *tx, id, me).await?,
        Kind::Household => repo::verify_household(&mut *tx, id, me).await?,
        Kind::Farm => repo::verify_farm(&mut *tx, id, me).await?,
        Kind::Plot => repo::verify_plot(&mut *tx, id, me).await?,
        Kind::Cycle => repo::verify_cycle(&mut *tx, id, me).await?,
        Kind::Harvest => repo::verify_harvest(&mut *tx, id, me).await?,
    }
    tx.commit().await?;
    Ok(id)
}

// ---- corrections ----

/// Run one correction under the caller's staff scope; 404 when the row isn't theirs.
async fn correct<F>(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    write: F,
) -> Result<Uuid, AppError>
where
    F: AsyncFnOnce(&mut sqlx::PgConnection, &Fix<'_>) -> Result<bool, AppError>,
{
    require_staff(access)?;
    let id = path_id(id)?;
    let scope = access.staff_scope();
    let fix = Fix {
        ws: access.workspace_id,
        id,
        all: scope.all,
        villages: &scope.villages,
        actor: access.user_id,
    };
    let mut tx = access.begin(&state.db).await?;
    if !write(&mut *tx, &fix).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(id)
}

pub async fn correct_person(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    p: PersonFix,
) -> Result<Uuid, AppError> {
    correct(state, access, id, async |c, f| {
        Ok(repo::correct_person(c, f, &p.given_name, &p.family_name, p.phone.as_deref()).await?)
    })
    .await
}

pub async fn correct_household(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    label: String,
) -> Result<Uuid, AppError> {
    correct(state, access, id, async |c, f| {
        Ok(repo::correct_household(c, f, &label).await?)
    })
    .await
}

pub async fn correct_farm(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    p: PlaceFix,
) -> Result<Uuid, AppError> {
    correct(state, access, id, async |c, f| {
        Ok(repo::correct_farm(c, f, &p.label, p.latitude, p.longitude).await?)
    })
    .await
}

pub async fn correct_plot(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    p: PlaceFix,
) -> Result<Uuid, AppError> {
    correct(state, access, id, async |c, f| {
        Ok(repo::correct_plot(c, f, &p.label, p.area_ha, p.latitude, p.longitude).await?)
    })
    .await
}

/// A cycle keeps exactly the measure its crop is measured by.
pub async fn correct_cycle(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    fix: CycleFix,
) -> Result<Uuid, AppError> {
    require_staff(access)?;
    let measured_by = repo::crop_measures(&state.db, access.workspace_id, &[fix.crop_id])
        .await?
        .pop()
        .map(|(_, m)| m)
        .ok_or_else(|| AppError::field("crop_id", "crop does not exist or is inactive"))?;
    let f = &fix.fields;
    let (has_area, has_trees, has_units) = (
        f.area_ha.is_some(),
        f.tree_count.is_some(),
        f.unit_count.is_some(),
    );
    let wrong = match measured_by.as_str() {
        "area" if has_trees || has_units => Some(("area_ha", "only area is valid for this crop")),
        "tree_count" if has_area || has_units => {
            Some(("tree_count", "only tree count is valid for this crop"))
        }
        "unit_count" if has_area || has_trees => {
            Some(("unit_count", "only unit count is valid for this crop"))
        }
        "area" if !has_area => Some(("area_ha", "area is required for this crop")),
        "tree_count" if !has_trees => Some(("tree_count", "tree count is required for this crop")),
        "unit_count" if !has_units => Some(("unit_count", "unit count is required for this crop")),
        _ => None,
    };
    if let Some((field, message)) = wrong {
        return Err(AppError::field(field, message));
    }
    correct(state, access, id, async |c, fx| {
        Ok(repo::correct_cycle(c, fx, &cols(fix.crop_id, f)).await?)
    })
    .await
}

// ---- harvests ----

/// Replace the current figure of this kind; the old one stays, no longer current.
pub async fn revise_harvest(
    state: &AppState,
    access: &FarmAccess,
    cycle: &str,
    rev: NewRevision,
) -> Result<HarvestRow, AppError> {
    require_staff(access)?;
    let cycle = path_id(cycle)?;
    let scope = access.staff_scope();
    let ws = access.workspace_id;
    let mut tx = access.begin(&state.db).await?;
    // The cycle lock serialises revisions, so the one-current index never sees two.
    let village = repo::lock_cycle(&mut *tx, ws, scope.all, &scope.villages, cycle)
        .await?
        .ok_or(AppError::NotFound)?;
    repo::retire_current(&mut *tx, cycle, &rev.kind).await?;
    let stamp = Stamp {
        ws,
        village,
        source: &rev.source,
        actor: access.user_id,
        confidence: rev.confidence.as_deref(),
    };
    let id = repo::insert_harvest(
        &mut *tx,
        &stamp,
        cycle,
        &rev.kind,
        rev.quantity_kg,
        rev.reported_for,
    )
    .await?;
    tx.commit().await?;
    one(repo::harvests(&state.db, ws, None, Some(id), false, 1, 0).await?)
}

/// Harvest reports of a cycle the caller sees, else 404.
pub async fn harvests(
    state: &AppState,
    access: &FarmAccess,
    cycle: &str,
    current_only: bool,
    limit: i64,
    offset: i64,
) -> Result<(Vec<HarvestRow>, i64), AppError> {
    let cycle = path_id(cycle)?;
    let scope = access.staff_scope();
    one(repo::cycles(&state.db, &seen(access, &scope.villages), &Pick::one(cycle)).await?)?;
    Ok(repo::harvests(
        &state.db,
        access.workspace_id,
        Some(cycle),
        None,
        current_only,
        limit,
        offset,
    )
    .await?)
}

/// One harvest report, after its cycle's visibility check.
pub async fn harvest(
    state: &AppState,
    access: &FarmAccess,
    id: Uuid,
) -> Result<HarvestRow, AppError> {
    let row =
        one(repo::harvests(&state.db, access.workspace_id, None, Some(id), false, 1, 0).await?)?;
    let scope = access.staff_scope();
    one(repo::cycles(
        &state.db,
        &seen(access, &scope.villages),
        &Pick::one(row.crop_cycle_id),
    )
    .await?)?;
    Ok(row)
}
