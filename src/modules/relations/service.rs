//! Relation rules: create with number + inline default contact in one transaction, detail view.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::Rules,
        list::{Page, Paginated},
    },
    modules::{
        contacts, custom_types, discount_groups,
        relations::{
            address,
            dto::{
                Detail, IndexFlags, NewRelation, RelationIndexResource, RelationPatch,
                RelationResource, TYPE_NOT_VALID, WorkspaceCtx, keeps_ftp_password,
            },
            phone,
            repo::{self, ListFilter},
            search, vat_number,
        },
        settings, vat_groups, workspaces,
    },
};

/// Malformed id = unknown id = 404.
pub fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Insert relation (+ inline contact) under the per-workspace number lock; returns its id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    mut input: NewRelation,
) -> Result<Uuid, AppError> {
    let mut tx = state.db.begin().await?;
    let relation_type_id = match input.relation_type_id {
        Some(id) => {
            if !custom_types::repo::is_live(&mut *tx, workspace_id, id, Some("relation")).await? {
                return Err(AppError::field("relation_type_id", TYPE_NOT_VALID));
            }
            id
        }
        None => custom_types::repo::first_of_domain(&mut *tx, workspace_id, "relation")
            .await?
            .ok_or_else(|| {
                AppError::field(
                    "relation_type_id",
                    "The relation type id field is required.",
                )
            })?,
    };
    if let Some(id) = input.source_type_id
        && !custom_types::repo::is_live(&mut *tx, workspace_id, id, Some("lead_source")).await?
    {
        return Err(AppError::field(
            "source_type_id",
            "The source type id is not valid",
        ));
    }
    let country = phone::country_of(&input.addresses);
    input.phone_number = phone::normalize(input.phone_number.as_deref(), country);
    if let Some(c) = &mut input.contact {
        c.default_contact = true;
        c.phone_number = phone::normalize(c.phone_number.as_deref(), country);
        phone::normalize_phones(&mut c.phones, country);
    }
    // Lock first, then read the series, so the snapshot is the latest settings.
    repo::lock_numbers(&mut tx, workspace_id).await?;
    let (appendix, start) =
        settings::service::numbering(&mut *tx, workspace_id, "relation").await?;
    let number = repo::next_number(&mut tx, workspace_id, start).await?;
    let appendix = appendix.to_json();
    let id = repo::insert(
        &mut *tx,
        workspace_id,
        user_id,
        relation_type_id,
        number,
        &appendix,
        &input,
    )
    .await?;
    if let Some(c) = &input.contact {
        contacts::repo::insert(&mut *tx, workspace_id, id, c).await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// GET flags (Laravel GetRelationRequest).
pub struct GetFlags {
    pub slim: bool,
    pub include_unpaid_invoices: bool,
}

/// Detail view of a live relation of this workspace, else 404. Five fixed queries.
pub async fn get(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    flags: GetFlags,
) -> Result<RelationResource, AppError> {
    let id = parse(id)?;
    let (row, relation_type, source_type) = repo::find_detail(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let contacts = contacts::repo::for_relations(&state.db, workspace_id, &[id]).await?;
    let (country, language) = workspaces::repo::locale(&state.db, workspace_id).await?;
    let vat_groups = vat_groups::repo::all(&state.db, workspace_id).await?;
    let (supplier_discount_groups, relation_discount_groups) =
        discount_groups::repo::for_relation(&state.db, workspace_id, id).await?;
    Ok(Detail {
        supplier_discount_groups,
        relation_discount_groups,
        row,
        relation_type,
        source_type,
        contacts,
        vat_groups,
        workspace: WorkspaceCtx { country, language },
        slim: flags.slim,
        include_unpaid_invoices: flags.include_unpaid_invoices,
    }
    .into())
}

/// What a list or `/searches?domain=relation` call asks for.
pub struct IndexQuery<'a> {
    pub q: Option<&'a str>,
    pub suppliers: bool,
    pub include_advice: bool,
    pub last_name: Option<&'a str>,
    pub zipcode: Option<&'a str>,
    pub company_name: Option<&'a str>,
    pub page: Page,
}

/// Trimmed optional filter, capped like Laravel's request rules; None when blank.
fn filter(r: &mut Rules, key: &str, v: Option<&str>, max: usize) -> Option<String> {
    let v = v.map(str::trim).filter(|v| !v.is_empty())?;
    r.max_chars(key, v, max);
    Some(v.to_owned())
}

/// Filters + search -> one page. A `q` emptied by cleanup ("null undefined") or a zipcode
/// with no letters or digits matches nothing.
pub async fn index(
    state: &AppState,
    workspace_id: Uuid,
    iq: IndexQuery<'_>,
) -> Result<Paginated<RelationIndexResource>, AppError> {
    let mut r = Rules::default();
    let last_name = filter(&mut r, "last_name", iq.last_name, 255).map(|l| l.to_lowercase());
    let zip = filter(&mut r, "zipcode", iq.zipcode, 10);
    let company = filter(&mut r, "company_name", iq.company_name, 255).map(|c| c.to_lowercase());
    r.into_result()?;
    let empty = || Paginated::new(Vec::new(), 0, iq.page);
    let zipcode = match zip.as_deref().map(search::compact_zipcode) {
        Some(None) => return Ok(empty()),
        other => other.flatten(),
    };
    let query = match search::parse(iq.q)? {
        Some(query) => query,
        None if last_name.is_none() && zipcode.is_none() && company.is_none() => {
            return Ok(empty());
        }
        None => search::Query::default(),
    };
    let f = ListFilter {
        suppliers: iq.suppliers,
        last_name: last_name.as_deref(),
        zipcode: zipcode.as_deref(),
        company_name: company.as_deref(),
        q: &query,
        limit: iq.page.size,
        offset: iq.page.offset(),
    };
    let flags = IndexFlags {
        suppliers: iq.suppliers,
        include_advice: iq.include_advice,
    };
    let (data, total) = list(state, workspace_id, &f, flags).await?;
    Ok(Paginated::new(data, total, iq.page))
}

/// One list page (index rows with their contacts) plus the matching total. Two queries; a
/// page past the end re-reads the first row only to get the total.
async fn list(
    state: &AppState,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
    flags: IndexFlags,
) -> Result<(Vec<RelationIndexResource>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, f).await?;
    if rows.is_empty() && f.offset > 0 {
        let first = ListFilter {
            limit: 1,
            offset: 0,
            ..*f
        };
        total = repo::list(&state.db, workspace_id, &first).await?.1;
    }
    let ids: Vec<Uuid> = rows.iter().map(|(r, _, _)| r.id).collect();
    let mut contacts = contacts::repo::for_relations(&state.db, workspace_id, &ids).await?;
    let data = rows
        .into_iter()
        .map(|row| {
            let id = row.0.id;
            let mine = contacts.extract_if(.., |c| c.relation_id == id).collect();
            RelationIndexResource::new(row, mine, flags)
        })
        .collect();
    Ok((data, total))
}

/// Relations among `ids` in RelationIndexResource shape, keyed by id (missing or deleted ones
/// absent). Two queries whatever the count; for resources that embed a relation.
pub async fn index_values(
    state: &AppState,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, Value>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = repo::find_many(&state.db, workspace_id, ids).await?;
    let found: Vec<Uuid> = rows.iter().map(|(r, _, _)| r.id).collect();
    let mut contacts = contacts::repo::for_relations(&state.db, workspace_id, &found).await?;
    let flags = IndexFlags {
        suppliers: false,
        include_advice: false,
    };
    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        let id = row.0.id;
        let mine = contacts.extract_if(.., |c| c.relation_id == id).collect();
        let v = serde_json::to_value(RelationIndexResource::new(row, mine, flags))
            .map_err(|e| AppError::Internal(e.to_string()))?;
        out.insert(id, v);
    }
    Ok(out)
}

