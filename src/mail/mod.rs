//! Outgoing email. Every mail leaves through `Mailer::dispatch`: the recipient allow-list
//! (`MAIL_ALLOWED_RECIPIENT_DOMAINS`) first, then one transport under one time limit: the
//! platform SMTP (Resend in prod, Mailpit in dev, a capture in tests), a workspace's own SMTP
//! server (`smtp`) or Microsoft Graph (`graph`). Logs name domains and counts, never an
//! address, a subject, a body or a credential.

pub mod graph;
pub mod smtp;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{
        Attachment as Part, Mailbox, MultiPart, SinglePart,
        header::{ContentType, HeaderName, HeaderValue},
    },
};
use uuid::Uuid;

use crate::error::AppError;

/// A hung SMTP server must not hold a request open for lettre's 60s default.
const SMTP_TIMEOUT: Duration = Duration::from_secs(10);

/// One send, whatever the transport: well under the 60 s request limit.
const SEND_TIMEOUT: Duration = Duration::from_secs(15);

/// One plain-text platform email.
#[derive(Debug, Clone)]
pub struct Outgoing {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Mails captured by `Mailer::Capture`.
pub type Outbox = Arc<Mutex<Vec<Outgoing>>>;

/// An address with an optional display name.
#[derive(Debug, Clone)]
pub struct Sender {
    pub email: String,
    pub name: Option<String>,
}

/// A file attached to a workspace mail.
pub struct Attachment {
    pub name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// A rendered workspace mail. Recipients are lowercase, valid and distinct.
pub struct Mail {
    pub from: Sender,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub html: String,
    pub attachments: Vec<Attachment>,
    /// Sent as `X-Genesy-Email-Log`, so provider events can find the log row.
    pub log_id: Uuid,
}

impl Mail {
    fn recipients(&self) -> impl Iterator<Item = &str> {
        self.to
            .iter()
            .chain(&self.cc)
            .chain(&self.bcc)
            .map(String::as_str)
    }
}

/// Where a workspace mail leaves.
pub enum Via {
    Smtp(smtp::Server),
    Graph(graph::Account),
}

/// What became of a mail.
#[derive(Debug, PartialEq)]
pub enum Delivered {
    Accepted {
        message_id: Option<String>,
    },
    /// Some recipient's domain isn't allowed: nothing was sent.
    Blocked {
        domains: Vec<String>,
    },
}

/// Why a send failed. Neither carries an address or a server's own words.
#[derive(Debug, PartialEq)]
pub enum Failure {
    /// The workspace's setup can't work (a 422 with this text).
    Refused(&'static str),
    /// The server or service failed; a short class for the log (a 503 to retry).
    Failed(String),
}

/// Mail sender, cheap to clone.
#[derive(Clone)]
pub struct Mailer {
    platform: Platform,
    shared: Arc<Shared>,
}

#[derive(Clone)]
enum Platform {
    Smtp {
        transport: AsyncSmtpTransport<Tokio1Executor>,
        from: Mailbox,
    },
    /// Test double: keeps platform mails in memory instead of sending.
    Capture(Outbox),
}

struct Shared {
    /// Lowercase domains a mail may go to; empty allows all.
    allowed: Vec<String>,
    timeout: Duration,
    /// Operator-configured SMTP hosts that skip the public-address and port checks (Mailpit).
    smtp_trusted: Vec<String>,
    graph: graph::Client,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            allowed: Vec::new(),
            timeout: SEND_TIMEOUT,
            smtp_trusted: Vec::new(),
            graph: graph::Client::microsoft(),
        }
    }
}

/// What `dispatch` sends: a platform mail or a workspace mail by its transport.
enum Target<'a> {
    Platform(&'a Outgoing),
    Workspace(&'a Via, &'a Mail),
}

impl Mailer {
    /// `url` like `smtp://127.0.0.1:1025` or `smtps://resend:<key>@smtp.resend.com:465`.
    /// `from` like `Genesy <no-reply@genesy.app>`.
    pub fn smtp(url: &str, from: &str) -> Result<Self, String> {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
            .map_err(|e| format!("SMTP_URL invalid: {e}"))?
            .timeout(Some(SMTP_TIMEOUT))
            .build();
        let from = from
            .parse()
            .map_err(|e| format!("MAIL_FROM invalid: {e}"))?;
        Ok(Self {
            platform: Platform::Smtp { transport, from },
            shared: Arc::default(),
        })
    }

    /// A mailer whose platform mails land in the returned outbox.
    pub fn capture() -> (Self, Outbox) {
        let outbox = Outbox::default();
        (Self::capture_into(outbox.clone()), outbox)
    }

    /// A mailer whose platform mails land in `outbox`.
    pub fn capture_into(outbox: Outbox) -> Self {
        Self {
            platform: Platform::Capture(outbox),
            shared: Arc::default(),
        }
    }

