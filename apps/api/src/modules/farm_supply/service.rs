//! Supply batch rules: ops or admin buy verified, current, actual harvests of one village and
//! crop at a price per kg; the buyer must not have verified any of them; a harvest's kg over
//! all batches never pass its figure (RUAHA's double-commit guard); the farm's primary manager
//! is credited. One transaction makes the raw lot and the frozen batch; a client_ref replays.

use std::collections::{BTreeMap, HashMap};

use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::{
        currencies::repo as currencies,
        farm_supply::{
            dto::{
                BatchResource, BuyableHarvest, DispatchLinkResource, DispatchLot, DispatchOptions,
                DispatchWarehouse, NewBatch, OptionsResource, RawProduct, RawProductCreated,
            },
            repo::{self, HarvestRow, NewBatchRow},
        },
        farm_villages::repo as villages,
        lots::{repo::LotRow, service as lots},
        orders::repo as orders,
        products::{dto::Fields, repo as products, service as catalog},
        stock::{
            repo::{self as stock, Cause, Floor, Mutated, Slot},
            service::tracks_stock,
        },
        supply_links::dto::Partner,
        warehouses::repo as warehouses,
    },
};

const REF_TAKEN: &str = "This client_ref belongs to another supply batch.";
const OWN_VERIFICATION: &str = "A harvest you verified must be bought by someone else.";
const BAD_HARVEST: &str =
    "The harvest report must be the current, verified actual figure of this village and crop.";
const NO_MANAGER: &str = "The harvest's farm has no primary manager.";
const INVALID: &str = "is invalid.";

fn invalid(field: &str) -> String {
    format!("The selected {} {INVALID}", field.replace('_', " "))
}

/// Replay of an earlier batch under `client_ref`, or 409 when another member or body used it.
async fn replay(
    state: &AppState,
    access: &FarmAccess,
    input: &NewBatch,
    hash: &[u8; 32],
) -> Result<Option<BatchResource>, AppError> {
    let Some(r) = repo::find_ref(&state.db, access.workspace_id, input.client_ref).await? else {
        return Ok(None);
    };
    if r.created_by != access.user_id || r.request_hash != hash {
        return Err(AppError::Conflict(REF_TAKEN));
    }
    get_one(state, access.workspace_id, r.id, true, &[])
        .await
        .map(Some)
}

/// Field errors for references outside the workspace (village, crop, product, currency).
async fn check_refs(
    state: &AppState,
    ws: Uuid,
    input: &NewBatch,
) -> Result<(String, Option<Uuid>, BTreeMap<String, Vec<String>>), AppError> {
    let mut errors = BTreeMap::new();
    // The farm's store: the one sent, else the default warehouse, else none (an empty lot).
    let warehouse = match input.warehouse_id {
        Some(w) => {
            if !warehouses::exists(&state.db, ws, w).await? {
                errors.insert("warehouse_id".into(), vec![invalid("warehouse_id")]);
            }
            Some(w)
        }
        None => orders::default_warehouse(&state.db, ws).await?,
    };
    let village = villages::find(&state.db, ws, input.village_id).await?;
    if village.is_none() {
        errors.insert("village_id".into(), vec![invalid("village_id")]);
    }
    if !repo::crop_exists(&state.db, ws, input.crop_id).await? {
        errors.insert("crop_id".into(), vec![invalid("crop_id")]);
    }
    let product = products::find(&state.db, ws, input.raw_product_id).await?;
    if !product.as_ref().is_some_and(tracks_stock) {
        errors.insert("raw_product_id".into(), vec![invalid("raw_product_id")]);
    }
    let currency = currencies::find(&state.db, ws, input.currency_id).await?;
    if !currency.is_some_and(|c| c.active) {
        errors.insert("currency_id".into(), vec![invalid("currency_id")]);
    }
    Ok((
        village.map(|v| v.name).unwrap_or_default(),
        warehouse,
        errors,
    ))
}

