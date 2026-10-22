//! Microsoft Graph mail (Laravel MicrosoftGraphMailService): an app-only token (client
//! credentials, cached per tenant, client and secret until a minute before it expires),
//! `sendMail` and an inbox probe. Calls go through `Outbound` to Microsoft's fixed hosts;
//! `GRAPH_LOGIN_URL` / `GRAPH_API_URL` replace them only for local fakes.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use base64::Engine;
use reqwest::{Method, Url, header::CONTENT_TYPE};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    mail::{Failure, Mail},
    modules::auth::token::PlainSecret,
    outbound::Outbound,
};

pub const LOGIN_URL: &str = "https://login.microsoftonline.com";
pub const API_URL: &str = "https://graph.microsoft.com";

/// 422 when no token can be had (Laravel's text).
pub const TOKEN_FAILED: &str =
    "Could not authenticate. Check the tenant, client id and client secret.";

/// Largest answer read back (a token or one inbox message id).
const MAX_ANSWER: usize = 1024 * 1024;

/// An app registration and the mailbox it acts for.
pub struct Account {
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: PlainSecret,
    pub mailbox: String,
}

/// Tokens by hash of (tenant, client, secret), with the instant they stop being used.
type Tokens = Arc<Mutex<HashMap<[u8; 32], (String, Instant)>>>;

/// The Graph client, cheap to clone.
#[derive(Clone)]
pub struct Client {
    login: String,
    api: String,
    http: Option<Outbound>,
    tokens: Tokens,
}

impl Client {
    /// Microsoft's hosts, but no HTTP client yet: every call fails until `new` replaces it.
    pub fn microsoft() -> Self {
        Self {
            login: LOGIN_URL.into(),
            api: API_URL.into(),
            http: None,
            tokens: Arc::default(),
        }
    }

    /// A client for `login` and `api` (no trailing slash) through `http`.
    pub fn new(login: &str, api: &str, http: Outbound) -> Self {
        Self {
            login: login.trim_end_matches('/').to_owned(),
            api: api.trim_end_matches('/').to_owned(),
            http: Some(http),
            tokens: Arc::default(),
        }
    }

