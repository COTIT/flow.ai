//! A workspace's own SMTP server (settings `smtp` or an email profile's). The host is resolved
//! here and dialled only when every address is public and the port is a mail port; the
//! connection is pinned to that address while TLS checks the certificate against the name, so
//! DNS can't swap the target in between. Operator-trusted hosts (a dev mail catcher) skip both
//! checks.

use std::net::SocketAddr;

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    transport::smtp::{
        authentication::Credentials,
        client::{Tls, TlsParameters},
    },
};

use crate::{mail::Failure, modules::auth::token::PlainSecret, outbound::is_public};

/// Ports a workspace server may be on.
pub const PORTS: &[u16] = &[25, 465, 587, 2525];

/// 422 texts.
pub const NOT_PUBLIC: &str = "The SMTP server isn't at a public address.";
pub const BAD_PORT: &str = "The SMTP port must be 25, 465, 587 or 2525.";
pub const BAD_CERT: &str = "The SMTP server's TLS certificate isn't valid for its host name.";
pub const BAD_LOGIN: &str = "The SMTP server refused the username or password.";

/// A server to send through. `encryption`: `ssl` (TLS from the start), `tls` (STARTTLS
/// required), else STARTTLS when offered.
pub struct Server {
    pub host: String,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<PlainSecret>,
    pub encryption: Option<String>,
}

impl Server {
    fn port(&self) -> u16 {
        self.port
            .unwrap_or(if self.encryption.as_deref() == Some("ssl") {
                465
            } else {
                587
            })
    }
}

/// The addresses to dial: refused unless the port is a mail port and every address `host`
/// resolves to is public, `trusted` hosts excepted.
pub async fn targets(
    host: &str,
    port: u16,
    trusted: &[String],
) -> Result<Vec<SocketAddr>, Failure> {
    let trust = trusted.iter().any(|t| t.eq_ignore_ascii_case(host));
    if !trust && !PORTS.contains(&port) {
        return Err(Failure::Refused(BAD_PORT));
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| Failure::Failed("smtp: host not found".into()))?
        .collect();
    if addrs.is_empty() {
        return Err(Failure::Failed("smtp: host not found".into()));
    }
    if !trust && !addrs.iter().all(|a| is_public(a.ip())) {
        return Err(Failure::Refused(NOT_PUBLIC));
    }
    Ok(addrs)
}

/// Send `message` through `server`, trying each vetted address until one answers; the caller
/// bounds the time.
pub async fn send(server: &Server, trusted: &[String], message: Message) -> Result<(), Failure> {
    let port = server.port();
    let mut last = Failure::Failed("smtp: connection failed".into());
    for addr in targets(&server.host, port, trusted).await? {
        let params = TlsParameters::new(server.host.clone())
            .map_err(|_| Failure::Failed("smtp: tls setup".into()))?;
        let tls = match server.encryption.as_deref() {
            Some("ssl") => Tls::Wrapper(params),
            Some("tls") => Tls::Required(params),
            _ => Tls::Opportunistic(params),
        };
        let mut builder =
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(addr.ip().to_string())
                .port(port)
                .tls(tls)
                .timeout(Some(std::time::Duration::from_secs(15)));
        if let Some(user) = server.username.as_deref().filter(|u| !u.is_empty()) {
            let pass = server
                .password
                .as_ref()
                .map(PlainSecret::expose)
                .unwrap_or_default();
            builder = builder.credentials(Credentials::new(user.to_owned(), pass.to_owned()));
        }
        match builder.build().send(message.clone()).await {
            Ok(_) => return Ok(()),
            Err(e) if e.is_tls() => return Err(Failure::Refused(BAD_CERT)),
            Err(e) if e.status().is_some_and(|c| c.to_string() == "535") => {
                return Err(Failure::Refused(BAD_LOGIN));
            }
            // The server answered: another address won't do better.
            Err(e) if e.status().is_some() || e.is_timeout() => {
                return Err(Failure::Failed(class(&e)));
            }
            Err(e) => last = Failure::Failed(class(&e)),
        }
    }
    Err(last)
}

/// A short class of an SMTP error, without the server's text (it can echo addresses).
pub fn class(e: &lettre::transport::smtp::Error) -> String {
    match e.status() {
        Some(code) if e.is_permanent() => format!("smtp: permanent {code}"),
        Some(code) => format!("smtp: transient {code}"),
        None if e.is_timeout() => "smtp: timed out".into(),
        None if e.is_tls() => "smtp: tls".into(),
        None => "smtp: connection failed".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn only_public_addresses_on_mail_ports() {
        assert_eq!(
            targets("localhost", 587, &[]).await,
            Err(Failure::Refused(NOT_PUBLIC))
        );
        assert_eq!(
            targets("127.0.0.1", 25, &[]).await,
            Err(Failure::Refused(NOT_PUBLIC))
        );
        assert_eq!(
            targets("example.com", 6379, &[]).await,
            Err(Failure::Refused(BAD_PORT))
        );
        // A trusted host skips both checks.
        let trusted = ["LOCALHOST".to_owned()];
        assert!(targets("localhost", 1025, &trusted).await.is_ok());
    }

    #[test]
    fn default_ports() {
        let s = |e: Option<&str>| Server {
            host: "h".into(),
            port: None,
            username: None,
            password: None,
            encryption: e.map(str::to_owned),
        };
        assert_eq!(
            (s(Some("ssl")).port(), s(Some("tls")).port(), s(None).port()),
            (465, 587, 587)
        );
    }
}
