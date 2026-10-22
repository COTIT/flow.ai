//! PDF templates: CRUD, the fields a domain offers, and a preview rendered with the
//! workspace's newest invoice (stored as a file, as Laravel did).

use std::collections::BTreeMap;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::{Rules, human},
    modules::{
        files::{self, dto::FileResource, service::NewFile},
        invoices,
        pdf_templates::{
            dto::{Conditions, TemplateInput, check_column_fields},
            repo::{self, ListFilter, TemplateRow, Write},
        },
    },
};

/// 422 text when there is no invoice to preview with.
const NO_SAMPLE: &str = "Create an invoice to preview this template.";

/// New template; its id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    input: TemplateInput,
) -> Result<Uuid, AppError> {
    check_refs(state, workspace_id, &input).await?;
    let domain = input
        .domain
        .as_deref()
        .ok_or_else(|| AppError::Internal("validated create without domain".into()))?;
    let conditions = conditions_json(&input.conditions)?;
    let w = write(&input, conditions.as_ref());
    Ok(repo::insert(&state.db, workspace_id, domain, &w).await?)
}

/// Patch a template (its domain stays), else 404.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    input: TemplateInput,
) -> Result<(), AppError> {
    let id = parse(id)?;
    let row = repo::find(&state.db, workspace_id, id, None)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(cols) = &input.columns {
        let mut r = Rules::default();
        check_column_fields(&mut r, &row.domain, cols);
        r.into_result()?;
    }
    check_refs(state, workspace_id, &input).await?;
    let conditions = conditions_json(&input.conditions)?;
    let w = write(&input, conditions.as_ref());
    repo::update(&state.db, workspace_id, id, &w)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// Soft delete, else 404. Its logo and background files stay (Laravel).
pub async fn delete(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    repo::soft_delete(&state.db, workspace_id, parse(id)?)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}

/// One template in resource shape, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<Value, AppError> {
    let row = repo::find(&state.db, workspace_id, parse(id)?, None)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(render(state, workspace_id, vec![row]).await?.remove(0))
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
    Ok((render(state, workspace_id, rows).await?, total))
}

