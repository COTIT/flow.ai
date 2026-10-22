//! Sending a workspace mail (Laravel SendMail): pick the transport and sender, load the
//! attachments, commit a `pending` log row, deliver outside any transaction, then record what
//! happened. Transport, in Laravel's order: Microsoft Graph when outbound sending is enabled
//! and configured; else the profile's SMTP server when it has a host; else the workspace's.

use askama::Template;
use chrono::Datelike;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    mail::{Attachment, Delivered, Failure, Mail, Sender, Via, graph, smtp},
    modules::{
        auth::token::PlainSecret,
        email_profiles::{self, service::password_path},
        files,
        messages::repo::{self, LogRow, NewLog},
        settings::{self, credentials::RE_ENTER},
        users, workspaces,
    },
};

/// 422 without any way to send (Laravel's text).
pub const NO_HOST: &str = "No smtp host specified in the settings";
/// 422 when the workspace's server has no from address.
pub const NO_FROM: &str = "No smtp from address specified in the settings";
/// 503 when the server or service failed (Laravel's text).
pub const FAILED: &str = "Failed to send message.";
/// Attachment totals: SMTP, and Graph (its whole request may be 4 MB).
const SMTP_CAP: usize = 20 * 1024 * 1024;
const GRAPH_CAP: usize = 3 * 1024 * 1024;

const GRAPH_SECRET: &str = "integrations.mail.graph.client_secret";

/// A mail to send.
pub struct Outgoing<'a> {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: &'a str,
    /// The user's HTML; the layout wraps it.
    pub body: &'a str,
    pub attachments: &'a [Uuid],
    pub email_profile_id: Option<Uuid>,
    /// Laravel's message type (`send_invoice`, …): picks a Graph sender.
    pub message_type: Option<&'a str>,
}

/// How a mail leaves, from whom, with which footer.
struct Route {
    via: Via,
    from: Sender,
    footer: String,
    provider: &'static str,
}

/// Send `o` for `user`. Nothing to anyone is a no-op (as Laravel); a blocked mail is ok to
/// the caller and `blocked` in the log.
pub async fn send(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    o: Outgoing<'_>,
) -> Result<(), AppError> {
    if o.to.is_empty() && o.cc.is_empty() && o.bcc.is_empty() {
        return Ok(());
    }
    let rows = attachment_rows(state, workspace_id, o.attachments).await?;
    let route = route(state, workspace_id, o.email_profile_id, o.message_type).await?;
    let cap = if matches!(route.via, Via::Graph(_)) {
        GRAPH_CAP
    } else {
        SMTP_CAP
    };
    let attachments = attachments(state, rows, cap).await?;
    let html = Layout {
        body: o.body,
        footer: &route.footer,
        year: chrono::Utc::now().year(),
    }
    .render()
    .map_err(|e| AppError::Internal(format!("mail layout: {e}")))?;
    let log_id = Uuid::now_v7();
    let mut tx = state.db.begin().await?;
    repo::insert(
        &mut tx,
        &NewLog {
            id: log_id,
            workspace_id,
            user_id,
            provider: route.provider,
            from_email: &route.from.email,
            subject: o.subject,
            to: &o.to,
            cc: &o.cc,
            bcc: &o.bcc,
        },
    )
    .await?;
    tx.commit().await?;
    repo::mark(&state.db, log_id, "sending", None, None).await?;
    let mail = Mail {
        from: route.from,
        to: o.to,
        cc: o.cc,
        bcc: o.bcc,
        subject: o.subject.to_owned(),
        html,
        attachments,
        log_id,
    };
    match state.mailer.deliver(&route.via, &mail).await {
        Ok(Delivered::Accepted { message_id }) => {
            repo::mark(&state.db, log_id, "accepted", message_id.as_deref(), None).await?;
            Ok(())
        }
        Ok(Delivered::Blocked { .. }) => {
            repo::mark(&state.db, log_id, "blocked", None, None).await?;
            Ok(())
        }
        Err(Failure::Refused(why)) => {
            repo::mark(&state.db, log_id, "failed", None, Some(why)).await?;
            Err(AppError::Unprocessable(why.into()))
        }
        Err(Failure::Failed(class)) => {
            tracing::warn!(log = %log_id, error = %class, "workspace mail failed");
            repo::mark(&state.db, log_id, "failed", None, Some(&class)).await?;
            Err(AppError::Unavailable(FAILED))
        }
    }
}

