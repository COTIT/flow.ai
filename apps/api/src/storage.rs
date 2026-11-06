//! Object storage for files: an S3-compatible bucket (Backblaze B2 in prod, MinIO locally),
//! spoken to with presigned requests (rusty-s3 signs, `Outbound` sends), or an in-memory store
//! for tests. Objects are private; a download is a short-lived presigned URL.

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};

use reqwest::{Method, StatusCode, header};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};

use crate::{error::AppError, outbound::Outbound};

/// The bucket prefix every object lives under: the uploads keys (production and staging B2) are
/// limited to it, so a write anywhere else is refused.
pub const UPLOADS_ROOT: &str = "genesy";

/// How long a signed PUT/DELETE stays valid (the request is sent at once).
const SIGN_TTL: Duration = Duration::from_secs(60);
/// Whole request time for one object (10 MB at a slow uplink).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Where the bucket is.
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub key_id: String,
    pub secret: String,
}

/// Tests' bucket: key -> (bytes, content type).
type Objects = Arc<Mutex<HashMap<String, (Vec<u8>, String)>>>;

/// The file store.
#[derive(Clone)]
pub enum Store {
    S3(Arc<S3>),
    Memory(Objects),
}

/// A bucket plus its credentials.
pub struct S3 {
    bucket: Bucket,
    credentials: Credentials,
    http: Outbound,
}

impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::S3(_) => "Store::S3",
            Self::Memory(_) => "Store::Memory",
        })
    }
}

impl Store {
    /// An S3 store; `http` must trust the endpoint's host when it is a local one (MinIO).
    pub fn s3(c: S3Config, http: Outbound) -> Result<Self, String> {
        let endpoint = c
            .endpoint
            .parse()
            .map_err(|e| format!("S3 endpoint: {e}"))?;
        let bucket = Bucket::new(endpoint, UrlStyle::Path, c.bucket, c.region)
            .map_err(|e| format!("S3 bucket: {e}"))?;
        Ok(Self::S3(Arc::new(S3 {
            bucket,
            credentials: Credentials::new(c.key_id, c.secret),
            http,
        })))
    }

    /// An empty in-memory store.
    pub fn memory() -> Self {
        Self::Memory(Arc::default())
    }

    /// Store `bytes` under `key`, with the type and disposition (`file_name`, `inline`) the
    /// bucket serves it with even when a signed URL's overrides are ignored.
    pub async fn put(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: &str,
        file_name: &str,
        inline: bool,
    ) -> Result<(), AppError> {
        match self {
            Self::Memory(m) => {
                lock(m)?.insert(key.to_owned(), (bytes, content_type.to_owned()));
                Ok(())
            }
            Self::S3(s) => {
                let url = s
                    .bucket
                    .put_object(Some(&s.credentials), key)
                    .sign(SIGN_TTL);
                let req = s
                    .http
                    .request(Method::PUT, url.as_str())
                    .header(header::CONTENT_TYPE, content_type)
                    .header(header::CONTENT_DISPOSITION, disposition(file_name, inline))
                    .timeout(REQUEST_TIMEOUT)
                    .body(bytes);
                let res = s.http.send(req).await.map_err(storage_error)?;
                ok(res.status(), "put")
            }
        }
    }

    /// Remove `key`; a missing object is fine.
    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        match self {
            Self::Memory(m) => {
                lock(m)?.remove(key);
                Ok(())
            }
            Self::S3(s) => {
                let url = s
                    .bucket
                    .delete_object(Some(&s.credentials), key)
                    .sign(SIGN_TTL);
                let req = s
                    .http
                    .request(Method::DELETE, url.as_str())
                    .timeout(REQUEST_TIMEOUT);
                let res = s.http.send(req).await.map_err(storage_error)?;
                match res.status() {
                    StatusCode::NOT_FOUND => Ok(()),
                    status => ok(status, "delete"),
                }
            }
        }
    }

    /// A URL that downloads `key` for `ttl`, served with `content_type` and as `file_name`;
    /// `inline` opens it in the browser (a PDF), else it downloads.
    pub fn download_url(
        &self,
        key: &str,
        file_name: &str,
        content_type: &str,
        inline: bool,
        ttl: Duration,
    ) -> Result<String, AppError> {
        match self {
            // The same overrides a signed URL carries, unencoded so tests can read them.
            Self::Memory(m) => lock(m)?
                .contains_key(key)
                .then(|| {
                    format!(
                        "memory://{key}?response-content-type={content_type}&response-content-disposition={}",
                        disposition(file_name, inline)
                    )
                })
                .ok_or_else(|| AppError::Internal(format!("no object {key}"))),
            Self::S3(s) => {
                let mut action = s.bucket.get_object(Some(&s.credentials), key);
                let query = action.query_mut();
                query.insert("response-content-type", content_type.to_owned());
                query.insert(
                    "response-content-disposition",
                    disposition(file_name, inline),
                );
                Ok(action.sign(ttl).to_string())
            }
        }
    }

    /// Bytes of `key`, else None when there is no such object.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, AppError> {
        match self {
            Self::Memory(m) => Ok(lock(m)?.get(key).map(|(b, _)| b.clone())),
            Self::S3(s) => {
                let url = s
                    .bucket
                    .get_object(Some(&s.credentials), key)
                    .sign(SIGN_TTL);
                let req = s
                    .http
                    .request(Method::GET, url.as_str())
                    .timeout(REQUEST_TIMEOUT);
                let res = s.http.send(req).await.map_err(storage_error)?;
                match res.status() {
                    StatusCode::NOT_FOUND => Ok(None),
                    status if status.is_success() => {
                        let bytes = res
                            .bytes()
                            .await
                            .map_err(|e| storage_error(e.without_url().to_string()))?;
                        Ok(Some(bytes.to_vec()))
                    }
                    status => Err(storage_error(format!("get: {status}"))),
                }
            }
        }
    }

    /// Bytes and content type of `key` (memory store only; tests read what was stored).
    pub fn read(&self, key: &str) -> Option<(Vec<u8>, String)> {
        match self {
            Self::Memory(m) => m.lock().ok()?.get(key).cloned(),
            Self::S3(_) => None,
        }
    }
}