    /// Send `mail` as its from address (`POST /v1.0/users/{from}/sendMail`), a copy kept in
    /// Sent Items.
    pub async fn send(&self, account: &Account, mail: &Mail) -> Result<(), Failure> {
        let token = self.token(account).await?;
        let to = |list: &[String]| -> Vec<Value> {
            list.iter()
                .map(|a| json!({ "emailAddress": { "address": a } }))
                .collect()
        };
        let attachments: Vec<Value> = mail
            .attachments
            .iter()
            .map(|a| {
                json!({
                    "@odata.type": "#microsoft.graph.fileAttachment",
                    "name": a.name,
                    "contentType": a.content_type,
                    "contentBytes": base64::engine::general_purpose::STANDARD.encode(&a.bytes),
                })
            })
            .collect();
        let mut message = json!({
            "subject": mail.subject,
            "body": { "contentType": "HTML", "content": mail.html },
            "toRecipients": to(&mail.to),
            "ccRecipients": to(&mail.cc),
            "bccRecipients": to(&mail.bcc),
            "attachments": attachments,
            "internetMessageHeaders": [{ "name": "X-Genesy-Email-Log", "value": mail.log_id.to_string() }],
        });
        if let Some(name) = &mail.from.name {
            message["from"] =
                json!({ "emailAddress": { "address": mail.from.email, "name": name } });
        }
        let url = self.users_url(&[&mail.from.email, "sendMail"])?;
        let body = json!({ "message": message, "saveToSentItems": true });
        let res = self.call(Method::POST, url, &token, Some(body)).await?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(Failure::Failed(format!(
                "graph: sendMail {}",
                res.status().as_u16()
            )))
        }
    }

    /// A token can be had for `account` (the Office 365 test's first stage).
    pub async fn check_token(&self, account: &Account) -> Result<(), Failure> {
        self.token(account).await.map(|_| ())
    }

    /// Messages (0 or 1) in the mailbox's inbox: proves the token may read it. The error is
    /// Graph's status.
    pub async fn inbox_probe(&self, account: &Account) -> Result<usize, Failure> {
        let token = self.token(account).await?;
        let mut url = self.users_url(&[&account.mailbox, "mailFolders", "inbox", "messages"])?;
        url.query_pairs_mut()
            .append_pair("$top", "1")
            .append_pair("$select", "id");
        let res = self.call(Method::GET, url, &token, None).await?;
        let status = res.status();
        if !status.is_success() {
            return Err(Failure::Failed(format!(
                "Graph answered {}.",
                status.as_u16()
            )));
        }
        let v = read_json(res).await?;
        Ok(v["value"].as_array().map_or(0, Vec::len))
    }

    /// A token for `account`, from the cache while it has a minute left.
    async fn token(&self, account: &Account) -> Result<String, Failure> {
        let key: [u8; 32] = Sha256::digest(
            [
                &account.tenant_id,
                &account.client_id,
                account.client_secret.expose(),
            ]
            .join("\n"),
        )
        .into();
        if let Ok(cache) = self.tokens.lock()
            && let Some((token, until)) = cache.get(&key)
            && Instant::now() < *until
        {
            return Ok(token.clone());
        }
        let tenant = &account.tenant_id;
        if tenant.is_empty()
            || tenant.len() > 255
            || !tenant
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        {
            return Err(Failure::Refused(TOKEN_FAILED));
        }
        let url = format!("{}/{tenant}/oauth2/v2.0/token", self.login);
        let mut form = Url::parse("http://form/").map_err(|e| Failure::Failed(e.to_string()))?;
        form.query_pairs_mut()
            .append_pair("client_id", &account.client_id)
            .append_pair("client_secret", account.client_secret.expose())
            .append_pair("scope", &format!("{API_URL}/.default"))
            .append_pair("grant_type", "client_credentials");
        let body = form.query().unwrap_or_default().to_owned();
        let http = self.http()?;
        let req = http
            .request(Method::POST, &url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .timeout(Duration::from_secs(15))
            .body(body);
        let res = http
            .send(req)
            .await
            .map_err(|_| Failure::Failed("graph: login unreachable".into()))?;
        if !res.status().is_success() {
            return Err(Failure::Refused(TOKEN_FAILED));
        }
        let v = read_json(res).await?;
        let token = v["access_token"]
            .as_str()
            .ok_or(Failure::Refused(TOKEN_FAILED))?
            .to_owned();
        let life = v["expires_in"].as_u64().unwrap_or(0).saturating_sub(60);
        if let Ok(mut cache) = self.tokens.lock() {
            cache.retain(|_, (_, until)| Instant::now() < *until);
            cache.insert(
                key,
                (token.clone(), Instant::now() + Duration::from_secs(life)),
            );
        }
        Ok(token)
    }

    /// `{api}/v1.0/users/{segments…}`, each segment escaped.
    fn users_url(&self, segments: &[&str]) -> Result<Url, Failure> {
        let mut url =
            Url::parse(&self.api).map_err(|_| Failure::Failed("graph: api url".into()))?;
        url.path_segments_mut()
            .map_err(|_| Failure::Failed("graph: api url".into()))?
            .pop_if_empty()
            .extend(["v1.0", "users"])
            .extend(segments);
        Ok(url)
    }

    async fn call(
        &self,
        method: Method,
        url: Url,
        token: &str,
        body: Option<Value>,
    ) -> Result<reqwest::Response, Failure> {
        let http = self.http()?;
        let mut req = http
            .request(method, url.as_str())
            .bearer_auth(token)
            .timeout(Duration::from_secs(15));
        if let Some(body) = body {
            let bytes = serde_json::to_vec(&body).map_err(|e| Failure::Failed(e.to_string()))?;
            req = req.header(CONTENT_TYPE, "application/json").body(bytes);
        }
        http.send(req)
            .await
            .map_err(|_| Failure::Failed("graph: unreachable".into()))
    }

    fn http(&self) -> Result<&Outbound, Failure> {
        self.http
            .as_ref()
            .ok_or_else(|| Failure::Failed("graph: no http client".into()))
    }
}

/// A JSON answer of at most `MAX_ANSWER` bytes.
async fn read_json(mut res: reqwest::Response) -> Result<Value, Failure> {
    let mut body = Vec::new();
    while let Some(chunk) = res
        .chunk()
        .await
        .map_err(|_| Failure::Failed("graph: answer cut".into()))?
    {
        if body.len() + chunk.len() > MAX_ANSWER {
            return Err(Failure::Failed("graph: answer too large".into()));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| Failure::Failed("graph: answer not json".into()))
}
