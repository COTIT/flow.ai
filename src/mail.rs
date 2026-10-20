//! Outgoing email: SMTP (Mailpit in dev, Postmark SMTP in prod) or in-memory capture for tests.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
};

use crate::error::AppError;

/// A hung SMTP server must not hold a request open for lettre's 60s default.
const SMTP_TIMEOUT: Duration = Duration::from_secs(10);

/// One plain-text email.
#[derive(Debug, Clone)]
pub struct Outgoing {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Mails captured by `Mailer::Capture`.
pub type Outbox = Arc<Mutex<Vec<Outgoing>>>;

/// Mail sender. Enum over two fixed backends, so no async trait object needed.
#[derive(Clone)]
pub enum Mailer {
    Smtp {
        transport: AsyncSmtpTransport<Tokio1Executor>,
        from: Mailbox,
    },
    /// Test double: keeps mails in memory instead of sending.
    Capture(Outbox),
}

impl Mailer {
    /// `url` like `smtp://127.0.0.1:1025` or `smtp://user:pass@smtp.postmarkapp.com:587?tls=required`.
    /// `from` like `Genesy <no-reply@genesy.app>`.
    pub fn smtp(url: &str, from: &str) -> Result<Self, String> {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
            .map_err(|e| format!("SMTP_URL invalid: {e}"))?
            .timeout(Some(SMTP_TIMEOUT))
            .build();
        let from = from
            .parse()
            .map_err(|e| format!("MAIL_FROM invalid: {e}"))?;
        Ok(Self::Smtp { transport, from })
    }

    pub fn capture() -> (Self, Outbox) {
        let outbox = Outbox::default();
        (Self::Capture(outbox.clone()), outbox)
    }

    pub async fn send(&self, mail: Outgoing) -> Result<(), AppError> {
        match self {
            Self::Capture(outbox) => {
                outbox
                    .lock()
                    .map_err(|_| AppError::Internal("outbox lock poisoned".into()))?
                    .push(mail);
                Ok(())
            }
            Self::Smtp { transport, from } => {
                let to: Mailbox = mail
                    .to
                    .parse()
                    .map_err(|e| AppError::Internal(format!("mail recipient: {e}")))?;
                let message = Message::builder()
                    .from(from.clone())
                    .to(to)
                    .subject(mail.subject)
                    .header(ContentType::TEXT_PLAIN)
                    .body(mail.body)
                    .map_err(|e| AppError::Internal(format!("mail build: {e}")))?;
                transport
                    .send(message)
                    .await
                    .map_err(|e| AppError::Internal(format!("smtp send: {e}")))?;
                Ok(())
            }
        }
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
