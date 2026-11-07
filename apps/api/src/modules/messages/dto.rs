//! `POST /send-message` (Laravel SendMessageRequest) and the Office 365 test bodies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::{
        pricing::body::{Capped, Scalar},
        relations::address::{self, Address},
    },
};

/// What transport actually established; accepted never proves inbox delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SendStatus {
    Accepted,
    Blocked,
    Manual,
    NoRecipients,
}

/// Bounded evidence; contains no recipients, message body or provider secrets.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct SendReceipt {
    pub status: SendStatus,
    pub send_log_id: Option<Uuid>,
}

/// Existing success message plus truthful transport receipt.
#[derive(Serialize, ToSchema)]
pub struct SendMessageReply {
    pub message: &'static str,
    pub delivery: SendReceipt,
}

/// `POST /send-message`; unknown keys are skipped.
#[derive(Deserialize, Default, ToSchema)]
#[schema(
    description = "A message to send. With `send_method` `manual` nothing is sent; without `to` the addressed contact gets it."
)]
pub struct SendMessageRequest {
    #[schema(value_type = Uuid, required = true)]
    #[serde(default)]
    addressed_contact_id: Scalar,
    /// Billing address (address object) a sent document takes.
    #[schema(value_type = Object, required = true)]
    #[serde(default)]
    address: Capped,
    #[schema(value_type = Option<Vec<String>>)]
    #[serde(default)]
    to: Capped,
    #[schema(value_type = Option<Vec<String>>)]
    #[serde(default)]
    cc: Capped,
    #[schema(value_type = Option<Vec<String>>)]
    #[serde(default)]
    bcc: Capped,
    /// Required for `email`.
    #[schema(value_type = Option<String>, max_length = 100)]
    #[serde(default)]
    subject: Scalar,
    /// HTML; required for `email`.
    #[schema(value_type = Option<String>)]
    #[serde(default)]
    body: Scalar,
    /// `email` or `manual`.
    #[schema(value_type = String, required = true)]
    #[serde(default)]
    send_method: Scalar,
    /// File ids of this workspace.
    #[schema(value_type = Option<Vec<Uuid>>)]
    #[serde(default)]
    attachments: Capped,
    #[schema(value_type = Option<Uuid>)]
    #[serde(default)]
    email_profile_id: Scalar,
    /// The PDF template a sent document renders with; ignored by `/send-message`.
    #[schema(value_type = Option<Uuid>)]
    #[serde(default)]
    pdf_template_id: Scalar,
}

// Doc-only: one send log as `service::log_resource` prints it with `json!`.
/// One sent message.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct EmailLog {
    id: Uuid,
    subject: String,
    status: String,
    provider: String,
    /// The workspace id.
    mailer: Uuid,
    recipient_count: i64,
    opened_count: i64,
    #[schema(value_type = Vec<String>)]
    to_recipients: Value,
    #[schema(value_type = Vec<String>)]
    cc_recipients: Value,
    #[schema(value_type = Vec<String>)]
    bcc_recipients: Value,
    /// Every recipient, comma separated.
    recipients_summary: String,
    #[schema(required = true)]
    sent_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    delivered_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    bounced_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    opened_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

// Doc-only: profile test-connection reply built with `json!`.
/// Profile connection test result.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct ProfileTestResult {
    /// True only when transport accepted the test mail.
    success_connection: bool,
    delivery: SendReceipt,
}

// Doc-only: `data` of an Office 365 test reply built with `json!`.
/// Office 365 test result.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct Office365Answer {
    ok: bool,
    delivery: Option<SendReceipt>,
    /// Step reached: `token`, `mailbox` or `send`.
    stage: String,
    message: String,
    /// test-connection success only.
    messages_with_attachments: Option<i64>,
    /// test-connection mailbox failure only.
    reason: Option<String>,
}

/// Office 365 test reply: `{data: {ok, stage, message, ...}}`.
// Doc-only, like `Office365Answer`.
#[allow(dead_code)]
#[derive(ToSchema)]
pub struct Office365Reply {
    data: Office365Answer,
}

