//! HTTP edge for /v1/workspace/{workspace}/relations/{relation}/contacts: `contact.*`
//! permission (via `Can`), call service, wrap.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::AppError,
    http::{
        json::ValidatedJson,
        response::{Data, Message},
        workspace::{Can, permissions},
    },
    modules::contacts::{
        dto::{ContactCreated, CreateContactRequest, UpdateContactRequest},
        service,
    },
};

permissions! {
    ContactCreate => "contact.create",
    ContactUpdate => "contact.update",
    ContactDelete => "contact.delete",
}

/// POST /relations/{relation}/contacts -> 201 `{data:{contact_id}}`.
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
pub async fn delete(
    State(state): State<AppState>,
    access: Can<ContactDelete>,
    Path((_, relation, contact)): Path<(String, String, String)>,
) -> Result<Message, AppError> {
    service::delete_contact(&state, access.workspace_id, &relation, &contact).await?;
    Ok(Message("ok"))
}
