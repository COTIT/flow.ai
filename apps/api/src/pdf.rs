//! HTML to PDF through the Gotenberg sidecar (headless Chromium, as Laravel's Browsershot): one
//! multipart POST to `/forms/chromium/convert/html`. The sidecar runs with JavaScript off and an
//! allow-list of `data:` URIs plus its own temp files, so the HTML carries its images inline.

use std::time::Duration;

use reqwest::{
    Method,
    multipart::{Form, Part},
};
use tokio::sync::{Semaphore, SemaphorePermit};

use crate::{error::AppError, outbound::Outbound};

/// Whole call; the sidecar gives up on a render after 30 s itself.
const TIMEOUT: Duration = Duration::from_secs(35);

/// 422 when the sidecar refuses a document.
pub const REFUSED: &str =
    "This document can't be rendered: its template loads something it may not.";

/// Largest PDF read back; a bigger answer is cut off as an error.
const MAX_PDF: usize = 25 * 1024 * 1024;

/// Renders building at once: inline images can make one page's HTML tens of MB.
static RENDERS: Semaphore = Semaphore::const_new(2);

/// A render slot, held while a document's HTML is built and rendered; 503 after 10 s waiting.
pub async fn slot() -> Result<SemaphorePermit<'static>, AppError> {
    tokio::time::timeout(Duration::from_secs(10), RENDERS.acquire())
        .await
        .map_err(|_| AppError::Overloaded)?
        .map_err(|_| AppError::Overloaded)
}

/// Page setup of one render (Browsershot `format('A4')`, `margins(…)`, `showBackground`).
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// Margins in millimetres: top, right, bottom, left.
    pub margins_mm: [u32; 4],
    pub print_background: bool,
    /// Chromium page header template (Laravel's background image header).
    pub header_html: Option<String>,
    pub footer_html: Option<String>,
}

/// The Gotenberg client.
#[derive(Clone)]
pub struct Renderer {
    url: String,
    http: Outbound,
}

impl Renderer {
    /// `url` is the sidecar's base URL (`http://gotenberg:3000`); `http` must trust its host.
    pub fn new(url: &str, http: Outbound) -> Self {
        Self {
            url: url.trim_end_matches('/').to_owned(),
            http,
        }
    }

    /// PDF bytes of `html` on A4.
    pub async fn render(&self, html: String, page: Page) -> Result<Vec<u8>, AppError> {
        let [top, right, bottom, left] = page.margins_mm;
        let mut form = Form::new()
            .part("files", html_part("index.html", html)?)
            .text("paperWidth", "210mm")
            .text("paperHeight", "297mm")
            .text("marginTop", format!("{top}mm"))
            .text("marginRight", format!("{right}mm"))
            .text("marginBottom", format!("{bottom}mm"))
            .text("marginLeft", format!("{left}mm"))
            .text("printBackground", page.print_background.to_string())
            .text("emulatedMediaType", "print");
        if let Some(h) = page.header_html {
            form = form.part("files", html_part("header.html", h)?);
        }
        if let Some(f) = page.footer_html {
            form = form.part("files", html_part("footer.html", f)?);
        }
        let req = self
            .http
            .request(
                Method::POST,
                &format!("{}/forms/chromium/convert/html", self.url),
            )
            .timeout(TIMEOUT)
            .multipart(form);
        let mut res = self.http.send(req).await.map_err(unavailable)?;
        if res.status().is_client_error() {
            // The sidecar refused the HTML (it reached for a URL off its allow-list).
            tracing::warn!(status = %res.status(), "pdf render refused");
            return Err(AppError::Unprocessable(REFUSED.into()));
        }
        if !res.status().is_success() {
            return Err(unavailable(format!("status {}", res.status())));
        }
        let mut pdf = Vec::new();
        while let Some(chunk) = res
            .chunk()
            .await
            .map_err(|e| unavailable(e.without_url().to_string()))?
        {
            if pdf.len() + chunk.len() > MAX_PDF {
                return Err(failed("larger than 25 MiB".into()));
            }
            pdf.extend_from_slice(&chunk);
        }
        if !pdf.starts_with(b"%PDF") {
            return Err(failed("not a PDF".into()));
        }
        Ok(pdf)
    }
}

fn html_part(name: &'static str, html: String) -> Result<Part, AppError> {
    Part::text(html)
        .file_name(name)
        .mime_str("text/html")
        .map_err(|e| failed(e.to_string()))
}

fn failed(e: String) -> AppError {
    AppError::Internal(format!("pdf render: {e}"))
}

/// The sidecar failed or is down: logged, then a 503 the client may retry.
fn unavailable(e: String) -> AppError {
    tracing::warn!(error = %e, "pdf render failed");
    AppError::Unavailable("PDF rendering failed. Try again later.")
}