/// Render `id` with the workspace's newest invoice and store the PDF; its file.
pub async fn preview(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
) -> Result<FileResource, AppError> {
    let row = repo::find(&state.db, workspace_id, parse(id)?, None)
        .await?
        .ok_or(AppError::NotFound)?;
    files::service::store(state)?;
    let invoice = invoices::repo::newest(&state.db, workspace_id)
        .await?
        .ok_or_else(|| AppError::Unprocessable(NO_SAMPLE.into()))?;
    let pdf = invoices::pdf::render(state, workspace_id, invoice, Some(&row)).await?;
    let file_id = files::service::create(
        state,
        workspace_id,
        user_id,
        NewFile {
            document_name: &format!(
                "test-preview-pdf-template-{}.pdf",
                chrono::Utc::now().timestamp()
            ),
            description: None,
            meta: None,
            bytes: pdf.bytes,
        },
    )
    .await?;
    files::service::get(state, workspace_id, &file_id.to_string()).await
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

/// Logo and background must be stored files of this workspace; a custom relation type in the
/// conditions must be a live one.
/// Largest logo or background image.
const MAX_IMAGE_BYTES: i64 = 2 * 1024 * 1024;

async fn check_refs(
    state: &AppState,
    workspace_id: Uuid,
    input: &TemplateInput,
) -> Result<(), AppError> {
    let mut errors = BTreeMap::new();
    for (field, sent) in [
        ("background_file_id", input.background_file_id),
        ("logo_file_id", input.logo_file_id),
    ] {
        let Some(Some(id)) = sent else {
            continue;
        };
        let file = match id {
            Some(id) => files::repo::find(&state.db, workspace_id, id).await?,
            None => None,
        };
        // Inlined as base64 into every page, so bounded well under the sidecar's 20 MB body.
        let msg = match file {
            None => format!("The selected {} is invalid.", human(field)),
            Some(f) if !f.content_type.starts_with("image/") => {
                format!("The {} must be an image.", human(field))
            }
            Some(f) if f.size > MAX_IMAGE_BYTES => format!(
                "The {} must not be greater than {} kilobytes.",
                human(field),
                MAX_IMAGE_BYTES / 1024
            ),
            Some(_) => continue,
        };
        errors.insert(field.to_owned(), vec![msg]);
    }
    if custom_type_unknown(state, workspace_id, &input.conditions).await? {
        errors.insert(
            "conditions.relation_custom_type_id".into(),
            vec![UNKNOWN_CUSTOM_TYPE.into()],
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

/// 422 text for a custom relation type this workspace doesn't have.
pub const UNKNOWN_CUSTOM_TYPE: &str = "The selected conditions.relation custom type id is invalid.";

/// True when the conditions name a custom relation type this workspace doesn't have.
pub async fn custom_type_unknown(
    state: &AppState,
    workspace_id: Uuid,
    c: &Option<Option<Conditions>>,
) -> Result<bool, AppError> {
    match c {
        Some(Some(Conditions {
            relation_custom_type_id: Some(t),
            ..
        })) => Ok(!repo::relation_type_exists(&state.db, workspace_id, *t).await?),
        _ => Ok(false),
    }
}

/// Conditions as stored (jsonb), when set.
pub fn conditions_json(c: &Option<Option<Conditions>>) -> Result<Option<Value>, AppError> {
    c.as_ref()
        .and_then(Option::as_ref)
        .map(|c| serde_json::to_value(c).map_err(|e| AppError::Internal(e.to_string())))
        .transpose()
}

fn write<'a>(i: &'a TemplateInput, conditions: Option<&'a Value>) -> Write<'a> {
    let text = |p: &'a Option<Option<String>>| (p.is_some(), p.as_ref().and_then(Option::as_deref));
    let domain_action = i.domain_action.as_ref().and_then(Option::as_deref);
    let (set_header_left, header_left) = text(&i.header_left);
    let (set_header_right, header_right) = text(&i.header_right);
    let (set_header_top, header_top) = text(&i.header_top);
    let (set_footer, footer) = text(&i.footer);
    Write {
        name: i.name.as_deref(),
        domain_action,
        columns: i.columns.as_ref(),
        set_header_left,
        header_left,
        set_header_right,
        header_right,
        set_header_top,
        header_top,
        set_footer,
        footer,
        padding_top: i.padding_top,
        padding_bottom: i.padding_bottom,
        padding_x: i.padding_x,
        enable_summary: i.enable_summary,
        is_default: i.is_default,
        set_conditions: i.conditions.is_some(),
        conditions,
        set_extra_fields: i.extra_fields.is_some(),
        extra_fields: i.extra_fields.as_ref().and_then(Option::as_ref),
        set_meta: i.meta.is_some(),
        meta: i.meta.as_ref().and_then(Option::as_ref),
        set_background: i.background_file_id.is_some(),
        background_file_id: i.background_file_id.flatten().flatten(),
        set_logo: i.logo_file_id.is_some(),
        logo_file_id: i.logo_file_id.flatten().flatten(),
    }
}

/// Laravel PdfTemplateResource; files embedded as FileResource (one query for all rows).
async fn render(
    state: &AppState,
    workspace_id: Uuid,
    rows: Vec<TemplateRow>,
) -> Result<Vec<Value>, AppError> {
    let ids: Vec<Uuid> = rows
        .iter()
        .flat_map(|r| [r.background_file_id, r.logo_file_id])
        .flatten()
        .collect();
    let files = files::service::values(state, workspace_id, &ids).await?;
    let file = |id: Option<Uuid>| {
        id.and_then(|id| files.get(&id).cloned())
            .unwrap_or(Value::Null)
    };
    Ok(rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "name": r.name,
                "domain": r.domain,
                "domain_action": r.domain_action,
                "columns": r.columns,
                "padding_top": r.padding_top,
                "padding_bottom": r.padding_bottom,
                "padding_x": r.padding_x,
                // Laravel left the column uncast: 1/0.
                "enable_summary": i32::from(r.enable_summary),
                "header_left": r.header_left,
                "header_right": r.header_right,
                "header_top": r.header_top,
                "footer": r.footer,
                "default": r.is_default,
                "conditions": r.conditions,
                "extra_fields": r.extra_fields,
                "created_at": r.created_at,
                "last_updated_at": r.updated_at,
                "meta": r.meta,
                "relationships": {
                    "backgroundFile": file(r.background_file_id),
                    "logoFile": file(r.logo_file_id),
                },
            })
        })
        .collect())
}