/// Patch a live relation of this workspace, else 404. Row locked first: the VAT and phone
/// rules read its stored addresses (Laravel ValidateVatNumber used the stored ones too).
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    mut p: RelationPatch,
) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let stored = repo::addresses_for_update(&mut tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let relation_type_id = p.relation_type_id;
    if !custom_types::repo::is_live(&mut *tx, workspace_id, relation_type_id, Some("relation"))
        .await?
    {
        return Err(AppError::field("relation_type_id", TYPE_NOT_VALID));
    }
    let stored_country = address::default_of(&stored).and_then(|a| a.country.clone());
    if let Some(Some(vat)) = &p.vat_number
        && stored_country.as_deref().is_some_and(vat_number::in_vat_eu)
        && !vat_number::is_valid_format(vat)
    {
        return Err(AppError::field(
            "vat_number",
            "The Vat number must be a valid number issued by any Member State.",
        ));
    }
    if let Some(Some(n)) = &p.phone_number {
        let country = p
            .addresses
            .as_deref()
            .and_then(phone::country_of)
            .or_else(|| phone::country_of(&stored));
        p.phone_number = Some(phone::normalize(Some(n), country));
    }
    let keep = matches!(&p.supplier_data, Some(Some(s)) if keeps_ftp_password(s));
    if !repo::update(&mut *tx, workspace_id, id, relation_type_id, &p, keep).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// Soft delete, else 404.
// shortcut: no invoices/orders/estimates/work orders yet, so Laravel's "has documents" 422
// can't trigger; the order/invoice batch adds that guard.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Laravel GetRelationFinancialSummary with no documents: every amount is 0, credit comes from
/// the financial account. Key sets match Laravel's lite and full answers.
// shortcut: invoices/payments/loyalty not ported; the invoice batch replaces the zeros.
pub async fn financial_summary(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    lite: bool,
) -> Result<Value, AppError> {
    let id = parse(id)?;
    let (account, created_at) = repo::summary_source(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let credit_limit = account
        .as_ref()
        .and_then(|a| a.get("credit_limit"))
        .and_then(|c| {
            c.as_f64()
                .or_else(|| c.as_str().and_then(|s| s.trim().parse().ok()))
        })
        .filter(|c: &f64| c.is_finite());
    let available = credit_limit.map(|c| (c.max(0.0) * 100.0).round() / 100.0);
    if lite {
        return Ok(json!({
            "relation_id": id, "projected_ltv": null, "total_spend": 0.0,
            "outstanding_amount": 0.0, "outstanding_count": 0, "total_customer_value": 0.0,
            "outstanding_balance": 0.0, "credit_limit": credit_limit, "available_credit": available,
            "account_manager_name": null, "customer_since": null, "last_payment_date": null,
            "loyalty_points": 0,
        }));
    }
    Ok(json!({
        "relation_id": id, "first_send_date": null, "last_send_date": null,
        "months_since_last": null, "orders_count": 0, "years_active": 0.0, "total_spend": 0.0,
        "avg_order_value": 0.0, "purchases_per_year": 0.0,
        // Laravel's base lifespan for a relation without recent orders.
        "expected_lifespan_years": 5, "projected_ltv": 0.0, "remaining_ltv": 0.0,
        "outstanding_amount": 0.0, "outstanding_count": 0, "total_customer_value": 0.0,
        "outstanding_balance": 0.0,
        // shortcut: no set-owner route yet, so no relation has an account manager.
        "account_manager_name": null,
        "customer_since": mysql_datetime(created_at), "last_payment_date": null,
        "credit_limit": credit_limit, "available_credit": available,
    }))
}

/// `Y-m-d H:i:s`, the raw MySQL column form Laravel returned from its summary query.
fn mysql_datetime(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%d %H:%M:%S").to_string()
}
