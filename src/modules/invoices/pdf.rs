//! Invoice PDFs (Laravel CreateInvoicePdf): with a chosen invoice template
//! (CreateFileFromPdfTemplate + `custom-pdf-template.blade.php`) or the built-in layout
//! (`invoice.blade.php`), rendered as HTML (askama, escaped by default) through Gotenberg.
//! Template texts are the user's HTML; values put into them are escaped (Laravel didn't).

use std::collections::HashMap;

use askama::Template;
use base64::Engine;
use rust_decimal::{Decimal, RoundingStrategy};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        contacts::{self, dto::full_name, repo::ContactRow},
        files,
        invoices::{
            repo::{self, InvoiceRow, ItemRow},
            resource::read_summary,
        },
        pdf_templates::{fields, repo::TemplateRow},
        pricing::engine,
        products::{self, repo::ProductRow},
        relations::{self, address::Address, dto::relation_nr},
        settings, workspaces,
    },
    pdf::Page,
};

/// 503 text without the Gotenberg sidecar.
pub const NO_RENDERER: &str = "PDF rendering isn't configured.";
const TAILWIND: &str = include_str!("../../../templates/pdf/tailwind.css");

/// A rendered PDF and the file name Laravel gave it.
pub struct Pdf {
    pub bytes: Vec<u8>,
    pub file_name: String,
}

/// Render invoice `id` with `template` (an invoice template), else with the built-in layout.
pub async fn render(
    state: &AppState,
    workspace_id: Uuid,
    id: Uuid,
    template: Option<&TemplateRow>,
) -> Result<Pdf, AppError> {
    let renderer = state
        .pdf
        .as_ref()
        .ok_or(AppError::Unavailable(NO_RENDERER))?;
    let _slot = crate::pdf::slot().await?;
    let d = Data::load(state, workspace_id, id).await?;
    let (html, page, file_name) = match template {
        Some(t) => {
            let html = custom(state, workspace_id, &d, t).await?;
            let mut name = format!("{}_", ucfirst(&t.domain));
            if let Some(nr) = d.invoice_nr() {
                name.push_str(&format!("{nr}_"));
            }
            if let Some(action) = &t.domain_action {
                name.push_str(&format!("{action}_"));
            }
            name.push_str(&format!("{}.pdf", chrono::Utc::now().timestamp()));
            (html.0, html.1, name)
        }
        None => {
            let html = legacy(state, workspace_id, &d).await?;
            let name = format!("Invoice_pdf_{}.pdf", d.invoice_nr().unwrap_or_default());
            (
                html,
                Page {
                    margins_mm: [0; 4],
                    print_background: false,
                    header_html: None,
                    footer_html: None,
                },
                name,
            )
        }
    };
    let bytes = renderer.render(html, &page).await?;
    Ok(Pdf { bytes, file_name })
}

/// Everything both layouts read.
struct Data {
    invoice: InvoiceRow,
    lines: Vec<ItemRow>,
    summary: engine::Summary,
    currency: repo::CurrencyPrint,
    nl: bool,
    color: Colors,
    relation: Option<relations::repo::RelationRow>,
    relation_type: Option<String>,
    contact: Option<ContactRow>,
    products: HashMap<Uuid, ProductRow>,
}

impl Data {
    async fn load(state: &AppState, workspace_id: Uuid, id: Uuid) -> Result<Self, AppError> {
        let invoice = repo::find(&state.db, workspace_id, id)
            .await?
            .ok_or(AppError::NotFound)?;
        let lines = repo::lines(&state.db, workspace_id, id).await?;
        let currency = repo::currency_print(&state.db, workspace_id, invoice.currency_id).await?;
        let dp = read_summary(&invoice.summary)
            .2
            .unwrap_or(u32::try_from(currency.decimals).unwrap_or(2));
        let engine_lines: Vec<engine::Line> = lines
            .iter()
            .map(|l| engine::Line {
                quantity: l.quantity,
                price: l.price,
                vat: l.vat,
                discount: l.discount_percentage,
                purchase_price: Some(l.purchase_price),
                set_child: false,
            })
            .collect();
        let summary = engine::summarise(&engine_lines, dp)
            .map_err(|_| AppError::Internal("stored invoice lines out of range".into()))?;
        let (_, language) = workspaces::repo::locale(&state.db, workspace_id).await?;
        let general = settings::repo::section(&state.db, workspace_id, "general").await?;
        let color = Colors::of(
            general
                .as_ref()
                .and_then(|g| g.get("color"))
                .and_then(Value::as_str),
        );
        let mut found =
            relations::repo::find_many(&state.db, workspace_id, &[invoice.relation_id]).await?;
        let (relation, relation_type) = match found.pop() {
            Some((r, kind, _)) => (Some(r), kind.map(|k| k.name)),
            None => (None, None),
        };
        let contact =
            contacts::repo::for_relations(&state.db, workspace_id, &[invoice.relation_id])
                .await?
                .into_iter()
                .max_by_key(|c| c.default_contact);
        let product_ids: Vec<Uuid> = lines.iter().filter_map(|l| l.product_id).collect();
        let products = products::repo::find_many(&state.db, workspace_id, &product_ids)
            .await?
            .into_iter()
            .map(|p| (p.id, p))
            .collect();
        Ok(Self {
            invoice,
            lines,
            summary,
            currency,
            nl: language.as_deref() == Some("nl"),
            color,
            relation,
            relation_type,
            contact,
            products,
        })
    }