    /// Only these recipient domains (comma-separated, any case); empty allows all.
    pub fn with_allowed_domains(self, list: &str) -> Self {
        let allowed = list
            .split(',')
            .map(|d| d.trim().to_ascii_lowercase())
            .filter(|d| !d.is_empty())
            .collect();
        self.with(|s| s.allowed = allowed)
    }

    /// Workspace SMTP hosts the operator trusts (dev mail catchers).
    pub fn with_trusted_smtp_hosts(self, hosts: &[String]) -> Self {
        let hosts = hosts.iter().map(|h| h.to_ascii_lowercase()).collect();
        self.with(|s| s.smtp_trusted = hosts)
    }

    /// A Graph client other than Microsoft's (local fakes).
    pub fn with_graph(self, client: graph::Client) -> Self {
        self.with(|s| s.graph = client)
    }

    /// Another limit for one send (tests).
    pub fn with_timeout(self, timeout: Duration) -> Self {
        self.with(|s| s.timeout = timeout)
    }

    fn with(mut self, f: impl FnOnce(&mut Shared)) -> Self {
        let mut shared = Shared {
            allowed: self.shared.allowed.clone(),
            timeout: self.shared.timeout,
            smtp_trusted: self.shared.smtp_trusted.clone(),
            graph: self.shared.graph.clone(),
        };
        f(&mut shared);
        self.shared = Arc::new(shared);
        self
    }

    /// The Graph client (the settings connection tests use it directly).
    pub fn graph(&self) -> &graph::Client {
        &self.shared.graph
    }

    /// A platform mail. A blocked one is dropped with a warning, as Laravel did.
    pub async fn send(&self, mail: Outgoing) -> Result<(), AppError> {
        match self.dispatch(Target::Platform(&mail)).await {
            Ok(_) => Ok(()),
            Err(Failure::Refused(why)) => Err(AppError::Internal(format!("platform mail: {why}"))),
            Err(Failure::Failed(why)) => Err(AppError::Internal(format!("platform mail: {why}"))),
        }
    }

    /// A workspace mail by `via`.
    pub async fn deliver(&self, via: &Via, mail: &Mail) -> Result<Delivered, Failure> {
        self.dispatch(Target::Workspace(via, mail)).await
    }

    /// The one way out: the allow-list, then the transport within the time limit.
    async fn dispatch(&self, target: Target<'_>) -> Result<Delivered, Failure> {
        let recipients: Vec<&str> = match &target {
            Target::Platform(m) => vec![m.to.as_str()],
            Target::Workspace(_, m) => m.recipients().collect(),
        };
        if let Some(domains) = self.blocked(&recipients) {
            tracing::warn!(
                ?domains,
                recipients = recipients.len(),
                "mail blocked by the recipient allow-list"
            );
            return Ok(Delivered::Blocked { domains });
        }
        let send = async {
            match target {
                Target::Platform(m) => self.platform_send(m).await,
                Target::Workspace(Via::Smtp(server), m) => {
                    let message = build(m)?;
                    let id = message_id(&message);
                    smtp::send(server, &self.shared.smtp_trusted, message).await?;
                    Ok(Delivered::Accepted { message_id: id })
                }
                Target::Workspace(Via::Graph(account), m) => {
                    self.shared.graph.send(account, m).await?;
                    Ok(Delivered::Accepted { message_id: None })
                }
            }
        };
        tokio::time::timeout(self.shared.timeout, send)
            .await
            .unwrap_or_else(|_| Err(Failure::Failed("timed out".into())))
    }

    /// The domains outside the allow-list, if any (all when there is no recipient at all).
    fn blocked(&self, recipients: &[&str]) -> Option<Vec<String>> {
        if self.shared.allowed.is_empty() {
            return None;
        }
        let mut domains: Vec<String> = recipients
            .iter()
            .map(|r| {
                r.rsplit_once('@')
                    .map_or("", |(_, d)| d)
                    .to_ascii_lowercase()
            })
            .filter(|d| !self.shared.allowed.contains(d))
            .collect();
        domains.sort();
        domains.dedup();
        (recipients.is_empty() || !domains.is_empty()).then_some(domains)
    }

