//! Contact rules: written only through a live relation of the workspace, phones normalized
//! with the relation's country, one default contact per relation.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        contacts::{
            dto::{ContactFields, ContactPatch},
            repo as contacts_repo,
        },
        relations::{self, phone, service::parse},
    },
};

/// New contact of a live relation; returns its id. Phones normalized with the relation's
/// country; a new default demotes the others (one default per relation).
pub async fn create_contact(
    state: &AppState,
    workspace_id: Uuid,
    relation: &str,
    mut c: ContactFields,
) -> Result<Uuid, AppError> {
    let relation = parse(relation)?;
    let mut tx = state.db.begin().await?;
    let stored = relations::repo::addresses_for_update(&mut tx, workspace_id, relation)
        .await?
        .ok_or(AppError::NotFound)?;
    let country = phone::country_of(&stored);
    c.phone_number = phone::normalize(c.phone_number.as_deref(), country);
    phone::normalize_phones(&mut c.phones, country);
    if c.default_contact {
        contacts_repo::demote_others(&mut *tx, workspace_id, relation, None).await?;
    }
    let id = contacts_repo::insert(&mut *tx, workspace_id, relation, &c).await?;
    tx.commit().await?;
    Ok(id)
}

/// Patch a contact of this relation (404 when it belongs to another one).
pub async fn update_contact(
    state: &AppState,
    workspace_id: Uuid,
    relation: &str,
    contact: &str,
    mut p: ContactPatch,
) -> Result<(), AppError> {
    let (relation, contact) = (parse(relation)?, parse(contact)?);
    let mut tx = state.db.begin().await?;
    let stored = relations::repo::addresses_for_update(&mut tx, workspace_id, relation)
        .await?
        .ok_or(AppError::NotFound)?;
    let country = phone::country_of(&stored);
    if let Some(Some(n)) = &p.phone_number {
        p.phone_number = Some(phone::normalize(Some(n), country));
    }
    if let Some(phones) = &mut p.phones {
        phone::normalize_phones(phones, country);
    }
    if p.default_contact == Some(true) {
        contacts_repo::demote_others(&mut *tx, workspace_id, relation, Some(contact)).await?;
    }
    if !contacts_repo::update(&mut *tx, workspace_id, relation, contact, &p).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// Soft delete a contact of this relation, else 404.
pub async fn delete_contact(
    state: &AppState,
    workspace_id: Uuid,
    relation: &str,
    contact: &str,
) -> Result<(), AppError> {
    let (relation, contact) = (parse(relation)?, parse(contact)?);
    contacts_repo::soft_delete(&state.db, workspace_id, relation, contact)
        .await?
        .then_some(())
        .ok_or(AppError::NotFound)
}