    /// Drafts have no number until F6 (send).
    fn invoice_nr(&self) -> Option<String> {
        None
    }

    fn dp(&self) -> u32 {
        u32::try_from(self.currency.decimals).unwrap_or(2)
    }

    /// `symbol_prefix` + `number_format(v, decimals, ',', '.')` (Laravel's summary amounts).
    fn money(&self, v: Decimal) -> String {
        format!(
            "{}{}",
            self.currency.symbol_prefix.as_deref().unwrap_or(""),
            number_format(v, self.dp())
        )
    }
}

/// Laravel GeneralSetting::templateColor classes.
struct Colors {
    header: &'static str,
    to_from: &'static str,
    summary_background: &'static str,
    summary_border: &'static str,
    item_header: &'static str,
}

impl Colors {
    fn of(color: Option<&str>) -> Self {
        match color {
            Some(c @ ("orange" | "green" | "blue")) => {
                let (text, bg, border) = match c {
                    "orange" => ("text-orange-500", "bg-orange-50", "border-orange-200"),
                    "green" => ("text-green-500", "bg-green-50", "border-green-200"),
                    _ => ("text-blue-500", "bg-blue-50", "border-blue-200"),
                };
                Self {
                    header: text,
                    to_from: text,
                    summary_background: bg,
                    summary_border: border,
                    item_header: text,
                }
            }
            _ => Self {
                header: "text-gray-900",
                to_from: "text-gray-500",
                summary_background: "bg-gray-50",
                summary_border: "border-gray-200",
                item_header: "text-gray-900",
            },
        }
    }
}

/// PHP `number_format($v, $dp, ',', '.')`: half away from zero, `.` thousands, `,` decimals.
pub fn number_format(v: Decimal, dp: u32) -> String {
    let r = v.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero);
    let text = format!("{:.*}", dp as usize, r.abs());
    let (int, frac) = text.split_once('.').unwrap_or((&text, ""));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    let sign = if r.is_sign_negative() && !r.is_zero() {
        "-"
    } else {
        ""
    };
    if frac.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped},{frac}")
    }
}

/// HTML-escape text (askama does this in templates; here for values put into user HTML).
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Laravel buildTemplateText: `{key}` -> its (escaped) value, twice over every key, then any
/// `{…}` left on one line -> `-`.
pub fn fill(text: &str, vars: &[(String, String)]) -> String {
    let mut out = text.to_owned();
    for _ in 0..2 {
        for (k, v) in vars {
            out = out.replace(&format!("{{{k}}}"), &escape(v));
        }
    }
    let mut result = String::with_capacity(out.len());
    let mut rest = out.as_str();
    while let Some(start) = rest.find('{') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find(['}', '\n']) {
            Some(end) if after.as_bytes()[end] == b'}' => {
                result.push('-');
                rest = &after[end + 1..];
            }
            _ => {
                result.push('{');
                rest = after;
            }
        }
    }
    result.push_str(rest);
    result
}