/// `inline|attachment; filename="…"` with a plain-ASCII fallback name and the UTF-8 one
/// (RFC 6266).
fn disposition(file_name: &str, inline: bool) -> String {
    let ascii: String = file_name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let encoded: String = file_name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    let kind = if inline { "inline" } else { "attachment" };
    format!("{kind}; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, AppError> {
    m.lock()
        .map_err(|_| AppError::Internal("memory store poisoned".into()))
}

fn ok(status: StatusCode, what: &str) -> Result<(), AppError> {
    if status.is_success() {
        Ok(())
    } else {
        Err(storage_error(format!("{what}: {status}")))
    }
}

/// Logged, then a 503 the client may retry.
fn storage_error(e: String) -> AppError {
    tracing::warn!(error = %e, "file storage call failed");
    AppError::Unavailable("File storage is unavailable. Try again later.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition_quotes_safely() {
        assert_eq!(
            disposition("Invoice 1.pdf", true),
            "inline; filename=\"Invoice 1.pdf\"; filename*=UTF-8''Invoice%201.pdf"
        );
        assert!(disposition("a.svg", false).starts_with("attachment; "));
        let tricky = disposition("a\"b\\c\r\nd é.pdf", true);
        assert!(
            tricky.starts_with("inline; filename=\"a_b_c__d _.pdf\""),
            "{tricky}"
        );
        assert!(!tricky.contains('\n'));
    }

    #[test]
    fn signed_urls_stay_on_the_bucket() {
        let store = Store::s3(
            S3Config {
                endpoint: "https://s3.eu-central-003.backblazeb2.com".into(),
                region: "eu-central-003".into(),
                bucket: "genesy-test".into(),
                key_id: "kid".into(),
                secret: "secret".into(),
            },
            Outbound::new(&[]).unwrap(),
        )
        .unwrap();
        let url = store
            .download_url(
                "genesy/ws/a.pdf",
                "a.pdf",
                "application/pdf",
                true,
                Duration::from_secs(300),
            )
            .unwrap();
        assert!(
            url.starts_with(
                "https://s3.eu-central-003.backblazeb2.com/genesy-test/genesy/ws/a.pdf?"
            ),
            "{url}"
        );
        assert!(url.contains("X-Amz-Signature="));
        assert!(url.contains("X-Amz-Expires=300"));
        assert!(url.contains("response-content-type=application%2Fpdf"));
        assert!(!url.contains("secret"));
    }

    #[tokio::test]
    async fn memory_store_round_trips() {
        let s = Store::memory();
        s.put("k", b"pdf".to_vec(), "application/pdf", "a.pdf", true)
            .await
            .unwrap();
        assert_eq!(
            s.read("k"),
            Some((b"pdf".to_vec(), "application/pdf".into()))
        );
        assert_eq!(
            s.download_url("k", "a.pdf", "application/pdf", true, SIGN_TTL)
                .unwrap(),
            "memory://k?response-content-type=application/pdf&response-content-disposition=inline; filename=\"a.pdf\"; filename*=UTF-8''a.pdf"
        );
        s.delete("k").await.unwrap();
        assert!(s.read("k").is_none());
        assert!(
            s.download_url("k", "a.pdf", "application/pdf", true, SIGN_TTL)
                .is_err()
        );
    }
}
