//! `POST /send-message` (Laravel SendMessageRequest) and the Office 365 test bodies.

use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::{
        pricing::body::{Capped, Scalar},
        relations::address::{self, Address},
    },
};

/// `POST /send-message`; unknown keys are skipped.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct SendMessageRequest {
    addressed_contact_id: Scalar,
    address: Capped,
    to: Capped,
    cc: Capped,
    bcc: Capped,
    subject: Scalar,
    body: Scalar,
    send_method: Scalar,
    attachments: Capped,
    email_profile_id: Scalar,
    pdf_template_id: Scalar,
}

/// A validated send. `template_id` was stored by Laravel, never used.
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

/// `graph` of the Office 365 tests (Laravel TestGraphMailConnectionRequest).
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct GraphBody {
    tenant_id: Scalar,
    client_id: Scalar,
    client_secret: Scalar,
    mailbox: Scalar,
}

/// `POST /settings/office365/test-connection` and `test-send`.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Office365Request {
    graph: Option<GraphBody>,
    from: Scalar,
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
