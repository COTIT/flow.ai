//! PDF templates (`/settings/pdf-templates`) and invoice PDFs (`/invoices/{id}/preview`),
//! final batch F5a. A local fake Gotenberg records the HTML it gets and answers a stub PDF;
//! files go to an in-memory store.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, fake_pdf, member_with};
use genesy_360_api::{build_app, mail::Mailer, outbound::Outbound, pdf::Renderer, storage::Store};
use serde_json::{Value, json};
use sqlx::PgPool;

const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
/// A 1×1 GIF.
const GIF_B64: &str = "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";

/// Templates API with a renderer and a readable store.
async fn setup(pool: PgPool) -> (Api, Store, fake_pdf::Seen) {
    setup_with(pool, b"%PDF-1.4 fake".to_vec()).await
}

/// `setup` with a sidecar that answers `answer`.
async fn setup_with(pool: PgPool, answer: Vec<u8>) -> (Api, Store, fake_pdf::Seen) {
    let (url, seen) = fake_pdf::start(answer).await;
    let files = Store::memory();
    let mut state = common::state(pool.clone(), Mailer::capture().0);
    state.files = Some(files.clone());
    state.pdf = Some(Renderer::new(
        &url,
        Outbound::new(&["127.0.0.1".to_owned()]).unwrap(),
    ));
    let mut api = Api::new(pool, "/settings/pdf-templates").await;
    api.app = build_app(state);
    (api, files, seen)
}

fn template() -> Value {
    json!({
        "name": "Default invoice", "domain": "invoice",
        "columns": [
            { "label": "Item", "alignment": "left", "fields": ["title", "product.product_nr"] },
            { "label": "Qty", "alignment": "right", "fields": ["quantity"] },
            { "label": "Total", "alignment": "right", "fields": ["total"] },
        ],
        "header_left": "<p>Ref {invoice.reference} for {invoice.relation.company_name} {nope}</p>",
        "header_right": "<p>{invoice.invoice_nr}</p>", "footer": "<p>Pay within {invoice.relation.invoice_payment_term} days</p>",
        "padding_top": 10, "padding_bottom": 12.5, "padding_x": 8,
        "enable_summary": true, "default": true,
        "conditions": { "language": "nl", "relation_type": "business" },
        "extra_fields": { "display_description": true },
    })
}

async fn post(api: &Api, path: &str, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, path, Some(body), &api.owner).await
}

async fn create(api: &Api, body: Value) -> String {
    let (status, res) = post(api, "", body).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"]["pdf_template_id"].as_str().unwrap().to_owned()
}

