//! Relation rules: create with number + inline default contact in one transaction, detail view.

use serde_json::json;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        contacts, custom_types,
        relations::{
            dto::{Detail, NewRelation, RelationResource, TYPE_NOT_VALID, WorkspaceCtx},
            phone, repo,
        },
        vat_groups, workspaces,
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
        && !custom_types::repo::is_live(&mut *tx, workspace_id, id, None).await?
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
    let number = repo::next_number(&mut tx, workspace_id).await?;
    // Laravel RelationSetting defaults, snapshotted per row like Laravel's relation_appendix.
    let appendix = json!({ "prefix": "RE-", "digits": 4, "suffix": null });
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

/// Detail view of a live relation of this workspace, else 404. Four fixed queries.
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
    Ok(Detail {
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