    async fn platform_send(&self, mail: &Outgoing) -> Result<Delivered, Failure> {
        match &self.platform {
            Platform::Capture(outbox) => {
                outbox
                    .lock()
                    .map_err(|_| Failure::Failed("outbox lock poisoned".into()))?
                    .push(mail.clone());
                Ok(Delivered::Accepted { message_id: None })
            }
            Platform::Smtp { transport, from } => {
                let to: Mailbox = mail
                    .to
                    .parse()
                    .map_err(|_| Failure::Failed("recipient isn't an address".into()))?;
                let message = Message::builder()
                    .from(from.clone())
                    .to(to)
                    .subject(mail.subject.clone())
                    .header(ContentType::TEXT_PLAIN)
                    .body(mail.body.clone())
                    .map_err(|e| Failure::Failed(format!("build: {e}")))?;
                transport
                    .send(message)
                    .await
                    .map_err(|e| Failure::Failed(smtp::class(&e)))?;
                Ok(Delivered::Accepted { message_id: None })
            }
        }
    }
}

/// A workspace mail as MIME: HTML body, attachments, the log header.
fn build(m: &Mail) -> Result<Message, Failure> {
    let bad = |what: &str| Failure::Failed(format!("{what} isn't an address"));
    let from = Mailbox::new(
        m.from.name.clone(),
        m.from.email.parse().map_err(|_| bad("from"))?,
    );
    let mut b = Message::builder()
        .from(from)
        .subject(m.subject.clone())
        .header(LogHeader(m.log_id));
    for (list, kind) in [(&m.to, 0), (&m.cc, 1), (&m.bcc, 2)] {
        for r in list {
            let mbox: Mailbox = r.parse().map_err(|_| bad("recipient"))?;
            b = match kind {
                0 => b.to(mbox),
                1 => b.cc(mbox),
                _ => b.bcc(mbox),
            };
        }
    }
    let mut body = MultiPart::mixed().singlepart(SinglePart::html(m.html.clone()));
    for a in &m.attachments {
        let kind = ContentType::parse(&a.content_type)
            .or_else(|_| ContentType::parse("application/octet-stream"))
            .map_err(|e| Failure::Failed(format!("attachment type: {e}")))?;
        body = body.singlepart(Part::new(a.name.clone()).body(a.bytes.clone(), kind));
    }
    b.multipart(body)
        .map_err(|e| Failure::Failed(format!("build: {e}")))
}

fn message_id(m: &Message) -> Option<String> {
    m.headers().get_raw("Message-ID").map(str::to_owned)
}

/// `X-Genesy-Email-Log: <uuid>`.
#[derive(Clone)]
struct LogHeader(Uuid);

impl lettre::message::header::Header for LogHeader {
    fn name() -> HeaderName {
        HeaderName::new_from_ascii_str("X-Genesy-Email-Log")
    }

    fn parse(s: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self(s.parse()?))
    }

    fn display(&self) -> HeaderValue {
        HeaderValue::new(Self::name(), self.0.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail() -> Outgoing {
        Outgoing {
            to: "a@b.co".into(),
            subject: "Hi".into(),
            body: "Body".into(),
        }
    }

    #[tokio::test]
    async fn capture_keeps_mail() {
        let (mailer, outbox) = Mailer::capture();

        mailer.send(mail()).await.unwrap();

        let sent = outbox.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to, "a@b.co");
    }

    /// Exact domains, any case; one outsider blocks the whole mail; no recipient is blocked
    /// once a list is set (Laravel BlockDisallowedMailRecipients).
    #[tokio::test]
    async fn the_allow_list() {
        let (open, _) = Mailer::capture();
        assert_eq!(open.blocked(&["x@anywhere.test"]), None);
        let (m, outbox) = Mailer::capture();
        let m = m.with_allowed_domains(" Cotit.EU , genesy.app,");
        assert_eq!(m.blocked(&["a@COTIT.eu", "b@genesy.app"]), None);
        assert_eq!(
            m.blocked(&["a@cotit.eu", "b@acme.test", "c@Acme.test"]),
            Some(vec!["acme.test".into()])
        );
        for evil in [
            "a@mail.cotit.eu",
            "a@evil-cotit.eu",
            "a@cotit.eu.evil.com",
            "a@eu",
        ] {
            assert!(m.blocked(&[evil]).is_some(), "{evil}");
        }
        assert_eq!(m.blocked(&[]), Some(vec![]));
        // Platform mails go through the same gate.
        m.send(mail()).await.unwrap();
        assert!(outbox.lock().unwrap().is_empty());
    }

    // tokio runtime needed: lettre's pool must drop inside one.
    #[tokio::test]
    async fn smtp_config_errors_name_var() {
        let bad_url = Mailer::smtp("not a url", "A <a@b.co>").err().unwrap();
        let bad_from = Mailer::smtp("smtp://127.0.0.1:1025", "not an address")
            .err()
            .unwrap();

        assert!(bad_url.starts_with("SMTP_URL"), "{bad_url}");
        assert!(bad_from.starts_with("MAIL_FROM"), "{bad_from}");
    }

    #[tokio::test]
    async fn smtp_unreachable_is_internal_error() {
        let mailer = Mailer::smtp("smtp://127.0.0.1:1", "A <a@b.co>").unwrap();

        assert!(matches!(
            mailer.send(mail()).await,
            Err(AppError::Internal(_))
        ));
    }
}