async fn error(api: &Api, path: &str, body: Value, field: &str) -> String {
    let (status, res) = post(api, path, body.clone()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
    res["errors"][field][0]
        .as_str()
        .unwrap_or_else(|| panic!("no error for {field}: {res}"))
        .to_owned()
}

async fn upload(api: &Api, name: &str, b64: &str) -> String {
    api.with_base("/files")
        .create(
            json!({ "upload": b64, "document_name": name, "public": 1 }),
            "file_id",
        )
        .await
}

/// A relation "Acme <B.V.>" and an invoice with a tiny line, reference "R&D".
async fn invoice(api: &Api) -> String {
    let rel = api
        .with_base("/relations")
        .create(
            json!({ "company_name": "Acme <B.V.>", "type": "business" }),
            "relation_id",
        )
        .await;
    api.with_base("/invoices")
        .create(
            json!({ "relation_id": rel, "reference": "R&D",
                    "items": [{ "title": "Tiny", "quantity": 3, "price": 0.07, "vat": 21 }] }),
            "invoice_id",
        )
        .await
}

// --- CRUD ----------------------------------------------------------------------------------

#[sqlx::test]
async fn create_get_update_list_delete(pool: PgPool) {
    let (api, _, _) = setup(pool).await;
    let logo = upload(&api, "logo.png", PNG_B64).await;
    let mut body = template();
    body["logo_file_id"] = json!(logo);
    let id = create(&api, body).await;

    let t = api.get(&id).await;
    assert_eq!(t["name"], "Default invoice");
    assert_eq!(t["domain"], "invoice");
    assert_eq!(t["padding_bottom"], 13, "rounded like Laravel's int column");
    assert_eq!(t["enable_summary"], 1);
    assert_eq!(t["default"], true);
    assert_eq!(
        t["columns"][1],
        json!({ "label": "Qty", "alignment": "right", "fields": ["quantity"] })
    );
    assert_eq!(
        t["conditions"],
        json!({ "language": "nl", "relation_type": "business", "relation_custom_type_id": null })
    );
    assert_eq!(t["extra_fields"], json!({ "display_description": true }));
    assert_eq!(t["relationships"]["logoFile"]["id"], logo);
    assert!(
        t["relationships"]["logoFile"]["file_path"]
            .as_str()
            .unwrap()
            .starts_with("memory://")
    );
    assert_eq!(t["relationships"]["backgroundFile"], Value::Null);

    let mut update = template();
    update["name"] = json!("Renamed");
    update["domain"] = json!("order");
    update["logo_file_id"] = Value::Null;
    update.as_object_mut().unwrap().remove("footer");
    let (status, res) = post(&api, &format!("/{id}"), update).await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let t = api.get(&id).await;
    assert_eq!(t["name"], "Renamed");
    assert_eq!(t["domain"], "invoice", "the domain can't change");
    assert_eq!(t["relationships"]["logoFile"], Value::Null, "null clears");
    assert!(
        t["footer"].as_str().unwrap().contains("Pay within"),
        "left out keeps it"
    );

    create(&api, json!({ "name": "Order", "domain": "order", "padding_top": 0, "padding_bottom": 0,
                         "padding_x": 0, "columns": [{ "alignment": "left", "fields": ["title"] }] }))
        .await;
    let count = |v: &Value| v["data"].as_array().unwrap().len();
    assert_eq!(count(&api.list("").await), 2);
    let invoices = api.list("?domain=invoice").await;
    assert_eq!(count(&invoices), 1);
    assert_eq!(invoices["data"][0]["id"], id);
    assert_eq!(invoices["data"][0]["name"], "Renamed");
    assert_eq!(invoices["data"][0]["domain"], "invoice");
    assert_eq!(count(&api.list("?q=renam").await), 1);
    assert_eq!(
        count(&api.list("?language=nl").await),
        2,
        "an unset condition matches"
    );
    assert_eq!(count(&api.list("?language=en").await), 1);

    let (status, _) = api
        .call(Method::DELETE, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn rules(pool: PgPool) {
    let (api, _, _) = setup(pool).await;
    let with = |k: &str, v: Value| {
        let mut b = template();
        b[k] = v;
        b
    };
    let cases = [
        (
            with("domain", json!("nope")),
            "domain",
            "The selected domain is invalid.",
        ),
        (
            with("name", Value::Null),
            "name",
            "The name field is required.",
        ),
        (
            with("columns", json!([])),
            "columns",
            "The columns field is required.",
        ),
        (
            with(
                "columns",
                json!([{ "alignment": "middle", "fields": ["title"] }]),
            ),
            "columns.0.alignment",
            "The selected columns.0.alignment is invalid.",
        ),
        (
            with(
                "columns",
                json!([{ "alignment": "left", "fields": ["title", "order.status"] }]),
            ),
            "columns.0.fields.1",
            "The selected columns.0.fields.1 is invalid.",
        ),
        (
            with(
                "columns",
                json!([{ "label": "x".repeat(56), "alignment": "left", "fields": ["title"] }]),
            ),
            "columns.0.label",
            "The columns.0.label must not be greater than 55 characters.",
        ),
        (
            with("padding_top", json!(41)),
            "padding_top",
            "The padding top must not be greater than 40.",
        ),
        (
            with("padding_x", Value::Null),
            "padding_x",
            "The padding x field is required.",
        ),
        (
            with(
                "logo_file_id",
                json!("01900000-0000-7000-8000-000000000001"),
            ),
            "logo_file_id",
            "The selected logo file id is invalid.",
        ),
        (
            with("conditions", json!({ "relation_type": "alien" })),
            "conditions.relation_type",
            "The selected conditions.relation type is invalid.",
        ),
        (
            with(
                "conditions",
                json!({ "relation_custom_type_id": "01900000-0000-7000-8000-000000000001" }),
            ),
            "conditions.relation_custom_type_id",
            "The selected conditions.relation custom type id is invalid.",
        ),
        (
            with("extra_fields", json!({ "display_hours": "maybe" })),
            "extra_fields.display_hours",
            "The extra fields.display hours field must be true or false.",
        ),
    ];
    for (body, field, msg) in cases {
        assert_eq!(error(&api, "", body.clone(), field).await, msg, "{body}");
    }
    // Logo and background are inlined into every page: images of up to 2 MB only.
    use base64::Engine;
    let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    let pdf = upload(&api, "a.pdf", &b64(b"%PDF-1.4")).await;
    let png = [b"\x89PNG\r\n\x1a\n".as_slice(), &vec![0u8; 2 * 1024 * 1024]].concat();
    let big = upload(&api, "big.png", &b64(&png)).await;
    assert_eq!(
        error(&api, "", with("logo_file_id", json!(pdf)), "logo_file_id").await,
        "The logo file id must be an image."
    );
    assert_eq!(
        error(
            &api,
            "",
            with("background_file_id", json!(big)),
            "background_file_id"
        )
        .await,
        "The background file id must not be greater than 2048 kilobytes."
    );
}

#[sqlx::test]
async fn available_fields_are_laravels(pool: PgPool) {
    let (api, _, _) = setup(pool).await;
    let (status, res) = api
        .call(Method::GET, "/available-fields/invoice", None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK);
    let fields = res["data"]["fields"].as_object().unwrap();
    assert_eq!(fields.len(), 25);
    assert_eq!(fields["price"]["type"], "currency");
    assert_eq!(
        res["data"]["variables"]["model"]["invoice.invoice_nr"],
        "invoice number"
    );
    let (_, unknown) = api
        .call(Method::GET, "/available-fields/nope", None, &api.owner)
        .await;
    assert_eq!(
        unknown,
        json!({ "data": { "fields": [], "variables": [] } })
    );
}

// --- rendering -----------------------------------------------------------------------------

#[sqlx::test]
async fn a_template_preview_renders_the_newest_invoice(pool: PgPool) {
    let (api, files, seen) = setup(pool).await;
    let mut body = template();
    body["logo_file_id"] = json!(upload(&api, "logo.png", PNG_B64).await);
    body["background_file_id"] = json!(upload(&api, "bg.png", GIF_B64).await);
    let id = create(&api, body).await;
    let (status, res) = api
        .call(Method::GET, &format!("/{id}/preview"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res,
        json!({ "message": "Create an invoice to preview this template." })
    );

    invoice(&api).await;
    let (status, res) = api
        .call(Method::GET, &format!("/{id}/preview"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let file = &res["data"];
    assert!(
        file["document_name"]
            .as_str()
            .unwrap()
            .starts_with("test-preview-pdf-template-")
    );
    let key = file["file_path"]
        .as_str()
        .unwrap()
        .trim_start_matches("memory://")
        .split('?')
        .next()
        .unwrap();
    assert_eq!(files.read(key).unwrap().0, b"%PDF-1.4 fake");

    let html = seen.lock().unwrap().pop().unwrap();
    // Values put into the user's HTML are escaped; unknown placeholders print a dash.
    assert!(
        html.contains("<p>Ref R&amp;D for Acme &lt;B.V.&gt; -</p>"),
        "{html}"
    );
    assert!(html.contains("--draft--"));
    // User HTML can't load anything: index.html and header.html both pin a CSP.
    let csp = r#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; font-src data:; style-src 'unsafe-inline'">"#;
    assert_eq!(html.matches(csp).count(), 2, "{html}");
    assert!(
        html.contains(&format!("data:image/png;base64,{PNG_B64}")),
        "logo inlined"
    );
    assert!(
        html.contains("header.html") && html.contains(&format!("data:image/gif;base64,{GIF_B64}")),
        "background in the page header, with its own type"
    );
    assert!(html.contains("Tiny") && html.contains("0,25"));
    assert!(html.contains("Subtotal") || html.contains("Subtotaal"));
    for field in [
        "paperWidth",
        "210mm",
        "marginTop",
        "10mm",
        "marginBottom",
        "13mm",
        "marginLeft",
        "8mm",
    ] {
        assert!(html.contains(field), "{field}");
    }
}

#[sqlx::test]
async fn an_invoice_preview_uses_the_chosen_template_or_the_built_in_layout(pool: PgPool) {
    let (api, _, seen) = setup(pool).await;
    let inv = invoice(&api).await;
    let invoices = api.with_base("/invoices");
    let (status, res) = invoices
        .call(Method::GET, &format!("/{inv}/preview"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert!(
        res["data"]["document_name"]
            .as_str()
            .unwrap()
            .starts_with("Invoice_pdf_")
    );
    let html = seen.lock().unwrap().pop().unwrap();
    assert!(
        html.contains("Invoice") && html.contains("Acme &#60;B.V.&#62;") && html.contains("Tiny"),
        "{html}"
    );
    assert!(
        html.contains("Content-Security-Policy"),
        "built-in layout pins the CSP too"
    );

    let tpl = create(&api, template()).await;
    let (status, res) = invoices
        .call(
            Method::GET,
            &format!("/{inv}/preview?pdf_template_id={tpl}"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert!(
        res["data"]["document_name"]
            .as_str()
            .unwrap()
            .starts_with("Invoice_")
    );
    assert!(seen.lock().unwrap().pop().unwrap().contains("Ref R&amp;D"));

    let (status, res) = invoices
        .call(
            Method::GET,
            &format!("/{inv}/preview?pdf_template_id=nope"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (status, _) = invoices
        .call(
            Method::GET,
            "/01900000-0000-7000-8000-000000000001/preview",
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A runaway answer (over 25 MiB) is cut off: a 500, and no file is stored.
#[sqlx::test]
async fn an_oversized_pdf_is_refused(pool: PgPool) {
    let big = [b"%PDF".as_slice(), &vec![0u8; 25 * 1024 * 1024]].concat();
    let (api, _, _) = setup_with(pool, big).await;
    let inv = invoice(&api).await;
    let (status, _) = api
        .with_base("/invoices")
        .call(Method::GET, &format!("/{inv}/preview"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM files")
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(stored, 0);
}

/// A sidecar that fails is a 503 the client may retry.
#[sqlx::test]
async fn a_failing_sidecar_is_503(pool: PgPool) {
    let (api, _, _) = setup_with(pool, Vec::new()).await;
    let inv = invoice(&api).await;
    let (status, res) = api
        .with_base("/invoices")
        .call(Method::GET, &format!("/{inv}/preview"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{res}");
    assert_eq!(
        res,
        json!({ "message": "PDF rendering failed. Try again later." })
    );
}

/// HTML the sidecar refuses (it reached off the allow-list) is the template's fault: 422.
#[sqlx::test]
async fn a_refused_render_is_422(pool: PgPool) {
    let (api, _, _) = setup_with(pool, b"refuse".to_vec()).await;
    let inv = invoice(&api).await;
    let (status, res) = api
        .with_base("/invoices")
        .call(Method::GET, &format!("/{inv}/preview"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res, json!({ "message": genesy_360_api::pdf::REFUSED }));
}

#[sqlx::test]
async fn without_the_sidecar_previews_are_503(pool: PgPool) {
    let mut api = Api::new(pool.clone(), "/invoices").await;
    let inv = invoice(&api).await;
    let mut state = common::state(pool, Mailer::capture().0);
    state.pdf = None;
    api.app = build_app(state);
    let (status, res) = api
        .call(Method::GET, &format!("/{inv}/preview"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{res}");
    assert_eq!(res, json!({ "message": "PDF rendering isn't configured." }));
}

// --- access --------------------------------------------------------------------------------

#[sqlx::test]
async fn access_rules(pool: PgPool) {
    let (api, _, _) = setup(pool).await;
    let id = create(&api, template()).await;
    api.assert_member_forbidden(&id).await;
    let perms = [
        "pdftemplate.list",
        "pdftemplate.view",
        "pdftemplate.create",
        "pdftemplate.update",
        "pdftemplate.delete",
    ];
    let carl = member_with(&api.app, &api.pool, &api.ws, "carl@example.com", &perms).await;
    let (status, res) = api.call(Method::POST, "", Some(template()), &carl).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    api.assert_foreign_404(&id, template()).await;

    // Eve's file and Eve's template are invisible to this workspace.
    let (eve, eve_ws) = common::signup(&api.app, "eve2@example.com", "evil2").await;
    let uri = format!("/v1/workspace/{eve_ws}/files");
    let body = json!({ "upload": PNG_B64, "document_name": "x.png", "public": 1 });
    let (_, res) = common::send(&api.app, Method::POST, &uri, Some(body), Some(&eve)).await;
    let mut body = template();
    body["logo_file_id"] = res["data"]["file_id"].clone();
    assert_eq!(
        error(&api, "", body, "logo_file_id").await,
        "The selected logo file id is invalid."
    );
    let uri = format!("/v1/workspace/{eve_ws}/settings/pdf-templates");
    let (_, res) = common::send(&api.app, Method::POST, &uri, Some(template()), Some(&eve)).await;
    let foreign = res["data"]["pdf_template_id"].as_str().unwrap();
    let inv = invoice(&api).await;
    let (status, res) = api
        .with_base("/invoices")
        .call(
            Method::GET,
            &format!("/{inv}/preview?pdf_template_id={foreign}"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert!(
        res["data"]["document_name"]
            .as_str()
            .unwrap()
            .starts_with("Invoice_pdf_"),
        "built-in layout"
    );
}
