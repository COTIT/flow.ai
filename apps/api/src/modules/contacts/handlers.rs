//! HTTP edge for /v1/workspace/{workspace}/relations/{relation}/contacts: `contact.*`
//! permission (via `Can`), call service, wrap.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        json::ValidatedJson,
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    modules::contacts::{
        dto::{ContactCreated, ContactInput, CreateContactRequest, UpdateContactRequest},
        service,
    },
};

permissions! {
    ContactCreate => "contact.create",
    ContactUpdate => "contact.update",
    ContactDelete => "contact.delete",
}

/// POST /relations/{relation}/contacts -> 201 `{data:{contact_id}}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/relations/{relation}/contacts", tag = "contacts",
    operation_id = "contacts_create", summary = "Add a contact to a relation",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("relation" = Uuid, Path, description = "Relation id"),
    ),
    request_body = ContactInput,
    responses(
        (status = 201, body = Envelope<ContactCreated>, description = "Created; returns the new id"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `contact.create`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or relation, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    access: Can<ContactCreate>,
    Path((_, relation)): Path<(String, String)>,
    ValidatedJson(input): ValidatedJson<CreateContactRequest>,
) -> Result<(StatusCode, Data<ContactCreated>), AppError> {
    let contact_id = service::create_contact(&state, access.workspace_id, &relation, input).await?;
    Ok((StatusCode::CREATED, Data(ContactCreated { contact_id })))
}

/// POST /relations/{relation}/contacts/{contact} -> `{"message":"ok"}`.
#[utoipa::path(
    post, path = "/workspace/{workspace}/relations/{relation}/contacts/{contact}", tag = "contacts",
    operation_id = "contacts_update", summary = "Update a contact",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("relation" = Uuid, Path, description = "Relation id"),
        ("contact" = Uuid, Path, description = "Contact id"),
    ),
    request_body = UpdateContactRequest,
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `contact.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, relation or contact, or caller not a member"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    access: Can<ContactUpdate>,
    Path((_, relation, contact)): Path<(String, String, String)>,
    ValidatedJson(patch): ValidatedJson<UpdateContactRequest>,
) -> Result<Message, AppError> {
    service::update_contact(&state, access.workspace_id, &relation, &contact, patch).await?;
    Ok(Message("ok"))
}

/// DELETE /relations/{relation}/contacts/{contact} -> `{"message":"ok"}`.
#[utoipa::path(
    delete, path = "/workspace/{workspace}/relations/{relation}/contacts/{contact}", tag = "contacts",
    operation_id = "contacts_delete", summary = "Delete a contact",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("relation" = Uuid, Path, description = "Relation id"),
        ("contact" = Uuid, Path, description = "Contact id"),
    ),
    responses(
        (status = 200, body = MessageBody, description = "Done (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `contact.delete`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, relation or contact, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    access: Can<ContactDelete>,
    Path((_, relation, contact)): Path<(String, String, String)>,
) -> Result<Message, AppError> {
    service::delete_contact(&state, access.workspace_id, &relation, &contact).await?;
    Ok(Message("ok"))
}
