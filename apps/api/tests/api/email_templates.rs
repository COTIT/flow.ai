//! /v1/workspace/{ws}/email-templates (final batch F5b): CRUD, one default per domain, list
//! filters and the variables a domain offers.

use crate::common;

use axum::http::{Method, StatusCode};
use common::Api;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn api(pool: PgPool) -> Api {
    Api::new(pool, "/email-templates").await
}

fn template(name: &str, domain: &str, default: bool) -> Value {
    json!({
        "template_name": name, "template_domain": domain, "default_template": default,
        "subject": "Invoice {invoice.invoice_nr}", "body": "<p>Dear {invoice.relation.company_name}</p>",
        "conditions": { "language": "nl", "relation_type": "business" },
        "meta": { "default_for": "send_invoice" },
    })
}

async fn post(api: &Api, path: &str, body: Value) -> (StatusCode, Value) {
    api.call(Method::POST, path, Some(body), &api.owner).await
}

async fn defaults(api: &Api) -> Vec<(String, bool)> {
    sqlx::query_as(
        "SELECT template_name, default_template FROM email_templates ORDER BY created_at",
    )
    .fetch_all(&api.pool)
    .await
    .unwrap()
}

#[sqlx::test]
async fn create_get_update_delete(pool: PgPool) {
    let api = api(pool).await;
    let id = api
        .create(template("Invoice NL", "Invoice", true), "email_template_id")
        .await;
    let t = api.get(&id).await;
    assert_eq!(t["id"], id);
    assert_eq!(t["template_name"], "Invoice NL");
    assert_eq!(
        t["template_domain"], "invoice",
        "lowercased like Laravel's enum"
    );
    assert_eq!(t["default_template"], true);
    assert_eq!(t["subject"], "Invoice {invoice.invoice_nr}");
    assert_eq!(
        t["conditions"],
        json!({ "language": "nl", "relation_type": "business", "relation_custom_type_id": null })
    );
    assert_eq!(t["meta"], json!({ "default_for": "send_invoice" }));
    assert!(t["created_at"].is_string() && t["last_updated_at"].is_string());

    // Update: body required, the domain can't change, left out keeps, null clears conditions.
    let (status, res) = post(
        &api,
        &format!("/{id}"),
        json!({ "template_domain": "order", "body": "<p>New</p>", "conditions": null }),
    )
    .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let t = api.get(&id).await;
    assert_eq!(
        (
            t["template_domain"].as_str(),
            t["body"].as_str(),
            t["subject"].as_str()
        ),
        (
            Some("invoice"),
            Some("<p>New</p>"),
            Some("Invoice {invoice.invoice_nr}")
        )
    );
    assert_eq!(t["conditions"], Value::Null);
    let (status, res) = post(&api, &format!("/{id}"), json!({ "subject": "x" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"]["body"][0], "The body field is required.");

    api.delete(&id).await;
    let (status, _) = api
        .call(Method::GET, &format!("/{id}"), None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// One default per domain: setting one (on create or update) clears the others of that domain.
#[sqlx::test]
async fn one_default_per_domain(pool: PgPool) {
    let api = api(pool).await;
    let a = api
        .create(template("A", "invoice", true), "email_template_id")
        .await;
    api.create(template("B", "invoice", true), "email_template_id")
        .await;
    api.create(template("C", "estimate", true), "email_template_id")
        .await;
    assert_eq!(
        defaults(&api).await,
        [("A".into(), false), ("B".into(), true), ("C".into(), true)]
    );
    let (status, _) = post(
        &api,
        &format!("/{a}"),
        json!({ "body": "x", "default_template": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        defaults(&api).await,
        [("A".into(), true), ("B".into(), false), ("C".into(), true)]
    );

    // Ten parallel creates of a default leave exactly one.
    let tasks: Vec<_> = (0..10)
        .map(|i| {
            let api = api.with_base("/email-templates");
            tokio::spawn(async move {
                post(&api, "", template(&format!("P{i}"), "order", true))
                    .await
                    .0
            })
        })
        .collect();
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::CREATED);
    }
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM email_templates WHERE template_domain = 'order' AND default_template",
    )
    .fetch_one(&api.pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn rules(pool: PgPool) {
    let api = api(pool).await;
    let with = |k: &str, v: Value| {
        let mut b = template("A", "invoice", false);
        b[k] = v;
        b
    };
    let cases = [
        (
            with("template_name", Value::Null),
            "template_name",
            "The template name field is required.",
        ),
        (
            with("template_domain", Value::Null),
            "template_domain",
            "The template domain field is required.",
        ),
        (
            with("template_domain", json!("nope")),
            "template_domain",
            "The selected template domain: nope is invalid.",
        ),
        (
            with("default_template", Value::Null),
            "default_template",
            "The default template field is required.",
        ),
        (
            with("default_template", json!("maybe")),
            "default_template",
            "The default template field must be true or false.",
        ),
        (
            with("subject", json!("x".repeat(256))),
            "subject",
            "The subject must not be greater than 255 characters.",
        ),
        (
            with("body", Value::Null),
            "body",
            "The body field is required.",
        ),
        (
            with("body", json!("x".repeat(25_001))),
            "body",
            "The body must not be greater than 25000 characters.",
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
    ];
    for (body, field, msg) in cases {
        let (status, res) = post(&api, "", body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert_eq!(res["errors"][field][0], msg, "{res}");
    }
}

#[sqlx::test]
async fn list_filters_and_pages(pool: PgPool) {
    let api = api(pool).await;
    api.create(
        template("Invoice NL", "invoice", false),
        "email_template_id",
    )
    .await;
    let mut en = template("Invoice EN", "invoice", false);
    en["conditions"] = json!({ "language": "en" });
    api.create(en, "email_template_id").await;
    let mut any = template("Order any", "order", false);
    any["conditions"] = Value::Null;
    api.create(any, "email_template_id").await;
    let names = |v: &Value| -> Vec<String> {
        v["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["template_name"].as_str().unwrap().to_owned())
            .collect()
    };
    let all = api.list("").await;
    assert_eq!(
        names(&all),
        ["Order any", "Invoice EN", "Invoice NL"],
        "newest first"
    );
    assert_eq!(all["meta"]["total"], 3);
    assert_eq!(
        names(&api.list("?template_domain=invoice").await),
        ["Invoice EN", "Invoice NL"]
    );
    assert_eq!(
        names(&api.list("?language=nl").await),
        ["Order any", "Invoice NL"],
        "an unset condition matches"
    );
    assert_eq!(names(&api.list("?q=invoice%20e").await), ["Invoice EN"]);
    assert_eq!(
        names(&api.list("?page_size=1&page=2").await),
        ["Invoice EN"]
    );
    let (status, _) = api
        .call(Method::GET, "?template_domain=nope", None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

/// Laravel's variable lists per domain (the PDF templates' oracle output), `[]` for others.
#[sqlx::test]
async fn variables_per_domain(pool: PgPool) {
    let api = api(pool).await;
    let (status, res) = api
        .call(Method::GET, "/variables/invoice", None, &api.owner)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        res["data"]["variables"]["model"]["invoice.invoice_nr"],
        "invoice number"
    );
    assert!(res["data"]["variables"]["relation"].is_object());
    let (_, res) = api
        .call(Method::GET, "/variables/leave_request", None, &api.owner)
        .await;
    assert_eq!(res, json!({ "data": { "variables": [] } }));
}

#[sqlx::test]
async fn access_rules(pool: PgPool) {
    let api = api(pool).await;
    let body = template("A", "invoice", false);
    let id = api.create(body.clone(), "email_template_id").await;
    api.assert_member_forbidden(&id).await;
    let perms = [
        "emailtemplate.list",
        "emailtemplate.get",
        "emailtemplate.create",
        "emailtemplate.update",
        "emailtemplate.delete",
    ];
    api.assert_granted(&perms, body.clone(), "email_template_id")
        .await;
    api.assert_foreign_404(&id, body.clone()).await;
    api.delete(&id).await;
    api.assert_bad_ids_404(&id, body).await;
}