/// The transport and sender for this workspace (and profile).
async fn route(
    state: &AppState,
    workspace_id: Uuid,
    profile_id: Option<Uuid>,
    message_type: Option<&str>,
) -> Result<Route, AppError> {
    let db = &state.db;
    let profile = match profile_id {
        Some(id) => Some(
            email_profiles::repo::sender(db, workspace_id, id)
                .await?
                .ok_or_else(|| {
                    AppError::field(
                        "email_profile_id",
                        "The selected email profile id is invalid.",
                    )
                })?,
        ),
        None => None,
    };
    let integrations = settings::repo::section(db, workspace_id, "integrations").await?;
    let mail = integrations.as_ref().map_or(&Value::Null, |v| &v["mail"]);
    if mail["outbound"]["send_enabled"] == true {
        match graph_account(state, workspace_id, &mail["graph"]).await? {
            Some(account) => {
                let (from, footer) = match &profile {
                    Some(p) => (profile_sender(p), p.html.clone().unwrap_or_default()),
                    None => (
                        graph_sender(mail, message_type, &account.mailbox),
                        String::new(),
                    ),
                };
                return Ok(Route {
                    via: Via::Graph(account),
                    from,
                    footer,
                    provider: "microsoft-graph",
                });
            }
            None => tracing::warn!("outbound Graph enabled but not configured: using SMTP"),
        }
    }
    let profile_server = profile.as_ref().and_then(|p| {
        let host = p.smtp.as_ref()?.get("host")?.as_str()?;
        (!host.is_empty()).then_some(p)
    });
    let (server, from, footer) = match profile_server {
        Some(p) => {
            let id = profile_id.unwrap_or_default();
            let password = open(
                state,
                workspace_id,
                &password_path(id),
                p.key_id.as_deref(),
                p.sealed.as_deref(),
            )?;
            let server = smtp_server(p.smtp.as_ref().unwrap_or(&Value::Null), password)
                .ok_or(AppError::Unprocessable(NO_HOST.into()))?;
            (
                server,
                profile_sender(p),
                p.html.clone().unwrap_or_default(),
            )
        }
        None => {
            let settings = settings::repo::section(db, workspace_id, "smtp")
                .await?
                .unwrap_or(Value::Null);
            let stored = settings::repo::credential(db, workspace_id, "smtp.password").await?;
            let password = open(
                state,
                workspace_id,
                "smtp.password",
                stored.as_ref().map(|s| s.0.as_str()),
                stored.as_ref().map(|s| s.1.as_slice()),
            )?;
            let server =
                smtp_server(&settings, password).ok_or(AppError::Unprocessable(NO_HOST.into()))?;
            let from = match &profile {
                Some(p) => profile_sender(p),
                None => {
                    let email = text(&settings["from_address"])
                        .ok_or(AppError::Unprocessable(NO_FROM.into()))?;
                    Sender {
                        email,
                        name: company_name(state, workspace_id).await?,
                    }
                }
            };
            let footer = profile
                .as_ref()
                .and_then(|p| p.html.clone())
                .unwrap_or_default();
            (server, from, footer)
        }
    };
    Ok(Route {
        via: Via::Smtp(server),
        from,
        footer,
        provider: "smtp",
    })
}

fn profile_sender(p: &email_profiles::repo::SenderRow) -> Sender {
    Sender {
        email: p.email.clone(),
        name: Some(p.sender_name.clone()),
    }
}

