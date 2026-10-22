//! HTML to PDF through the Gotenberg sidecar (headless Chromium, as Laravel's Browsershot): one
//! multipart POST to `/forms/chromium/convert/html`. The sidecar runs with JavaScript off and a
//! deny-list that lets only `data:` URIs load, so the HTML carries its images inline.

use std::time::Duration;

use reqwest::{
    Method,
    multipart::{Form, Part},
};

use crate::{error::AppError, outbound::Outbound};

/// Whole call; the sidecar gives up on a render after 30 s itself.
const TIMEOUT: Duration = Duration::from_secs(35);

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
    pub async fn render(&self, html: String, page: &Page) -> Result<Vec<u8>, AppError> {
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
        if let Some(h) = &page.header_html {
            form = form.part("files", html_part("header.html", h.clone())?);
        }
        if let Some(f) = &page.footer_html {
            form = form.part("files", html_part("footer.html", f.clone())?);
        }
        let req = self
            .http
            .request(
                Method::POST,
                &format!("{}/forms/chromium/convert/html", self.url),
            )
            .timeout(TIMEOUT)
            .multipart(form);
        let res = self.http.send(req).await.map_err(failed)?;
        if !res.status().is_success() {
            return Err(failed(format!("status {}", res.status())));
        }
        let bytes = res.bytes().await.map_err(|e| failed(e.to_string()))?;
        if !bytes.starts_with(b"%PDF") {
            return Err(failed("not a PDF".into()));
        }
        Ok(bytes.to_vec())
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