/// 422 of the Office 365 tests: invalid input, a refused credential, or a failed test.
pub struct Office365Failure;

impl utoipa::PartialSchema for Office365Failure {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::{Ref, schema::AnyOfBuilder};
        AnyOfBuilder::new()
            .item(Ref::from_schema_name("ValidationErrorBody"))
            .item(Ref::from_schema_name("MessageBody"))
            .item(Ref::from_schema_name("Office365Reply"))
            .description(Some(
                "Invalid input, or the test failed (`data.ok` false, `data.stage` says where)",
            ))
            .into()
    }
}

impl ToSchema for Office365Failure {}

/// A validated send. `template_id` was stored by Laravel, never used.
#[derive(Clone, Serialize, Deserialize)]
pub struct SendInput {
    pub addressed_contact_id: Uuid,
    /// The billing address a sent document takes.
    pub address: Option<Address>,
    /// `manual`: the user sends it elsewhere; nothing leaves here.
    pub manual: bool,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<Uuid>,
    pub email_profile_id: Option<Uuid>,
    /// The PDF template a sent document renders with (ignored by `/send-message`).
    pub pdf_template_id: Option<Uuid>,
}

impl Validate for SendMessageRequest {
    type Valid = SendInput;

    fn validate(self, r: &mut Rules) -> Option<SendInput> {
        let addressed_contact_id = match self.addressed_contact_id.filled() {
            None => {
                r.missing("addressed_contact_id");
                Uuid::nil()
            }
            Some(v) => uuid(
                r,
                "addressed_contact_id",
                &v,
                "The addressed contact id must be a valid UUID.",
            ),
        };
        let address = match &self.address {
            Capped::Absent | Capped::Value(Value::Null) => {
                r.missing("address");
                None
            }
            Capped::TooLarge => {
                r.add("address", "The address is too large.");
                None
            }
            Capped::Value(v) => address::validate_one(r, "address", v),
        };
        let manual = match self.send_method.filled() {
            None => {
                r.missing("send_method");
                false
            }
            Some(v) => {
                let sent = v.as_str().map(str::trim).unwrap_or_default().to_owned();
                match sent.to_lowercase().as_str() {
                    "email" => false,
                    "manual" => true,
                    _ => {
                        r.add(
                            "send_method",
                            format!("The selected send method: {sent} is invalid."),
                        );
                        false
                    }
                }
            }
        };
        let required = "field is required when send method is email.";
        let mut text = |field: &str, v: &Scalar, max: Option<usize>| -> String {
            if manual {
                return String::new();
            }
            match v.filled() {
                None => {
                    r.add(field, format!("The {} {required}", human(field)));
                    String::new()
                }
                Some(Value::String(s)) => {
                    if let Some(max) = max {
                        r.max_chars(field, &s, max);
                    }
                    s
                }
                Some(_) => {
                    r.add(field, format!("The {} must be a string.", human(field)));
                    String::new()
                }
            }
        };
        let subject = text("subject", &self.subject, Some(100)).trim().to_owned();
        let body = text("body", &self.body, None);
        let to = emails(r, "to", &self.to);
        let cc = emails(r, "cc", &self.cc);
        let bcc = emails(r, "bcc", &self.bcc);
        let attachments = match &self.attachments {
            Capped::Absent | Capped::Value(Value::Null) => Vec::new(),
            Capped::Value(Value::Array(list)) => list
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let f = format!("attachments.{i}");
                    uuid(r, &f, v, &format!("The selected {f} is invalid."))
                })
                .collect(),
            _ => {
                r.add("attachments", "The attachments must be an array.");
                Vec::new()
            }
        };
        let email_profile_id = self.email_profile_id.filled().map(|v| {
            uuid(
                r,
                "email_profile_id",
                &v,
                "The selected email profile id is invalid.",
            )
        });
        let pdf_template_id = self.pdf_template_id.filled().map(|v| {
            uuid(
                r,
                "pdf_template_id",
                &v,
                "The selected pdf template id is invalid.",
            )
        });
        Some(SendInput {
            addressed_contact_id,
            address,
            manual,
            to,
            cc,
            bcc,
            subject,
            body,
            attachments,
            email_profile_id,
            pdf_template_id,
        })
    }
}