/// Graph without a profile: the sender for this message type, else the default sender, else
/// the mailbox (Laravel GraphOutbound::senderFor).
fn graph_sender(mail: &Value, message_type: Option<&str>, mailbox: &str) -> Sender {
    let out = &mail["outbound"];
    let pick = |v: &Value| {
        text(&v["email"]).map(|email| Sender {
            email,
            name: text(&v["sender_name"]),
        })
    };
    message_type
        .and_then(|t| pick(&out["senders"][t]))
        .or_else(|| pick(&out["default_sender"]))
        .unwrap_or_else(|| Sender {
            email: mailbox.to_owned(),
            name: None,
        })
}

/// The Graph account when all four parts are there (the secret stored).
async fn graph_account(
    state: &AppState,
    workspace_id: Uuid,
    g: &Value,
) -> Result<Option<graph::Account>, AppError> {
    let (Some(tenant_id), Some(client_id), Some(mailbox)) = (
        text(&g["tenant_id"]),
        text(&g["client_id"]),
        text(&g["mailbox"]),
    ) else {
        return Ok(None);
    };
    let Some((key_id, sealed)) =
        settings::repo::credential(&state.db, workspace_id, GRAPH_SECRET).await?
    else {
        return Ok(None);
    };
    let client_secret = open(
        state,
        workspace_id,
        GRAPH_SECRET,
        Some(&key_id),
        Some(&sealed),
    )?
    .ok_or(AppError::Unprocessable(RE_ENTER.into()))?;
    Ok(Some(graph::Account {
        tenant_id,
        client_id,
        client_secret,
        mailbox: mailbox.to_lowercase(),
    }))
}

/// A stored secret, opened; None when none is stored. A key change makes it unreadable: 422.
fn open(
    state: &AppState,
    workspace_id: Uuid,
    path: &str,
    key_id: Option<&str>,
    sealed: Option<&[u8]>,
) -> Result<Option<PlainSecret>, AppError> {
    let (Some(key_id), Some(sealed)) = (key_id, sealed) else {
        return Ok(None);
    };
    let key = state
        .credentials
        .as_ref()
        .filter(|k| k.id() == key_id)
        .ok_or(AppError::Unprocessable(RE_ENTER.into()))?;
    key.open(workspace_id, path, sealed).map(Some)
}

/// An SMTP server from `smtp` settings (host required).
fn smtp_server(s: &Value, password: Option<PlainSecret>) -> Option<smtp::Server> {
    let host = text(&s["host"])?;
    let port = match &s["port"] {
        Value::Number(n) => n.as_u64().and_then(|p| u16::try_from(p).ok()),
        Value::String(p) => p.trim().parse().ok(),
        _ => None,
    };
    Some(smtp::Server {
        host: host.to_lowercase(),
        port,
        username: text(&s["username"]),
        password,
        encryption: text(&s["encryption"]).map(|e| e.to_lowercase()),
    })
}

fn text(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

async fn company_name(state: &AppState, workspace_id: Uuid) -> Result<Option<String>, AppError> {
    let record = workspaces::repo::record(&state.db, workspace_id).await?;
    Ok(record.and_then(|r| r.profile.and_then(|p| text(&p["company_name"]))))
}

/// The stored files `ids` of this workspace, in order; 422 for any other id.
async fn attachment_rows(
    state: &AppState,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<files::repo::FileRow>, AppError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut rows = files::repo::find_many(&state.db, workspace_id, ids).await?;
    let mut ordered = Vec::with_capacity(ids.len());
    for (i, id) in ids.iter().enumerate() {
        let Some(at) = rows.iter().position(|r| r.id == *id) else {
            let f = format!("attachments.{i}");
            return Err(AppError::field(&f, format!("The selected {f} is invalid.")));
        };
        ordered.push(rows.swap_remove(at));
    }
    Ok(ordered)
}

/// The bytes of `rows`, at most `cap` together.
async fn attachments(
    state: &AppState,
    rows: Vec<files::repo::FileRow>,
    cap: usize,
) -> Result<Vec<Attachment>, AppError> {
    let total: i64 = rows.iter().map(|r| r.size).sum();
    if usize::try_from(total).unwrap_or(usize::MAX) > cap {
        let msg = format!(
            "The attachments must not be greater than {} kilobytes in total.",
            cap / 1024
        );
        return Err(AppError::field("attachments", msg));
    }
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let bucket = files::service::store(state)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let bytes = bucket
            .get(&row.key)
            .await?
            .ok_or_else(|| AppError::Internal(format!("file {} has no object", row.id)))?;
        out.push(Attachment {
            name: row.document_name,
            content_type: row.content_type,
            bytes,
        });
    }
    Ok(out)
}

