//! HTTP edge for workspace mail: `/send-message`, the send logs and the connection tests.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::{AppError, UnprocessableBody},
    http::{
        json::ValidatedJson,
        list::{Page, Paginated},
        response::{Data, Envelope, Message, MessageBody},
        workspace::{Can, permissions},
    },
    mail::{Delivered, Failure, Mail, Sender, Via, graph},
    modules::{
        email_profiles,
        messages::{
            dto::{
                EmailLog, Office365Failure, Office365Reply, Office365Request, ProfileTestResult,
                SendMessageRequest,
            },
            service::{self, Outgoing},
        },
    },
};

permissions! {
    SendMessage => "workspace.send-message",
    SettingGet => "setting.get",
    SettingUpdate => "setting.update",
    ProfileUpdate => "emailprofile.update",
}

/// Paging query.
#[derive(serde::Deserialize, Default, utoipa::IntoParams)]
#[serde(default)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    page_size: Option<String>,
}

/// POST /send-message -> `{message:"ok"}` (also when the allow-list blocked it, as Laravel).
/// Without `to`, the addressed contact gets it (Laravel failed on a missing `to`).
#[utoipa::path(
    post, path = "/workspace/{workspace}/send-message", tag = "messages",
    operation_id = "messages_send", summary = "Send an email (or log a manual send)",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = SendMessageRequest,
    responses(
        (status = 200, body = MessageBody, description = "Sent, logged or blocked by the allow-list (`ok`)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `workspace.send-message`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "Invalid input, unknown contact, attachment or profile, or no usable mail server"),
    ),
    security(("bearer" = []))
)]
pub async fn send_message(
    State(state): State<AppState>,
    access: Can<SendMessage>,
    ValidatedJson(input): ValidatedJson<SendMessageRequest>,
) -> Result<Message, AppError> {
    let contact =
        service::contact_email(&state, access.workspace_id, input.addressed_contact_id).await?;
    if input.manual {
        return Ok(Message("ok"));
    }
    let to = if input.to.is_empty() {
        contact.into_iter().collect()
    } else {
        input.to
    };
    let outgoing = Outgoing {
        to,
        cc: input.cc,
        bcc: input.bcc,
        subject: &input.subject,
        body: &input.body,
        attachments: &input.attachments,
        email_profile_id: input.email_profile_id,
        message_type: None,
        entity: None,
        button: None,
    };
    service::send(&state, access.workspace_id, access.user_id, outgoing).await?;
    Ok(Message("ok"))
}

/// GET /settings/email-logs: the workspace's sends, newest first (`setting.get`, the
/// catalog's name; Laravel checked `setting.view`, which no team could grant).
#[utoipa::path(
    get, path = "/workspace/{workspace}/settings/email-logs", tag = "messages",
    operation_id = "messages_logs", summary = "List sent emails, newest first",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
    responses(
        (status = 200, body = Paginated<EmailLog>, description = "One page of sends"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `setting.get`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
    ),
    security(("bearer" = []))
)]
pub async fn logs(
    State(state): State<AppState>,
    access: Can<SettingGet>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Paginated<Value>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let (rows, total) =
        service::logs(&state, access.workspace_id, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(rows, total, page)))
}

