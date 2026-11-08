//! Review facts for farm and supply writes: names, not ids, on the approval card. Display
//! only; the native handler still checks every rule when the approved action runs.

use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{dto::ProposalInput, preconditions::validated, repo::Owner};
use crate::{
    AppError,
    http::farm::{ADMIN, FIELD_OFFICER, FarmRole, OPS},
    modules::{
        farm_memberships::repo::live_roles, farm_records, farm_supply, permissions, supply_chain,
        supply_links,
    },
};

const DENIED: &str = "You do not have permission for this action.";

/// Operations this module describes.
pub fn describes(operation_id: &str) -> bool {
    matches!(
        operation_id,
        "farm_supply_create"
            | "farm_records_harvest_reports_revise"
            | "supply_chain_dispatch"
            | "supply_links_grant"
    )
}

/// Facts for a farm or supply write, `None` for any other operation. The caller must hold
/// the role the operation needs, else 403 before any name is read.
pub async fn capture(
    conn: &mut PgConnection,
    owner: Owner,
    input: &ProposalInput,
) -> Result<Option<Value>, AppError> {
    let operation = input.operation_id.as_str();
    if !describes(operation) {
        return Ok(None);
    }
    let ws = owner.workspace_id;
    let body = input.body.clone().unwrap_or(json!({}));
    let roles = live_roles(&mut *conn, ws, owner.user_id).await?;
    let manages = roles.iter().any(|r| r.role == OPS || r.role == ADMIN);
    let facts = match operation {
        "farm_supply_create" => {
            if !manages {
                return Err(AppError::Forbidden(DENIED));
            }
            let batch = validated::<farm_supply::dto::CreateBatchRequest>(body)?;
            supply(conn, ws, &batch).await?
        }
        "farm_records_harvest_reports_revise" => {
            let revision = validated::<farm_records::dto::HarvestRevision>(body)?;
            harvest(conn, ws, &roles, segment(input, 6)?, &revision).await?
        }
        "supply_chain_dispatch" => {
            if !manages && !granted(conn, owner, "product.update").await? {
                return Err(AppError::Forbidden(DENIED));
            }
            let dispatch = validated::<supply_chain::dto::DispatchRequest>(body)?;
            dispatched(conn, ws, segment(input, 5)?, &dispatch).await?
        }
        _ => {
            if !granted(conn, owner, permissions::catalog::WORKSPACE_ADMIN).await? {
                return Err(AppError::Forbidden(DENIED));
            }
            let link = validated::<supply_links::dto::GrantLinkRequest>(body)?;
            grant(conn, ws, &link).await?
        }
    };
    Ok(Some(
        json!({"version":1,"operation_id":operation,"facts":facts}),
    ))
}

/// Path segment `n` of `/v1/workspace/{workspace}/…` as a uuid; 404 otherwise.
fn segment(input: &ProposalInput, n: usize) -> Result<Uuid, AppError> {
    input
        .path
        .split('/')
        .nth(n)
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or(AppError::NotFound)
}

async fn granted(conn: &mut PgConnection, owner: Owner, name: &str) -> Result<bool, AppError> {
    Ok(
        permissions::repo::access(&mut *conn, owner.workspace_id, owner.user_id)
            .await?
            .is_some_and(|a| {
                a.active && permissions::model::Granted::new(a.owner, a.permissions).allows(name)
            }),
    )
}

fn kg(q: Decimal) -> String {
    format!("{q:.2}")
}

fn invalid(field: &str) -> AppError {
    AppError::field(
        field,
        format!("The selected {} is invalid.", field.replace('_', " ")),
    )
}

