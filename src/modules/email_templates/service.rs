//! Email templates: CRUD under one default per domain, and the variables a domain offers.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        email_templates::{
            dto::TemplateInput,
            repo::{self, ListFilter, TemplateRow, Write},
        },
        pdf_templates::service::{UNKNOWN_CUSTOM_TYPE, conditions_json, custom_type_unknown},
    },
};

/// New template; its id. A default one clears the domain's other default.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: TemplateInput,
) -> Result<Uuid, AppError> {
    check_conditions(state, workspace_id, &input).await?;
    let domain = input
        .template_domain
        .as_deref()
        .ok_or_else(|| AppError::Internal("validated create without domain".into()))?;
    let conditions = conditions_json(&input.conditions)?;
    let id = Uuid::now_v7();
    let mut tx = state.db.begin().await?;
    if input.default_template == Some(true) {
        repo::lock_domain(&mut tx, workspace_id, domain).await?;
        repo::clear_defaults(&mut tx, workspace_id, domain, id).await?;
    }
    repo::insert(
        &mut tx,
        workspace_id,
        id,
        domain,
        &write(&input, conditions.as_ref()),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// Patch a template (its domain stays), else 404. Setting the default clears the domain's
/// other default too (Laravel did so only on create).
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    input: TemplateInput,
) -> Result<(), AppError> {
    let id = parse(id)?;
    let row = repo::find(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    check_conditions(state, workspace_id, &input).await?;
    let conditions = conditions_json(&input.conditions)?;
    let mut tx = state.db.begin().await?;
    if input.default_template == Some(true) {
        repo::lock_domain(&mut tx, workspace_id, &row.template_domain).await?;
        repo::clear_defaults(&mut tx, workspace_id, &row.template_domain, id).await?;
    }
    if !repo::update(
        &mut tx,
        workspace_id,
        id,
        &write(&input, conditions.as_ref()),
    )
    .await?
    {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// Soft delete, else 404.
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    if repo::soft_delete(&state.db, workspace_id, parse(id)?).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// One live template, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<Value, AppError> {
    let row = repo::find(&state.db, workspace_id, parse(id)?)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(render(row))
}

/// One page of templates plus the total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    filter: &ListFilter<'_>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Value>, i64), AppError> {
    let (rows, total) = repo::list(&state.db, workspace_id, filter, limit, offset).await?;
    Ok((rows.into_iter().map(render).collect(), total))
}

async fn check_conditions(
    state: &AppState,
    workspace_id: Uuid,
    input: &TemplateInput,
) -> Result<(), AppError> {
    if custom_type_unknown(state, workspace_id, &input.conditions).await? {
        return Err(AppError::Validation(BTreeMap::from([(
            "conditions.relation_custom_type_id".to_owned(),
            vec![UNKNOWN_CUSTOM_TYPE.to_owned()],
        )])));
    }
    Ok(())
}

fn write<'a>(i: &'a TemplateInput, conditions: Option<&'a Value>) -> Write<'a> {
    Write {
        template_name: i.template_name.as_deref(),
        default_template: i.default_template,
        subject: i.subject.as_deref(),
        body: &i.body,
        set_conditions: i.conditions.is_some(),
        conditions,
        set_meta: i.meta.is_some(),
        meta: i.meta.as_ref().and_then(Option::as_ref),
    }
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Laravel EmailTemplateResource.
fn render(r: TemplateRow) -> Value {
    json!({
        "id": r.id,
        "template_name": r.template_name,
        "template_domain": r.template_domain,
        "default_template": r.default_template,
        "subject": r.subject,
        "body": r.body,
        "conditions": r.conditions,
        "created_at": r.created_at,
        "last_updated_at": r.updated_at,
        "meta": r.meta,
    })
}