/// The share line of each input line, or field errors / 403.
fn shares(
    access: &FarmAccess,
    input: &NewBatch,
    harvests: &[HarvestRow],
    committed: &[(Uuid, Decimal)],
) -> Result<Vec<(Uuid, Uuid, Decimal)>, AppError> {
    let mut errors = BTreeMap::new();
    let mut own = false;
    let mut out = Vec::with_capacity(input.lines.len());
    for (n, (id, kg)) in input.lines.iter().enumerate() {
        let field = format!("lines.{n}.harvest_report_id");
        let Some(h) = harvests.iter().find(|h| h.id == *id).filter(|h| {
            h.kind == "actual"
                && h.is_current
                && !h.deleted
                && h.verification == "verified"
                && h.village_id == input.village_id
                && h.crop_id == input.crop_id
        }) else {
            errors.insert(field, vec![BAD_HARVEST.to_owned()]);
            continue;
        };
        let Some(person) = h.person_id else {
            errors.insert(field, vec![NO_MANAGER.to_owned()]);
            continue;
        };
        own |= h.verified_by == Some(access.user_id);
        let done = committed
            .iter()
            .find(|(c, _)| c == id)
            .map_or(Decimal::ZERO, |c| c.1);
        if done + kg > h.quantity_kg {
            // RUAHA's opportunity_supply_guard wording, shared with the market module.
            let msg = format!(
                "over-commitment: {:.2} kg available, {done:.2} kg already committed, {kg:.2} kg requested",
                h.quantity_kg
            );
            errors.insert(format!("lines.{n}.kg"), vec![msg]);
        }
        out.push((*id, person, *kg));
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    if own {
        return Err(AppError::Forbidden(OWN_VERIFICATION));
    }
    Ok(out)
}

/// POST /supply-batches. True = created, false = replayed.
pub async fn create(
    state: &AppState,
    access: &FarmAccess,
    input: NewBatch,
) -> Result<(BatchResource, bool), AppError> {
    if !access.manages() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let ws = access.workspace_id;
    let hash = input.hash();
    if let Some(done) = replay(state, access, &input, &hash).await? {
        return Ok((done, false));
    }
    let (village_name, warehouse, errors) = check_refs(state, ws, &input).await?;
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    let ids: Vec<Uuid> = input.lines.iter().map(|l| l.0).collect();
    let mut tx = access.begin(&state.db).await?;
    let harvests = repo::lock_harvests(&mut tx, ws, &ids).await?;
    // A copy of this request that held these locks first has committed by now: replay it.
    // Checked on our own connection; the replay read waits until ours is back in the pool.
    if repo::find_ref(&mut *tx, ws, input.client_ref)
        .await?
        .is_some()
    {
        drop(tx);
        if let Some(done) = replay(state, access, &input, &hash).await? {
            return Ok((done, false));
        }
        return Err(AppError::Conflict(REF_TAKEN));
    }
    let locked: Vec<Uuid> = harvests.iter().map(|h| h.id).collect();
    let committed = repo::committed(&mut tx, &locked).await?;
    let lines = shares(access, &input, &harvests, &committed)?;
    let lot = LotRow {
        id: Uuid::nil(),
        product_id: input.raw_product_id,
        code: input.code.clone(),
        public_code: String::new(),
        status: "released".into(),
        produced_on: None,
        best_before: None,
        expires_on: None,
        origin: Some(village_name),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let meta = json!({ "supply_batch_client_ref": input.client_ref });
    let lot = lots::insert_lot(&mut tx, ws, access.user_id, lot, Some(meta)).await?;
    let row = NewBatchRow {
        client_ref: input.client_ref,
        request_hash: &hash,
        village_id: input.village_id,
        crop_id: input.crop_id,
        lot_id: lot.id,
        price_per_kg: input.price_per_kg,
        currency_id: input.currency_id,
        kg_total: lines.iter().map(|l| l.2).sum(),
        created_by: access.user_id,
    };
    let id = repo::insert_batch(&mut tx, ws, &row).await.map_err(|e| {
        match e.as_database_error().and_then(|d| d.constraint()) {
            Some("farm_supply_batches_client_ref_key") => AppError::Conflict(REF_TAKEN),
            _ => e.into(),
        }
    })?;
    repo::insert_lines(&mut tx, ws, input.village_id, id, &lines).await?;
    // Buying is the receipt: the kg go into the raw lot at the farm's store.
    if let Some(warehouse_id) = warehouse {
        let slot = Slot {
            product_id: input.raw_product_id,
            warehouse_id,
            bin: None,
            lot_id: Some(lot.id),
        };
        let cause = Cause {
            reason: Some("bought"),
            user_id: Some(access.user_id),
            related: Some(("supply_batch", id)),
            ..Cause::default()
        };
        let kg: Decimal = lines.iter().map(|l| l.2).sum();
        if let Mutated::OutOfRange =
            stock::mutate(&mut tx, ws, slot, kg, Floor::None, &cause).await?
        {
            return Err(AppError::field(
                "lines",
                "The stock level would be out of range.",
            ));
        }
    }
    tx.commit().await?;
    Ok((get_one(state, ws, id, true, &[]).await?, true))
}

async fn get_one(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
    all: bool,
    villages: &[Uuid],
) -> Result<BatchResource, AppError> {
    let scope = crate::http::farm::Scope {
        all,
        villages: villages.to_vec(),
    };
    let b = repo::find(&state.db, ws, id, &scope)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&state.db, ws, &[b.id]).await?;
    Ok(BatchResource::new(b, lines))
}

fn require_staff(access: &FarmAccess) -> Result<(), AppError> {
    if access.is_staff() {
        Ok(())
    } else {
        Err(AppError::Forbidden(NOT_ALLOWED))
    }
}

/// One batch, for staff of its village.
pub async fn get(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
) -> Result<BatchResource, AppError> {
    require_staff(access)?;
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let s = access.staff_scope();
    get_one(state, access.workspace_id, id, s.all, &s.villages).await
}

/// One page of batches of the caller's staff villages, newest first.
pub async fn list(
    state: &AppState,
    access: &FarmAccess,
    limit: i64,
    offset: i64,
) -> Result<(Vec<BatchResource>, i64), AppError> {
    require_staff(access)?;
    let ws = access.workspace_id;
    let (rows, total) = repo::list(&state.db, ws, &access.staff_scope(), limit, offset).await?;
    let ids: Vec<Uuid> = rows.iter().map(|b| b.id).collect();
    let mut by_batch: HashMap<Uuid, Vec<_>> = HashMap::new();
    for l in repo::lines(&state.db, ws, &ids).await? {
        by_batch.entry(l.batch_id).or_default().push(l);
    }
    let data = rows
        .into_iter()
        .map(|b| {
            let mine = by_batch.remove(&b.id).unwrap_or_default();
            BatchResource::new(b, mine)
        })
        .collect();
    Ok((data, total))
}

/// Hops a lineage may take back to its batch (packing and transfers; a few in practice).
const MAX_HOPS: usize = 4;

/// Where a lot's farmers come from: the batch's raw lot, the packing on the way (raw kg,
/// units), and the allow-list of the first transfer crossed walking back from the lot.
pub struct Lineage {
    pub batch_id: Uuid,
    pub root_workspace_id: Uuid,
    pub root_lot_id: Uuid,
    pub price_per_kg: i64,
    pub currency_id: Uuid,
    pub pack: Option<(Decimal, Decimal)>,
    /// Public fields allowed, as frozen on the received transfer; None = same workspace.
    pub fields: Option<Vec<String>>,
}

/// Walk back from `lot_id` through packing parents and received transfers to a supply
/// batch. None when the lot doesn't come from one.
pub async fn lineage(
    conn: &mut sqlx::PgConnection,
    lot_id: Uuid,
) -> Result<Option<Lineage>, AppError> {
    let (mut cur, mut pack, mut fields) = (lot_id, None, None);
    for _ in 0..=MAX_HOPS {
        let Some(h) = repo::hop(&mut *conn, cur).await? else {
            return Ok(None);
        };
        if let (Some(kg), Some(units)) = (h.pack_raw_kg, h.pack_units) {
            pack = pack.or(Some((kg, units)));
        }
        if fields.is_none() {
            fields = h.fields.clone();
        }
        match h.parent_lot_id.or(h.from_lot_id) {
            Some(next) => cur = next,
            None => {
                let batch = repo::batch_of_lot(&mut *conn, h.workspace_id, h.id).await?;
                return Ok(batch.map(|(batch_id, price_per_kg, currency_id)| Lineage {
                    batch_id,
                    root_workspace_id: h.workspace_id,
                    root_lot_id: h.id,
                    price_per_kg,
                    currency_id,
                    pack,
                    fields,
                }));
            }
        }
    }
    Ok(None)
}

/// The public farmer groups of `lot_id`: from its batch, cut to the link's allow-list when it
/// crossed one (no `village`: nothing; no `farmer_count`: no count; no `harvest_window`: no dates).
pub async fn public_farmers(
    db: &sqlx::PgPool,
    lot_id: Uuid,
) -> Result<Vec<repo::GroupRow>, AppError> {
    let Some(l) = lineage(&mut *db.acquire().await?, lot_id).await? else {
        return Ok(Vec::new());
    };
    let mut groups = repo::public_groups(db, l.root_workspace_id, l.root_lot_id).await?;
    if let Some(fields) = l.fields {
        let has = |f: &str| fields.iter().any(|x| x == f);
        if !has("village") {
            return Ok(Vec::new());
        }
        for g in &mut groups {
            if !has("farmer_count") {
                g.farmers = 0;
            }
            if !has("harvest_window") {
                g.harvest_start = None;
                g.harvest_end = None;
            }
        }
    }
    Ok(groups)
}

/// GET /supply-batches/options: harvests a new batch of `village` and `crop` may buy, the
/// products its raw lot may be, and whether the caller may add a product. ops or admin.
pub async fn options(
    state: &AppState,
    access: &FarmAccess,
    village_id: Uuid,
    crop_id: Uuid,
) -> Result<OptionsResource, AppError> {
    if !access.manages() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let ws = access.workspace_id;
    let mut errors = BTreeMap::new();
    if villages::find(&state.db, ws, village_id).await?.is_none() {
        errors.insert("village_id".into(), vec![invalid("village_id")]);
    }
    if !repo::crop_exists(&state.db, ws, crop_id).await? {
        errors.insert("crop_id".into(), vec![invalid("crop_id")]);
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    let harvests = repo::buyable(&state.db, ws, village_id, crop_id, access.user_id).await?;
    let raw = repo::raw_products(&state.db, ws, crop_id).await?;
    Ok(OptionsResource {
        harvests: harvests
            .into_iter()
            .map(|h| BuyableHarvest {
                harvest_report_id: h.id,
                farmer_name: h.farmer_name,
                plot_label: h.plot_label,
                quantity_kg: format!("{:.2}", h.quantity_kg),
                committed_kg: format!("{:.2}", h.committed_kg),
                available_kg: format!("{:.2}", h.quantity_kg - h.committed_kg),
            })
            .collect(),
        raw_products: raw
            .into_iter()
            .map(|p| RawProduct {
                id: p.id,
                name: p.name,
                unit: p.unit,
                suggested_for_crop: p.suggested,
            })
            .collect(),
        can_create_product: access.manages(),
    })
}

/// POST /supply-batches/raw-product: ops or admin add a raw material through the products
/// service (numbering, default variant, VAT default), no general `product.create` needed.
pub async fn create_raw_product(
    state: &AppState,
    access: &FarmAccess,
    fields: Fields,
) -> Result<RawProductCreated, AppError> {
    if !access.manages() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let actor = catalog::Actor {
        user_id: access.user_id,
        flow: "manual",
    };
    let id = catalog::create(state, access.workspace_id, fields, actor).await?;
    let p = products::find(&state.db, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(RawProductCreated {
        id: p.id,
        name: p.name,
        unit: p.unit,
    })
}

/// GET /dispatch/options: ops or admin; the live links, batch lots and stocked warehouses a
/// dispatch may use.
pub async fn dispatch_options(
    state: &AppState,
    access: &FarmAccess,
) -> Result<DispatchOptions, AppError> {
    if !access.manages() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let (db, ws) = (&state.db, access.workspace_id);
    let links = repo::dispatch_links(db, ws).await?;
    let lots = repo::batch_lots(db, ws).await?;
    let warehouses = repo::dispatch_warehouses(db, ws).await?;
    Ok(DispatchOptions {
        links: links
            .into_iter()
            .map(|l| DispatchLinkResource {
                id: l.id,
                partner: Partner::new(l.partner_id, l.partner_name, l.partner_display_name),
            })
            .collect(),
        lots: lots
            .into_iter()
            .map(|l| DispatchLot {
                id: l.id,
                code: l.name,
            })
            .collect(),
        warehouses: warehouses
            .into_iter()
            .map(|w| DispatchWarehouse {
                id: w.id,
                name: w.name,
            })
            .collect(),
    })
}