async fn supply(
    conn: &mut PgConnection,
    ws: Uuid,
    b: &farm_supply::dto::NewBatch,
) -> Result<Value, AppError> {
    // shortcut: English crop name on every card, pick by locale once office sends one.
    let head = sqlx::query!(
        r#"SELECT v.name AS village, cr.name_en AS crop, cu.asset_type AS currency, cu.decimals
           FROM farm_villages v, farm_crops cr, currencies cu
           WHERE v.workspace_id = $1 AND v.id = $2 AND cr.workspace_id = $1 AND cr.id = $3
             AND cu.workspace_id = $1 AND cu.id = $4"#,
        ws,
        b.village_id,
        b.crop_id,
        b.currency_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| invalid("village_id"))?;
    let ids: Vec<Uuid> = b.lines.iter().map(|(h, _)| *h).collect();
    let farmers = sqlx::query!(
        r#"SELECT h.id, concat_ws(' ', mp.given_name, mp.family_name) AS "farmer!"
           FROM farm_harvest_reports h
           JOIN farm_crop_cycles c ON c.workspace_id = h.workspace_id AND c.id = h.crop_cycle_id
           JOIN farm_plots pl ON pl.workspace_id = c.workspace_id AND pl.id = c.plot_id
           LEFT JOIN farm_managers m ON m.farm_id = pl.farm_id AND m.is_primary
           LEFT JOIN farm_persons mp ON mp.id = m.person_id
           WHERE h.workspace_id = $1 AND h.id = ANY($2)"#,
        ws,
        &ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut lines = Vec::with_capacity(b.lines.len());
    for (i, (id, q)) in b.lines.iter().enumerate() {
        let farmer = farmers
            .iter()
            .find(|f| f.id == *id)
            .ok_or_else(|| invalid(&format!("lines.{i}.harvest_report_id")))?;
        lines.push(json!({"farmer": farmer.farmer, "kg": kg(*q)}));
    }
    let total: Decimal = b.lines.iter().map(|(_, q)| *q).sum();
    Ok(json!({
        "kind": "farm_supply",
        "locked": {"label": format!("{} · {}", head.village, head.crop)},
        "priced": {
            "lines": lines,
            "kg_total": kg(total),
            "price_per_kg_minor": b.price_per_kg,
            "currency": {"code": head.currency, "decimals": head.decimals},
        },
    }))
}

async fn harvest(
    conn: &mut PgConnection,
    ws: Uuid,
    roles: &[FarmRole],
    cycle: Uuid,
    r: &farm_records::dto::NewRevision,
) -> Result<Value, AppError> {
    let row = sqlx::query!(
        r#"SELECT c.village_id, concat_ws(' ', mp.given_name, mp.family_name) AS "farmer!",
                  cr.name_en AS crop,
                  (SELECT h.quantity_kg FROM farm_harvest_reports h
                   WHERE h.workspace_id = c.workspace_id AND h.crop_cycle_id = c.id
                     AND h.kind = $3 AND h.is_current AND h.deleted_at IS NULL
                   ORDER BY h.created_at DESC LIMIT 1) AS current_kg
           FROM farm_crop_cycles c
           JOIN farm_crops cr ON cr.workspace_id = c.workspace_id AND cr.id = c.crop_id
           JOIN farm_plots pl ON pl.workspace_id = c.workspace_id AND pl.id = c.plot_id
           LEFT JOIN farm_managers m ON m.farm_id = pl.farm_id AND m.is_primary
           LEFT JOIN farm_persons mp ON mp.id = m.person_id
           WHERE c.workspace_id = $1 AND c.id = $2"#,
        ws,
        cycle,
        r.kind,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let staff = roles.iter().any(|x| {
        x.role == OPS
            || x.role == ADMIN
            || (x.role == FIELD_OFFICER && x.village_id == Some(row.village_id))
    });
    if !staff {
        return Err(AppError::Forbidden(DENIED));
    }
    Ok(json!({
        "kind": "harvest",
        "locked": {"label": format!("{} · {}", row.farmer, row.crop), "figure": r.kind},
        "changes": [{
            "field": "quantity_kg",
            "before": row.current_kg.map(kg),
            "after": kg(r.quantity_kg),
        }],
        "note": "The previous figure stays on record.",
    }))
}

async fn dispatched(
    conn: &mut PgConnection,
    ws: Uuid,
    lot: Uuid,
    d: &supply_chain::dto::NewDispatch,
) -> Result<Value, AppError> {
    let row = sqlx::query!(
        r#"SELECT l.code AS lot, p.name AS product,
                  (SELECT coalesce(nullif(btrim(w.profile->>'company_name'), ''), w.name)
                   FROM supply_links s JOIN workspaces w ON w.id = s.to_workspace_id
                   WHERE s.id = $3 AND s.from_workspace_id = $1) AS partner,
                  (SELECT name FROM warehouses WHERE workspace_id = $1 AND id = $4) AS warehouse
           FROM lots l JOIN products p ON p.workspace_id = l.workspace_id AND p.id = l.product_id
           WHERE l.workspace_id = $1 AND l.id = $2"#,
        ws,
        lot,
        d.link_id,
        d.warehouse_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let partner = row.partner.ok_or_else(|| invalid("link_id"))?;
    let warehouse = row.warehouse.ok_or_else(|| invalid("warehouse_id"))?;
    Ok(json!({
        "kind": "dispatch",
        "locked": {
            "label": row.lot,
            "product": row.product,
            "partner": partner,
            "warehouse": warehouse,
        },
        "changes": [{"field": "quantity", "after": kg(d.quantity)}],
    }))
}

async fn grant(
    conn: &mut PgConnection,
    ws: Uuid,
    l: &supply_links::dto::NewLink,
) -> Result<Value, AppError> {
    let partner = sqlx::query_scalar!(
        r#"SELECT coalesce(nullif(btrim(profile->>'company_name'), ''), name) AS "name!"
           FROM workspaces WHERE id = $1 AND id <> $2 AND deleted_at IS NULL"#,
        l.to_workspace_id,
        ws,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| invalid("to_workspace_id"))?;
    Ok(json!({
        "kind": "supply_link",
        "locked": {"label": partner},
        "fields": l.fields,
        "note": "Farmer names and photos stay off.",
    }))
}