/// Laravel Invoice::getVariables (+ Relation/Contact::getVariables), `invoice.` prefixed.
fn variables(d: &Data) -> Vec<(String, String)> {
    let i = &d.invoice;
    let date = |t: Option<chrono::DateTime<chrono::Utc>>| {
        t.map(|t| t.format("%d-%m-%Y").to_string())
            .unwrap_or_default()
    };
    let s = |v: Option<&str>| v.unwrap_or_default().to_owned();
    let total = d.summary.total;
    let total_text = format_decimals_plain(total, d.dp());
    let mut v: Vec<(String, String)> = vec![
        ("id".into(), i.id.to_string()),
        (
            "invoice_nr".into(),
            d.invoice_nr().unwrap_or_else(|| "--draft--".into()),
        ),
        ("status".into(), i.status.clone()),
        ("reference".into(), s(i.reference.as_deref())),
        ("note".into(), s(i.note.as_deref())),
        // shortcut: no payment links until F8 (Mollie); print the link once invoices get one.
        ("payment_link".into(), String::new()),
        ("send_date".into(), date(i.send_date)),
        (
            "payment_term".into(),
            i.send_date
                .map(|t| (chrono::Utc::now() - t).num_days().abs().to_string())
                .unwrap_or_else(|| "0".into()),
        ),
        ("due_date".into(), date(i.due_date)),
        (
            "billing_address".into(),
            i.billing_address
                .clone()
                .and_then(|a| serde_json::from_value::<Address>(a).ok())
                .map(|a| a.formatted())
                .unwrap_or_default(),
        ),
        (
            "total".into(),
            format!(
                "{}{} {}",
                d.currency.symbol_prefix.as_deref().unwrap_or(""),
                total_text,
                d.currency.symbol_suffix.as_deref().unwrap_or("")
            ),
        ),
        ("total_in_decimals".into(), total_text),
    ];
    let contact = i.addressed_contact.as_ref();
    for k in ["id", "full_name", "email", "job_title", "phone_number"] {
        let value = match k {
            "id" => contact.and_then(|c| c.get("uuid")),
            "full_name" => None,
            _ => contact.and_then(|c| c.get(k)),
        };
        let text = match k {
            "full_name" => contact.map(addressed_full_name).unwrap_or_default(),
            _ => value.and_then(Value::as_str).unwrap_or_default().to_owned(),
        };
        v.push((format!("addressed_contact.{k}"), text));
    }
    let customer_ref = i.customer_ref.as_ref();
    let cref = |k: &str| {
        customer_ref
            .and_then(|c| c.get(k))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    for k in ["ordered_by", "po_number", "reference"] {
        v.push((format!("customer_ref.{k}"), cref(k)));
    }
    let (ordered, po, reference) = if d.nl {
        (
            "Besteld door / Opgehaald door",
            "Bestel nummer",
            "general.customer_reference",
        )
    } else {
        (
            "general.ordered_by",
            "general.po_number",
            "general.customer_reference",
        )
    };
    let mut refs = String::from("<br/>");
    for (label, value) in [
        (ordered, cref("ordered_by")),
        (po, cref("po_number")),
        (reference, cref("reference")),
    ] {
        if !value.is_empty() {
            refs.push_str(&format!("{label}: {}<br/>", escape(&value)));
        }
    }
    // Already HTML (Laravel's `<br/>` lines): kept out of the escaping below.
    let refs_key = ("customer_references".to_owned(), refs);
    if let Some(r) = &d.relation {
        let terms = r.terms_and_periods.as_ref();
        let term = |k: &str| {
            terms
                .and_then(|t| t.get(k))
                .map(|t| match t {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    other => other.to_string(),
                })
                .unwrap_or_default()
        };
        let rel: Vec<(&str, String)> = vec![
            ("id", r.id.to_string()),
            ("alias", s(r.alias.as_deref())),
            (
                "relation_nr",
                relation_nr(r.relation_number, r.relation_appendix.as_ref()).unwrap_or_default(),
            ),
            ("company_name", s(r.company_name.as_deref())),
            ("email", s(r.email.as_deref())),
            ("website", s(r.website.as_deref())),
            ("phone_number", s(r.phone_number.as_deref())),
            ("vat_number", s(r.vat_number.as_deref())),
            ("coc_number", s(r.coc_number.as_deref())),
            ("relation_type", d.relation_type.clone().unwrap_or_default()),
            (
                "address",
                relations::address::default_of(&r.addresses)
                    .map(Address::formatted)
                    .unwrap_or_default(),
            ),
            (
                "contact.id",
                d.contact
                    .as_ref()
                    .map(|c| c.id.to_string())
                    .unwrap_or_default(),
            ),
            (
                "contact.full_name",
                d.contact.as_ref().map(full_name).unwrap_or_default(),
            ),
            (
                "contact.email",
                d.contact
                    .as_ref()
                    .and_then(|c| c.email.clone())
                    .unwrap_or_default(),
            ),
            (
                "contact.phone_number",
                d.contact.as_ref().map(contact_phone).unwrap_or_default(),
            ),
            (
                "contact.job_title",
                d.contact
                    .as_ref()
                    .and_then(|c| c.job_title.clone())
                    .unwrap_or_default(),
            ),
            ("invoice_payment_term", term("invoice_payment_term")),
            (
                "credit_invoice_payment_term",
                term("credit_invoice_payment_term"),
            ),
            (
                "collective_invoice_payment_term",
                term("collective_invoice_payment_term"),
            ),
            ("estimate_validity_period", term("estimate_validity_period")),
        ];
        v.extend(
            rel.into_iter()
                .map(|(k, val)| (format!("relation.{k}"), val)),
        );
    }
    let mut out: Vec<(String, String)> = v
        .into_iter()
        .map(|(k, val)| (format!("invoice.{k}"), val))
        .collect();
    out.push((format!("invoice.{}", refs_key.0), refs_key.1));
    out
}

/// `fill` with `customer_references` kept as HTML.
fn fill_text(text: Option<&str>, vars: &[(String, String)]) -> String {
    let Some(text) = text else {
        return String::new();
    };
    let (html, plain): (Vec<_>, Vec<_>) = vars
        .iter()
        .cloned()
        .partition(|(k, _)| k == "invoice.customer_references");
    let mut out = text.to_owned();
    for (k, v) in &html {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    fill(&out, &plain)
}

fn addressed_full_name(c: &Value) -> String {
    let part = |k: &str| {
        c.get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    [part("first_name"), part("middle_name"), part("last_name")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contact_phone(c: &ContactRow) -> String {
    c.phones
        .as_array()
        .and_then(|p| p.first())
        .and_then(|p| p.get("phone_number").or(p.get("number")))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// MoneyHelper::formatDecimals($v, $dp, false): round, keep trailing zeros, "0.00" for zero.
fn format_decimals_plain(v: Decimal, dp: u32) -> String {
    crate::money::format_decimals(Some(v), dp, false, crate::money::Zero::Falsy)
}

fn ucfirst(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// Bytes of a stored file, else None (a file gone from the bucket prints nothing).
async fn file_bytes(
    state: &AppState,
    workspace_id: Uuid,
    id: Option<Uuid>,
) -> Result<Option<(Vec<u8>, String)>, AppError> {
    let (Some(id), Some(bucket)) = (id, state.files.as_ref()) else {
        return Ok(None);
    };
    let Some(row) = files::repo::find(&state.db, workspace_id, id).await? else {
        return Ok(None);
    };
    Ok(bucket.get(&row.key).await?.map(|b| (b, row.content_type)))
}

// --- template layout -------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "pdf/custom.html")]
struct CustomView<'a> {
    css: &'a str,
    logo: Option<String>,
    header_left: String,
    header_right: String,
    header_top: String,
    headers: Vec<Header>,
    rows: Vec<Vec<Cell>>,
    short_rows: Vec<String>,
    summary: Option<SummaryView>,
    footer: String,
}

#[derive(Template)]
#[template(path = "pdf/header.html")]
struct HeaderView {
    background: String,
}

struct Header {
    label: String,
    class: String,
}

struct Cell {
    class: String,
    /// Inner HTML, values escaped.
    html: String,
}

struct SummaryView {
    background: &'static str,
    border: &'static str,
    subtotal_label: &'static str,
    subtotal: String,
    discount_label: &'static str,
    discount: Option<String>,
    excl_label: &'static str,
    excl: String,
    vat_label: &'static str,
    vats: Vec<(String, String)>,
    incl_label: &'static str,
    total: String,
}

fn summary_view(d: &Data, enable: bool, legacy: bool) -> Option<SummaryView> {
    if !enable {
        return None;
    }
    let s = &d.summary;
    let (subtotal, discount, excl, vat, incl) = match (d.nl, legacy) {
        (false, false) => (
            "Subtotal",
            "Total Discount",
            "Totaal excl. Btw",
            "VAT",
            "Totaal incl. Btw",
        ),
        (true, false) => (
            "Subtotaal",
            "Totale korting",
            "Totaal excl. Btw",
            "BTW",
            "Totaal incl. Btw",
        ),
        (false, true) => (
            "Subtotal",
            "Total Discount",
            "Total excl.",
            "VAT",
            "Total incl.",
        ),
        (true, true) => (
            "Subtotaal",
            "Totale korting",
            "Totaal excl.",
            "BTW",
            "Totaal incl.",
        ),
    };
    Some(SummaryView {
        background: d.color.summary_background,
        border: d.color.summary_border,
        subtotal_label: subtotal,
        subtotal: d.money(s.gross),
        discount_label: discount,
        discount: (s.discount > Decimal::ZERO).then(|| d.money(s.discount)),
        excl_label: excl,
        excl: d.money(s.excl),
        vat_label: vat,
        vats: s
            .vats
            .iter()
            .map(|v| (number_format(v.rate, d.dp()), d.money(v.vat)))
            .collect(),
        incl_label: incl,
        total: d.money(s.total),
    })
}

/// Laravel PreviewPdfTemplateData + custom-pdf-template.blade for an invoice.
async fn custom(
    state: &AppState,
    workspace_id: Uuid,
    d: &Data,
    t: &TemplateRow,
) -> Result<(String, Page), AppError> {
    let vars = variables(d);
    let columns: Vec<Value> = t.columns.as_array().cloned().unwrap_or_default();
    let count = columns.len();
    let mut rows = Vec::with_capacity(d.lines.len());
    let mut short_rows = Vec::with_capacity(d.lines.len());
    for line in &d.lines {
        let product = line.product_id.and_then(|p| d.products.get(&p));
        let mut cells = Vec::with_capacity(count);
        let mut short = String::new();
        for (k, col) in columns.iter().enumerate() {
            let alignment = if col["alignment"] == "right" {
                "right"
            } else {
                "left"
            };
            let keys: Vec<&str> = col["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let mut html = String::new();
            for (j, key) in keys.iter().enumerate() {
                let value = field_value(d, &t.domain, line, product, key);
                match *key {
                    "product.short_description" => {
                        if let FieldValue::Html(h) = &value {
                            let class = if j == 0 {
                                "font-medium text-gray-900"
                            } else {
                                "mt-0.5 text-gray-500"
                            };
                            short.push_str(&format!(
                                "<div class=\"{class}\"><div class=\"{class}\">{h}</div></div>"
                            ));
                        }
                    }
                    "product.description" => {
                        if let FieldValue::Html(h) = &value {
                            let class = if j == 0 {
                                "font-medium text-gray-900"
                            } else {
                                "mt-0.5 text-gray-500"
                            };
                            html.push_str(&format!("<div class=\"{class}\">{h}</div>"));
                        }
                    }
                    // shortcut: products have no images in genesy yet; print them once they do.
                    "product.image" | "product.images" => {}
                    _ => {
                        let FieldValue::Text(text) = value else {
                            continue;
                        };
                        if text.is_empty() {
                            continue;
                        }
                        let class = if j == 0 {
                            "font-medium text-gray-900"
                        } else {
                            "inline-block  mt-0.5 text-gray-700"
                        };
                        let slash = if j > 2 { "/" } else { "" };
                        html.push_str(&format!(
                            "<div class=\"{class}\">{slash}  {}</div>",
                            escape(&text)
                        ));
                    }
                }
            }
            let place = if k == 0 {
                "pr-2"
            } else if k + 1 == count {
                "pl-2 pr-1.5"
            } else {
                "px-2"
            };
            let wide = if keys.contains(&"description") {
                "w-1/4"
            } else {
                ""
            };
            cells.push(Cell {
                class: format!("py-1.5 {place} {wide} text-{alignment}"),
                html,
            });
        }
        rows.push(cells);
        short_rows.push(short);
    }
    let headers = columns
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let alignment = if c["alignment"] == "right" {
                "right"
            } else {
                "left"
            };
            let place = if k == 0 {
                "pr-2"
            } else if k + 1 == count {
                "pl-2 pr-1.5"
            } else {
                "px-2"
            };
            Header {
                label: c["label"].as_str().unwrap_or_default().to_owned(),
                class: format!(
                    "py-2 {place} text-{alignment} text-sm font-semibold text-xs {}",
                    d.color.item_header
                ),
            }
        })
        .collect();
    let logo = file_bytes(state, workspace_id, t.logo_file_id)
        .await?
        .map(|(b, kind)| {
            format!(
                "data:{kind};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(b)
            )
        });
    let background = file_bytes(state, workspace_id, t.background_file_id)
        .await?
        .map(|(b, _)| base64::engine::general_purpose::STANDARD.encode(b));
    let view = CustomView {
        css: TAILWIND,
        logo,
        header_left: fill_text(t.header_left.as_deref(), &vars),
        header_right: fill_text(t.header_right.as_deref(), &vars),
        header_top: fill_text(t.header_top.as_deref(), &vars),
        headers,
        rows: if d.lines.is_empty() { Vec::new() } else { rows },
        short_rows,
        summary: summary_view(d, t.enable_summary, false),
        footer: fill_text(t.footer.as_deref(), &vars),
    };
    let html = view
        .render()
        .map_err(|e| AppError::Internal(format!("pdf template: {e}")))?;
    let page = match background {
        Some(background) => Page {
            margins_mm: margins(t),
            print_background: true,
            header_html: Some(
                HeaderView { background }
                    .render()
                    .map_err(|e| AppError::Internal(format!("pdf header: {e}")))?,
            ),
            footer_html: Some("<div style=\"page-break-before:always\">&nbsp;</div>".into()),
        },
        None => Page {
            margins_mm: margins(t),
            print_background: false,
            header_html: None,
            footer_html: None,
        },
    };
    Ok((html, page))
}

/// Browsershot `margins(top, x, bottom, x)` in millimetres.
fn margins(t: &TemplateRow) -> [u32; 4] {
    let mm = |v: i32| u32::try_from(v).unwrap_or(0);
    [
        mm(t.padding_top),
        mm(t.padding_x),
        mm(t.padding_bottom),
        mm(t.padding_x),
    ]
}

/// A column field's printed value.
enum FieldValue {
    Text(String),
    /// Product description HTML (the catalog's own rich text).
    Html(String),
    None,
}

/// Laravel getItemValue + checkFieldType for an invoice line.
fn field_value(
    d: &Data,
    domain: &str,
    l: &ItemRow,
    p: Option<&ProductRow>,
    key: &str,
) -> FieldValue {
    let Some(kind) = fields::kind(domain, key) else {
        return FieldValue::None;
    };
    let hundred = Decimal::ONE_HUNDRED;
    let mul = |a: Decimal, b: Decimal| a.checked_mul(b).unwrap_or_default();
    let vat_calc = Decimal::ONE
        .checked_add(l.vat.checked_div(hundred).unwrap_or_default())
        .unwrap_or_default();
    let sum = mul(l.quantity, l.price);
    let discount = mul(
        sum,
        l.discount_percentage
            .checked_div(hundred)
            .unwrap_or_default(),
    );
    let total = sum.checked_sub(discount).unwrap_or_default();
    let text = |v: Option<&str>| v.unwrap_or_default().to_owned();
    let gross = p.map(|p| p.gross_price).unwrap_or_default();
    let number: Option<Decimal> = match key {
        "quantity" => Some(l.quantity),
        "price" => Some(l.price),
        "price_incl" => Some(mul(l.price, vat_calc)),
        "gross_price" => Some(gross),
        "gross_price_incl_vat" => Some(mul(gross, vat_calc)),
        "vat" => Some(l.vat),
        "vat_amount" => Some(mul(total, vat_calc).checked_sub(total).unwrap_or_default()),
        "discount_percentage" => Some(l.discount_percentage),
        "discount_amount" => Some(discount),
        "subtotal" => Some(total),
        "total" => Some(
            total
                .checked_add(mul(total, vat_calc).checked_sub(total).unwrap_or_default())
                .unwrap_or_default(),
        ),
        "product.weight" => p.and_then(|p| p.weight),
        _ => None,
    };
    let prefix = |pre: &str, v: Option<&str>| {
        v.filter(|s| !s.is_empty())
            .map(|s| format!("{pre}{s}"))
            .unwrap_or_default()
    };
    let raw = match key {
        "title" => l.title.clone(),
        "description" => text(l.description.as_deref()),
        "product.product_nr" => p
            .map(|p| p.product_nr_formatted.clone())
            .unwrap_or_default(),
        "product.hs_code" => prefix("HS: ", p.and_then(|p| p.hs_code.as_deref())),
        "product.country_of_origin" => {
            prefix("CO: ", p.and_then(|p| p.country_of_origin.as_deref()))
        }
        "product.ean" => text(p.and_then(|p| p.ean13.as_deref())),
        "product.reference" => text(p.and_then(|p| p.reference.as_deref())),
        "product.reference_vendor" => text(p.and_then(|p| p.reference_vendor.as_deref())),
        "product.dimensions" => p
            .and_then(|p| p.dimensions.as_ref())
            .map(dimensions)
            .unwrap_or_default(),
        "product.description" | "product.short_description" => {
            let html = match key {
                "product.description" => p.and_then(|p| p.description.clone()),
                _ => p.and_then(|p| p.short_description.clone()),
            };
            return html
                .filter(|h| !h.is_empty())
                .map_or(FieldValue::None, FieldValue::Html);
        }
        _ => String::new(),
    };
    let symbol = d.currency.symbol_prefix.as_deref().unwrap_or("");
    let n = number.unwrap_or_default();
    FieldValue::Text(match kind.as_str() {
        "currency" if !n.is_zero() => format!("{symbol} {}", number_format(n, d.dp())),
        "currency" => String::new(),
        "negative_currency" => format!("- {symbol} {}", number_format(n, d.dp())),
        "percentage" if n > Decimal::ZERO => format!("{} %", number_format(n, 2)),
        "percentage" => String::new(),
        "number" if !n.is_zero() => number_format(n, 2),
        "number" => String::new(),
        "weight" if n > Decimal::ZERO => format!("{} KG", number_format(n, 2)),
        "weight" => String::new(),
        _ => raw,
    })
}

/// Laravel ProductDimension::format: `L10 CM - B20 CM - H30 CM`.
fn dimensions(v: &Value) -> String {
    let n = |k: &str| {
        v.get(k)
            .and_then(|x| {
                x.as_f64()
                    .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
            })
            .filter(|x| *x != 0.0)
    };
    let show = |x: f64| {
        if x.fract() == 0.0 {
            format!("{}", x as i64)
        } else {
            x.to_string()
        }
    };
    let (l, w, h) = (n("length"), n("width"), n("height"));
    let mut out = String::new();
    if let Some(l) = l {
        out.push_str(&format!("L{} CM ", show(l)));
    }
    if l.is_some() && w.is_some() {
        out.push_str("- ");
    }
    if let Some(w) = w {
        out.push_str(&format!("B{} CM ", show(w)));
    }
    if w.is_some() && h.is_some() {
        out.push_str("- ");
    }
    if let Some(h) = h {
        out.push_str(&format!("H{} CM", show(h)));
    }
    out
}

// --- built-in layout -------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "pdf/invoice.html")]
struct LegacyView<'a> {
    css: &'a str,
    t: LegacyText,
    color: &'a Colors,
    is_credit: bool,
    invoice_nr: String,
    logo: Option<String>,
    send_date: Option<String>,
    reference: Option<String>,
    relation_nr: String,
    due_date: String,
    from: Vec<String>,
    company: String,
    to_name: String,
    to_contact: String,
    to_addresses: Vec<[String; 3]>,
    items: Vec<LegacyItem>,
    summary: Option<SummaryView>,
    footer: Option<String>,
}

struct LegacyText {
    invoice: &'static str,
    credit_invoice: &'static str,
    send_date: &'static str,
    reference: &'static str,
    relation: &'static str,
    due_date: &'static str,
    from: &'static str,
    to: &'static str,
    item_name: &'static str,
    quantity: &'static str,
    amount: &'static str,
    vat: &'static str,
    discount: &'static str,
    total: &'static str,
}

struct LegacyItem {
    title: String,
    description: Option<String>,
    quantity: String,
    price: String,
    vat: String,
    discount: Option<(String, String)>,
    total: String,
}

/// Laravel `invoice.blade.php` (no template chosen).
async fn legacy(state: &AppState, workspace_id: Uuid, d: &Data) -> Result<String, AppError> {
    let t = if d.nl {
        LegacyText {
            invoice: "Factuur",
            credit_invoice: "Creditfactuur",
            send_date: "Verzenddatum",
            reference: "Referentie",
            relation: "Relatienummer",
            due_date: "Vervaldatum",
            from: "VAN",
            to: "AAN",
            item_name: "Omschrijving",
            quantity: "Aantal",
            amount: "Prijs",
            vat: "BTW",
            discount: "Korting",
            total: "Totaal",
        }
    } else {
        LegacyText {
            invoice: "Invoice",
            credit_invoice: "Credit Invoice",
            send_date: "Send date",
            reference: "Reference",
            relation: "Relation Number",
            due_date: "Due date",
            from: "FROM",
            to: "TO",
            item_name: "Description",
            quantity: "Quantity",
            amount: "Price",
            vat: "VAT",
            discount: "Discount",
            total: "Total",
        }
    };
    let profile = workspaces::repo::record(&state.db, workspace_id)
        .await?
        .and_then(|w| w.profile)
        .unwrap_or(Value::Null);
    let p = |path: &[&str]| {
        path.iter()
            .try_fold(&profile, |v, k| v.get(*k))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let from = vec![
        format!(
            "{} {}",
            p(&["address", "address"]),
            p(&["address", "house_number"])
        ),
        format!(
            "{}  {}",
            p(&["address", "zipcode"]),
            p(&["address", "city"])
        ),
        format!("{} ", p(&["address", "country"])),
    ];
    let footer = settings::repo::section(&state.db, workspace_id, "invoice")
        .await?
        .and_then(|s| {
            s.get("default_footer_message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .filter(|f| !f.is_empty());
    let r = d.relation.as_ref();
    let items = d
        .lines
        .iter()
        .map(|l| {
            let sum = l.quantity.checked_mul(l.price).unwrap_or_default();
            let discount = sum
                .checked_mul(
                    l.discount_percentage
                        .checked_div(Decimal::ONE_HUNDRED)
                        .unwrap_or_default(),
                )
                .unwrap_or_default();
            LegacyItem {
                title: l.title.clone(),
                description: l.description.clone().filter(|s| !s.is_empty()),
                quantity: crate::money::format_decimals(
                    Some(l.quantity),
                    2,
                    true,
                    crate::money::Zero::Falsy,
                ),
                price: d.money(l.price),
                vat: format!("{} %", number_format(l.vat, d.dp())),
                discount: (l.discount_percentage > Decimal::ZERO).then(|| {
                    (
                        format!("{} %", number_format(l.discount_percentage, d.dp())),
                        d.money(discount),
                    )
                }),
                total: d.money(sum),
            }
        })
        .collect();
    let view = LegacyView {
        css: TAILWIND,
        color: &d.color,
        is_credit: d.invoice.is_credit,
        invoice_nr: d.invoice_nr().unwrap_or_default(),
        // shortcut: workspace logos aren't ported yet; the built-in layout prints none.
        logo: None,
        send_date: d
            .invoice
            .send_date
            .map(|t| t.format("%d-%m-%Y").to_string()),
        reference: d.invoice.reference.clone(),
        relation_nr: r
            .and_then(|r| relation_nr(r.relation_number, r.relation_appendix.as_ref()))
            .unwrap_or_default(),
        due_date: d
            .invoice
            .due_date
            .map(|t| t.format("%d-%m-%Y").to_string())
            .unwrap_or_default(),
        from,
        company: p(&["company_name"]),
        to_name: r.and_then(|r| r.company_name.clone()).unwrap_or_default(),
        to_contact: d.contact.as_ref().map(full_name).unwrap_or_default(),
        to_addresses: r
            .map(|r| {
                r.addresses
                    .iter()
                    .map(|a| {
                        let s = |v: &Option<String>| v.clone().unwrap_or_default();
                        [
                            format!("{} {}", s(&a.address), s(&a.house_number)),
                            format!("{}  {}", s(&a.zipcode), s(&a.city)),
                            format!("{} ", s(&a.country)),
                        ]
                    })
                    .collect()
            })
            .unwrap_or_default(),
        items,
        summary: summary_view(d, true, true),
        footer,
        t,
    };
    view.render()
        .map_err(|e| AppError::Internal(format!("pdf invoice: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_format_like_php() {
        let d = |s: &str| s.parse::<Decimal>().unwrap();
        assert_eq!(number_format(d("1234567.891"), 2), "1.234.567,89");
        assert_eq!(number_format(d("0.005"), 2), "0,01");
        assert_eq!(number_format(d("-1234.5"), 2), "-1.234,50");
        assert_eq!(number_format(d("21"), 0), "21");
        assert_eq!(number_format(d("-0.001"), 2), "0,00");
    }

    #[test]
    fn placeholders_fill_escaped_and_unknown_become_dashes() {
        let vars = vec![
            ("invoice.reference".to_owned(), "<b>R&D</b>".to_owned()),
            ("invoice.total".to_owned(), "€12,10".to_owned()),
        ];
        assert_eq!(
            fill(
                "<p>{invoice.reference} {invoice.total} {nope} {a\nb}</p>",
                &vars
            ),
            "<p>&lt;b&gt;R&amp;D&lt;/b&gt; €12,10 - {a\nb}</p>"
        );
        assert_eq!(fill("{", &vars), "{");
    }

    #[test]
    fn dimensions_like_laravel() {
        let v = serde_json::json!({ "length": 10, "width": 20.5, "height": 30 });
        assert_eq!(dimensions(&v), "L10 CM - B20.5 CM - H30 CM");
        assert_eq!(dimensions(&serde_json::json!({ "height": 3 })), "H3 CM");
        assert_eq!(dimensions(&serde_json::json!({})), "");
    }
}