/// GET /email-profiles/{id}/test-connection: a test mail to the caller through the profile
/// (`emailprofile.update`; Laravel checked `ftpserver.view`).
#[utoipa::path(
    get, path = "/workspace/{workspace}/email-profiles/{profile}/test-connection", tag = "messages",
    operation_id = "messages_profile_test", summary = "Send a test email to the caller through a profile",
    params(("workspace" = Uuid, Path, description = "Workspace id"), ("profile" = Uuid, Path, description = "Email profile id")),
    responses(
        (status = 200, body = Envelope<ProfileTestResult>, description = "The test email went out"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `emailprofile.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace or profile, or caller not a member"),
        (status = 422, body = UnprocessableBody, description = "The profile has no usable mail server, or the server refused the mail"),
    ),
    security(("bearer" = []))
)]
pub async fn profile_test(
    State(state): State<AppState>,
    access: Can<ProfileUpdate>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<Value>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    email_profiles::repo::find(&state.db, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let to = service::caller_email(&state, access.user_id).await?;
    let text = "Test email profile connection";
    let outgoing = Outgoing {
        to: vec![to],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: text,
        body: text,
        attachments: &[],
        email_profile_id: Some(id),
        message_type: None,
        entity: None,
        button: None,
    };
    service::send(&state, access.workspace_id, access.user_id, outgoing).await?;
    Ok(Data(json!({ "success_connection": true })))
}

/// A test answer: `{data:{ok, stage, message, …}}`, 200 or 422.
fn answer(ok: bool, stage: &str, message: String, extra: Value) -> (StatusCode, Json<Value>) {
    let mut data = json!({ "ok": ok, "stage": stage, "message": message });
    if let (Some(d), Value::Object(e)) = (data.as_object_mut(), extra) {
        d.extend(e);
    }
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    };
    (status, Json(json!({ "data": data })))
}

async fn account(
    state: &AppState,
    workspace_id: Uuid,
    input: &crate::modules::messages::dto::Office365Input,
) -> Result<graph::Account, AppError> {
    Ok(graph::Account {
        tenant_id: input.tenant_id.clone(),
        client_id: input.client_id.clone(),
        client_secret: service::graph_secret(state, workspace_id, &input.client_secret).await?,
        mailbox: input.mailbox.clone(),
    })
}

/// POST /settings/office365/test-connection: a token, then the mailbox's inbox.
#[utoipa::path(
    post, path = "/workspace/{workspace}/settings/office365/test-connection", tag = "messages",
    operation_id = "messages_office365_connection", summary = "Test Microsoft Graph credentials and mailbox access",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = Office365Request,
    responses(
        (status = 200, body = Office365Reply, description = "Token and mailbox check worked (`data.ok` true)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `setting.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = inline(Office365Failure), description = "Invalid input, stored secret to re-enter, or the test failed"),
    ),
    security(("bearer" = []))
)]
pub async fn office365_connection(
    State(state): State<AppState>,
    access: Can<SettingUpdate>,
    ValidatedJson(input): ValidatedJson<Office365Request>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let account = account(&state, access.workspace_id, &input).await?;
    let graph = state.mailer.graph();
    if graph.check_token(&account).await.is_err() {
        return Ok(answer(
            false,
            "token",
            graph::TOKEN_FAILED.into(),
            json!({}),
        ));
    }
    Ok(match graph.inbox_probe(&account).await {
        Ok(n) => answer(
            true,
            "mailbox",
            "Connection successful. Mailbox is reachable and the Mail.ReadWrite permission is granted.".into(),
            json!({ "messages_with_attachments": n }),
        ),
        Err(e) => {
            let reason = match e {
                Failure::Refused(why) => why.to_owned(),
                Failure::Failed(why) => why,
                Failure::TimedOut => "timed out".to_owned(),
            };
            answer(
                false,
                "mailbox",
                "Authenticated, but could not read the mailbox. Ensure admin consent for the Mail.ReadWrite application permission, that the mailbox address is an existing Exchange Online mailbox, and that any Application Access Policy allows it.".into(),
                json!({ "reason": reason }),
            )
        }
    })
}

/// POST /settings/office365/test-send: Laravel's test mail from `from` (else the mailbox) to
/// `to`, through the allow-list like every mail.
#[utoipa::path(
    post, path = "/workspace/{workspace}/settings/office365/test-send", tag = "messages",
    operation_id = "messages_office365_send", summary = "Send a test email through Microsoft Graph",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = Office365Request,
    responses(
        (status = 200, body = Office365Reply, description = "Test email worked (`data.ok` true)"),
        (status = 403, body = MessageBody, description = "Inactive membership or missing `setting.update`"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller not a member"),
        (status = 422, body = inline(Office365Failure), description = "Invalid input, stored secret to re-enter, or the test failed"),
    ),
    security(("bearer" = []))
)]
pub async fn office365_send(
    State(state): State<AppState>,
    access: Can<SettingUpdate>,
    ValidatedJson(input): ValidatedJson<Office365Request>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let to = input
        .to
        .clone()
        .ok_or_else(|| AppError::field("to", "The to field is required."))?;
    let from = input.from.clone().unwrap_or_else(|| input.mailbox.clone());
    let account = account(&state, access.workspace_id, &input).await?;
    if state.mailer.graph().check_token(&account).await.is_err() {
        return Ok(answer(
            false,
            "token",
            graph::TOKEN_FAILED.into(),
            json!({}),
        ));
    }
    let mail = Mail {
        from: Sender {
            email: from.clone(),
            name: None,
        },
        to: vec![to.clone()],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: "Vimdesk test e-mail".into(),
        html: "<p>Dit is een test e-mail verzonden via Microsoft Graph vanuit Vimdesk.</p>".into(),
        attachments: Vec::new(),
        log_id: Uuid::now_v7(),
    };
    Ok(
        match state.mailer.deliver(&Via::Graph(account), &mail).await {
            Ok(Delivered::Accepted { .. }) => answer(
                true,
                "send",
                format!(
                    "Test e-mail sent to {to} from {from}. Check the inbox and the sender mailbox Sent Items."
                ),
                json!({}),
            ),
            Ok(Delivered::Blocked { .. }) => answer(
                false,
                "send",
                "This server may not send to that address's domain.".into(),
                json!({}),
            ),
            Err(_) => answer(
                false,
                "send",
                format!(
                    "Authenticated, but sending failed. Ensure admin consent for the Mail.Send application permission and that {from} is a valid mailbox the app may send as."
                ),
                json!({}),
            ),
        },
    )
}
