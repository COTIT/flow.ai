//! VAT group rules: plain CRUD with soft delete, scoped to one workspace.

use rust_decimal::prelude::ToPrimitive;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::vat_groups::{
        dto::{NewVatGroup, VatGroupPatch},
        repo::{self, ListFilter, VatGroupRow},
    },
};

/// One list page plus the total matching count.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<VatGroupRow>, i64), AppError> {
    let (rows, mut total) = repo::list(&state.db, workspace_id, f).await?;
    if rows.is_empty() && f.offset > 0 {
        total = repo::count(&state.db, workspace_id, f).await?;
    }
    Ok((rows, total))
}

/// One live row of this workspace, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<VatGroupRow, AppError> {
    repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)
}

/// Insert after the module's checks; returns the new id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: NewVatGroup,
) -> Result<Uuid, AppError> {
    Ok(repo::insert(&state.db, workspace_id, &input).await?)
}

/// Patch a live row of this workspace, else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    patch: VatGroupPatch,
) -> Result<(), AppError> {
    found(repo::update(&state.db, workspace_id, parse(id)?, &patch).await?)
}

/// Soft delete, no in-use guard (Laravel parity): products keep pointing at the row.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    found(repo::soft_delete(&state.db, workspace_id, parse(id)?).await?)
}

/// Laravel VatRatesForCountry: the country's groups as `{vat_amount, name, default_group}`,
/// default first then highest rate, plus a 0 % row when the country has none. `vat_amount` is
/// a JSON number (Laravel float cast). The extra row is named in the workspace language.
pub fn rates_for_country(groups: &[VatGroupRow], country: &str, language: Option<&str>) -> Value {
    let mut rows: Vec<&VatGroupRow> = groups
        .iter()
        .filter(|g| g.country.as_deref() == Some(country))
        .collect();
    rows.sort_by(|a, b| {
        b.default_group
            .cmp(&a.default_group)
            .then(b.vat_amount.cmp(&a.vat_amount))
    });
    let has_zero = rows.iter().any(|g| g.vat_amount.is_zero());
    let mut out: Vec<Value> = rows
        .into_iter()
        .map(|g| {
            json!({
                "vat_amount": g.vat_amount.to_f64().unwrap_or_default(),
                "name": g.name,
                "default_group": g.default_group,
            })
        })
        .collect();
    if !has_zero {
        // Laravel AccessToWorkspace locale: first `-` part of profile.language, en unless nl.
        let nl = language.is_some_and(|l| {
            l.split('-')
                .next()
                .is_some_and(|p| p.eq_ignore_ascii_case("nl"))
        });
        let name = if nl {
            "BTW-verlegd"
        } else {
            "VAT reverse charge"
        };
        out.push(json!({ "name": name, "vat_amount": 0, "default_group": false }));
    }
    Value::Array(out)
}

/// Malformed id = unknown id = 404.
fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

fn found(changed: bool) -> Result<(), AppError> {
    changed.then_some(()).ok_or(AppError::NotFound)
}