fn uuid(r: &mut Rules, field: &str, v: &Value, message: &str) -> Uuid {
    match v.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok()) {
        Some(id) => id,
        None => {
            r.add(field, message);
            Uuid::nil()
        }
    }
}

/// A list of email addresses, trimmed, lowercase and distinct in order.
fn emails(r: &mut Rules, field: &str, v: &Capped) -> Vec<String> {
    let list = match v {
        Capped::Absent | Capped::Value(Value::Null) => return Vec::new(),
        Capped::Value(Value::Array(list)) => list,
        _ => {
            r.add(field, format!("The {} must be an array.", human(field)));
            return Vec::new();
        }
    };
    let mut out: Vec<String> = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let f = format!("{field}.{i}");
        let e = v
            .as_str()
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_default();
        r.email(&f, &e);
        r.max_chars(&f, &e, 255);
        if !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

// `graph` of the Office 365 tests (Laravel TestGraphMailConnectionRequest).
/// Microsoft Graph app credentials.
#[derive(Deserialize, Default, ToSchema)]
pub struct GraphBody {
    #[schema(value_type = String, max_length = 255, required = true)]
    #[serde(default)]
    tenant_id: Scalar,
    #[schema(value_type = String, max_length = 255, required = true)]
    #[serde(default)]
    client_id: Scalar,
    /// Send the mask back to use the stored secret.
    #[schema(value_type = String, max_length = 1000, required = true)]
    #[serde(default)]
    client_secret: Scalar,
    #[schema(value_type = String, format = Email, max_length = 255, required = true)]
    #[serde(default)]
    mailbox: Scalar,
}

/// `POST /settings/office365/test-connection` and `test-send`.
#[derive(Deserialize, Default, ToSchema)]
pub struct Office365Request {
    #[schema(value_type = GraphBody, required = true)]
    #[serde(default)]
    graph: Option<GraphBody>,
    /// test-send: sender; defaults to the mailbox.
    #[schema(value_type = Option<String>, format = Email)]
    #[serde(default)]
    from: Scalar,
    /// test-send: recipient (required there).
    #[schema(value_type = Option<String>, format = Email)]
    #[serde(default)]
    to: Scalar,
}

/// A validated Office 365 test. `client_secret` may be the mask (the stored one).
pub struct Office365Input {
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: String,
    pub mailbox: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl Validate for Office365Request {
    type Valid = Office365Input;

    fn validate(self, r: &mut Rules) -> Option<Office365Input> {
        let Some(g) = self.graph else {
            r.missing("graph");
            return None;
        };
        let mut req = |field: &str, v: &Scalar, max: usize| -> String {
            match v.filled() {
                Some(Value::String(s)) => {
                    let s = s.trim().to_owned();
                    r.max_chars(field, &s, max);
                    s
                }
                Some(_) => {
                    r.add(field, format!("The {} must be a string.", human(field)));
                    String::new()
                }
                None => {
                    r.missing(field);
                    String::new()
                }
            }
        };
        let tenant_id = req("graph.tenant_id", &g.tenant_id, 255);
        let client_id = req("graph.client_id", &g.client_id, 255);
        let client_secret = req("graph.client_secret", &g.client_secret, 1000);
        let mailbox = req("graph.mailbox", &g.mailbox, 255).to_lowercase();
        if !mailbox.is_empty() {
            r.email("graph.mailbox", &mailbox);
        }
        let mut address = |field: &str, v: &Scalar| {
            v.filled().map(|v| {
                let e = v
                    .as_str()
                    .map(|s| s.trim().to_lowercase())
                    .unwrap_or_default();
                r.email(field, &e);
                e
            })
        };
        let from = address("from", &self.from);
        let to = address("to", &self.to);
        Some(Office365Input {
            tenant_id,
            client_id,
            client_secret,
            mailbox,
            from,
            to,
        })
    }
}