/// The caller's address, for the profile connection test.
pub async fn caller_email(state: &AppState, user_id: Uuid) -> Result<String, AppError> {
    users::repo::find_contact(&state.db, user_id)
        .await?
        .map(|c| c.email.to_lowercase())
        .ok_or(AppError::Unauthorized)
}

/// The addressed contact's email (the default recipient), else a 422.
pub async fn contact_email(
    state: &AppState,
    workspace_id: Uuid,
    contact_id: Uuid,
) -> Result<Option<String>, AppError> {
    repo::contact_email(&state.db, workspace_id, contact_id)
        .await?
        .ok_or_else(|| {
            AppError::field(
                "addressed_contact_id",
                "The selected addressed contact id is invalid.",
            )
        })
        .map(|e| e.map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty()))
}

/// The Graph secret for an Office 365 test: the one sent, or the stored one for the mask.
pub async fn graph_secret(
    state: &AppState,
    workspace_id: Uuid,
    sent: &str,
) -> Result<PlainSecret, AppError> {
    if sent != settings::credentials::MASK {
        return Ok(PlainSecret::new(sent.to_owned()));
    }
    let stored = settings::repo::credential(&state.db, workspace_id, GRAPH_SECRET).await?;
    open(
        state,
        workspace_id,
        GRAPH_SECRET,
        stored.as_ref().map(|s| s.0.as_str()),
        stored.as_ref().map(|s| s.1.as_slice()),
    )?
    .ok_or_else(|| AppError::field("graph.client_secret", RE_ENTER))
}

/// One page of the workspace's send logs (Laravel ListEmailSendLogResource).
pub async fn logs(
    state: &AppState,
    workspace_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Value>, i64), AppError> {
    let (rows, total) = repo::list(&state.db, workspace_id, limit, offset).await?;
    Ok((
        rows.into_iter()
            .map(|r| log_resource(workspace_id, r))
            .collect(),
        total,
    ))
}

fn log_resource(workspace_id: Uuid, r: LogRow) -> Value {
    let all: Vec<&str> = [&r.to_recipients, &r.cc_recipients, &r.bcc_recipients]
        .into_iter()
        .flat_map(|v| v.as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    json!({
        "id": r.id,
        "subject": r.subject,
        "status": r.status,
        "provider": r.provider,
        "mailer": workspace_id,
        "recipient_count": r.recipient_count.max(all.len() as i64),
        "opened_count": r.opened_count,
        "to_recipients": r.to_recipients,
        "cc_recipients": r.cc_recipients,
        "bcc_recipients": r.bcc_recipients,
        "recipients_summary": all.join(", "),
        "sent_at": r.sent_at,
        "delivered_at": r.delivered_at,
        "bounced_at": r.bounced_at,
        "opened_at": r.opened_at,
        "created_at": r.created_at,
    })
}

/// The mail layout (Laravel `mailTemplates.default`): the body, the profile's footer and the
/// brand line.
#[derive(Template)]
#[template(path = "mail/default.html")]
struct Layout<'a> {
    body: &'a str,
    footer: &'a str,
    year: i32,
}
