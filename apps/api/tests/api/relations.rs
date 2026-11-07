//! CRM relations: create (inline contact, numbering, POS rules), get, list + search, update,
//! delete, financial summary. Spec: vimdesk-api tests/Feature/src/Domain/V1/CRM/Relation.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{Api, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn api(pool: PgPool) -> Api {
    Api::new(pool, "/relations").await
}

fn nl_address() -> Value {
    json!({
        "address": "midwaystraat", "address2": "midwaystraat1", "house_number": "11",
        "zipcode": "1339NM", "city": "almere", "state": "flevoland", "country": "nl"
    })
}

/// Laravel tests/Actions/Workspace/CreateRelation body.
fn body() -> Value {
    json!({
        "alias": "alias",
        "company_name": "COTIT",
        "type": "business",
        "email": "User@Cotit.eu",
        "phone_number": "+3622987654",
        "meta": { "source": "test" },
        "addresses": [nl_address()],
    })
}

async fn errors(api: &Api, body: Value) -> Value {
    let (status, res) = api.call(Method::POST, "", Some(body), &api.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    res["errors"].clone()
}

async fn type_id(api: &Api, name: &str) -> String {
    sqlx::query_scalar(
        "SELECT id::text FROM custom_types WHERE workspace_id = $1::uuid AND name = $2",
    )
    .bind(&api.ws)
    .bind(name)
    .fetch_one(&api.pool)
    .await
    .unwrap()
}

async fn stored(api: &Api, id: &str) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object('relation_number', relation_number, 'relation_appendix',
                relation_appendix, 'email', email, 'phone_number', phone_number,
                'supplier_data', supplier_data, 'created_by', created_by IS NOT NULL)
         FROM relations WHERE id = $1::uuid",
    )
    .bind(id)
    .fetch_one(&api.pool)
    .await
    .unwrap()
}

async fn contacts_in_db(api: &Api, relation: &str) -> Vec<Value> {
    sqlx::query_scalar(
        "SELECT to_jsonb(c) - 'id' - 'workspace_id' - 'relation_id' - 'created_at' - 'updated_at'
         FROM contacts c WHERE relation_id = $1::uuid ORDER BY id",
    )
    .bind(relation)
    .fetch_all(&api.pool)
    .await
    .unwrap()
}

// ---- create

#[sqlx::test]
async fn create_numbers_and_returns_relation_id(pool: PgPool) {
    let api = api(pool).await;

    let id = api.create(body(), "relation_id").await;
    let got = api.get(&id).await;

    assert_eq!(got["id"], id.as_str());
    assert_eq!(got["alias"], "alias");
    assert_eq!(got["company_name"], "COTIT");
    assert_eq!(got["email"], "user@cotit.eu");
    assert_eq!(got["relation_nr"], "RE-0001");
    assert_eq!(got["type"], "business");
    assert_eq!(got["meta"], json!({ "source": "test" }));
    assert_eq!(
        stored(&api, &id).await,
        json!({
            "relation_number": 1,
            "relation_appendix": { "prefix": "RE-", "digits": 4, "suffix": null },
            "email": "user@cotit.eu",
            "phone_number": "+3622987654",
            "supplier_data": null,
            "created_by": true,
        })
    );
    let second = api.create(body(), "relation_id").await;
    assert_eq!(api.get(&second).await["relation_nr"], "RE-0002");
}

#[sqlx::test]
async fn create_accepts_trailing_slash(pool: PgPool) {
    let api = api(pool).await;

    let (status, res) = api.call(Method::POST, "/", Some(body()), &api.owner).await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert!(res["data"]["relation_id"].is_string());
}

#[sqlx::test]
async fn create_without_type_fails_and_writes_nothing(pool: PgPool) {
    let api = api(pool).await;

    let e = errors(&api, json!({})).await;

    assert_eq!(e["type"], json!(["The type field is required."]));
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM relations")
        .fetch_one(&api.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test]
async fn relation_type_defaults_to_first_relation_type(pool: PgPool) {
    let api = api(pool).await;
    let mut b = body();
    b["relation_type_id"] = Value::Null;

    let id = api.create(b, "relation_id").await;
    let got = api.get(&id).await;

    let ty = &got["relationships"]["relation_type"];
    assert_eq!(ty["name"], "Supplier");
    assert_eq!(ty["domain"], "relation");
    assert_eq!(ty["properties"], json!({ "is_supplier": true }));
    assert!(ty["created_at"].is_string() && ty["last_updated_at"].is_string());
}

#[sqlx::test]
async fn given_relation_type_is_used(pool: PgPool) {
    let api = api(pool).await;
    let client = type_id(&api, "Client").await;
    let mut b = body();
    b["relation_type_id"] = json!(client);

    let id = api.create(b, "relation_id").await;

    assert_eq!(
        api.get(&id).await["relationships"]["relation_type"]["id"],
        client.as_str()
    );
}

#[sqlx::test]
async fn relation_type_must_be_a_live_relation_type_of_this_workspace(pool: PgPool) {
    let api = api(pool).await;
    let (_, evil_ws) = signup(&api.app, "eve@example.com", "evil").await;
    let foreign: String = sqlx::query_scalar(
        "SELECT id::text FROM custom_types WHERE workspace_id = $1::uuid AND domain = 'relation' LIMIT 1",
    )
    .bind(&evil_ws)
    .fetch_one(&api.pool)
    .await
    .unwrap();
    let lead_source = type_id(&api, "Website").await;
    let deleted = type_id(&api, "Client").await;
    sqlx::query("UPDATE custom_types SET deleted_at = now() WHERE id = $1::uuid")
        .bind(&deleted)
        .execute(&api.pool)
        .await
        .unwrap();

    for bad in [
        foreign.as_str(),
        lead_source.as_str(),
        deleted.as_str(),
        "01900000-0000-7000-8000-000000000001",
        "nope",
    ] {
        let mut b = body();
        b["relation_type_id"] = json!(bad);
        assert_eq!(
            errors(&api, b).await["relation_type_id"],
            json!(["The relation type id is not valid"]),
            "{bad}"
        );
    }
}

#[sqlx::test]
async fn no_relation_type_in_workspace_is_422_not_500(pool: PgPool) {
    let api = api(pool).await;
    sqlx::query("UPDATE custom_types SET deleted_at = now() WHERE domain = 'relation'")
        .execute(&api.pool)
        .await
        .unwrap();

    assert_eq!(
        errors(&api, body()).await["relation_type_id"],
        json!(["The relation type id field is required."])
    );
}

#[sqlx::test]
async fn pos_create_makes_one_default_contact_with_cleaned_names(pool: PgPool) {
    let api = api(pool).await;
    let b = json!({
        "created_from_pos": true,
        "type": "personal",
        "relation_type_id": null,
        "email": "Pos.Customer@Example.com",
        "addresses": [{ "address": "Midwaystraat", "house_number": "11", "zipcode": "1339NM",
                        "city": "Almere", "country": "NL", "main_address": true }],
        "contact": { "first_name": "john", "last_name": "doe", "email": "Pos.Customer@Example.com",
                     "default_contact": false, "middle_name": "Van", "prefix": "Mr",
                     "phones": [{ "type": "mobile", "number": "06 12345678" }] },
    });

    let id = api.create(b, "relation_id").await;

    let contacts = contacts_in_db(&api, &id).await;
    assert_eq!(contacts.len(), 1);
    let c = &contacts[0];
    assert_eq!(c["first_name"], "John");
    assert_eq!(c["last_name"], "Doe");
    assert_eq!(c["middle_name"], "van");
    assert_eq!(c["email"], "pos.customer@example.com");
    assert_eq!(c["default_contact"], true);
    assert_eq!(c["prefix"], "Mr");
    assert_eq!(
        c["phones"],
        json!([{ "type": "mobile", "number": "+31612345678" }])
    );
    let got = api.get(&id).await;
    let shown = &got["relationships"]["contacts"][0];
    assert_eq!(shown["full_name"], "John van Doe");
    assert_eq!(shown["relation_id"], id.as_str());
    assert_eq!(shown["relationships"], json!([]));
    assert_eq!(shown["email_validation"], Value::Null);
}

#[sqlx::test]
async fn pos_create_needs_a_display_name(pool: PgPool) {
    let api = api(pool).await;
    let b = json!({
        "created_from_pos": true, "type": "personal", "email": "pos-unnamed@example.com",
        "addresses": [nl_address()], "contact": { "first_name": "john" },
    });

    let e = errors(&api, b).await;

    let msg =
        json!(["A POS customer needs a company name, alias, or contact first and last name."]);
    assert_eq!(e["contact.first_name"], msg);
    assert_eq!(e["contact.last_name"], msg);
    for named in [
        json!({ "company_name": "COTIT POS" }),
        json!({ "alias": "Shop" }),
        json!({ "contact": { "first_name": "a", "last_name": "b" } }),
    ] {
        let mut ok = json!({ "created_from_pos": "1", "type": "business" });
        ok.as_object_mut()
            .unwrap()
            .extend(named.as_object().unwrap().clone());
        api.create(ok, "relation_id").await;
    }
}

#[sqlx::test]
async fn phone_numbers_normalized_with_the_main_address_country(pool: PgPool) {
    let api = api(pool).await;
    let b = json!({
        "type": "personal", "phone_number": "06 12345678",
        "addresses": [nl_address()],
        "contact": { "first_name": "j", "last_name": "d",
                     "phones": [{ "type": "phone", "number": "06 12345678" }] },
    });

    let id = api.create(b, "relation_id").await;

    assert_eq!(stored(&api, &id).await["phone_number"], "+31612345678");
    assert_eq!(
        contacts_in_db(&api, &id).await[0]["phones"][0]["number"],
        "+31612345678"
    );
}

#[sqlx::test]
async fn numbers_never_reused_and_unique_under_concurrency(pool: PgPool) {
    let api = api(pool).await;
    let first = api.create(body(), "relation_id").await;
    sqlx::query("UPDATE relations SET deleted_at = now() WHERE id = $1::uuid")
        .bind(&first)
        .execute(&api.pool)
        .await
        .unwrap();

    let mut set = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let (app, token) = (api.app.clone(), api.owner.clone());
        let uri = format!("/v1/workspace/{}/relations", api.ws);
        set.spawn(async move { send(&app, Method::POST, &uri, Some(body()), Some(&token)).await });
    }
    let results = set.join_all().await;

    assert!(results.iter().all(|(s, _)| *s == StatusCode::CREATED));
    let numbers: Vec<i64> = sqlx::query_scalar(
        "SELECT relation_number FROM relations WHERE deleted_at IS NULL ORDER BY relation_number",
    )
    .fetch_all(&api.pool)
    .await
    .unwrap();
    assert_eq!(numbers, (2..=21).collect::<Vec<_>>());
}

#[sqlx::test]
async fn field_rules_use_laravel_wording(pool: PgPool) {
    let api = api(pool).await;
    let mut b = body();
    for (k, v) in [
        ("type", json!("company")),
        ("email", json!("not-an-email")),
        ("website", json!("cotit.eu")),
        ("default_language", json!("du")),
        ("phone_number", json!("1".repeat(56))),
        ("vat_number", json!("NL123")),
        ("alias", json!("a".repeat(256))),
        ("allow_invoicing_on_account", json!("yes")),
        ("meta", json!([1])),
    ] {
        b[k] = v;
    }

    let e = errors(&api, b).await;

    assert_eq!(e["type"], json!(["The selected type: company is invalid."]));
    assert_eq!(
        e["email"],
        json!(["The email must be a valid email address."])
    );
    assert_eq!(e["website"], json!(["The website must be a valid URL."]));
    assert_eq!(
        e["default_language"],
        json!(["The selected default language is invalid."])
    );
    assert_eq!(
        e["phone_number"],
        json!(["The phone number must not be greater than 55 characters."])
    );
    assert_eq!(
        e["vat_number"],
        json!(["The Vat number must be a valid number issued by any Member State."])
    );
    assert_eq!(
        e["alias"],
        json!(["The alias must not be greater than 255 characters."])
    );
    assert_eq!(
        e["allow_invoicing_on_account"],
        json!(["The allow invoicing on account field must be true or false."])
    );
    assert_eq!(e["meta"], json!(["The meta must be an array."]));
}

#[sqlx::test]
async fn nested_values_validated(pool: PgPool) {
    let api = api(pool).await;
    let mut b = body();
    b["financial_account"] = json!({ "account_number": "NL00ABNA0417164300", "credit_limit": -1 });
    b["contact"] = json!({ "email": "x", "phones": [{ "type": "fax", "number": "1" }] });

    let e = errors(&api, b).await;

    assert_eq!(
        e["account_number"],
        json!(["The account number must be a valid IBAN."])
    );
    assert_eq!(
        e["credit_limit"],
        json!(["The credit limit must be at least 0."])
    );
    assert_eq!(
        e["contact.email"],
        json!(["The contact.email must be a valid email address."])
    );
    assert_eq!(
        e["contact.phones.0.type"],
        json!(["The selected type: fax is invalid."])
    );
}

#[sqlx::test]
async fn custom_sections_only_accept_none(pool: PgPool) {
    let api = api(pool).await;
    let mut ok = body();
    ok["custom_sections"] = json!([]);
    ok["custom_section_forms"] = Value::Null;
    api.create(ok, "relation_id").await;

    let mut bad = body();
    bad["custom_sections"] =
        json!([{ "section_id": "01900000-0000-7000-8000-000000000001", "values": [] }]);

    assert_eq!(
        errors(&api, bad).await["custom_sections.0.section_id"],
        json!(["this custom section does not exist for the selected domain"])
    );
}

#[sqlx::test]
async fn create_ignores_terms_and_supplier_data_like_laravel(pool: PgPool) {
    let api = api(pool).await;
    let mut b = body();
    b["terms_and_periods"] = json!({ "invoice_payment_term": 0 });
    b["supplier_data"] = json!({ "payment_term": "someday" });

    let id = api.create(b, "relation_id").await;

    assert_eq!(stored(&api, &id).await["supplier_data"], Value::Null);
    assert_eq!(
        api.get(&id).await["terms_and_periods"]["invoice_payment_term"],
        Value::Null
    );
}

#[sqlx::test]
async fn create_needs_relation_create(pool: PgPool) {
    let api = api(pool).await;
    let bob = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "bob@example.com",
        &["relation.list"],
    )
    .await;
    let carl = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "carl@example.com",
        &["relation.create"],
    )
    .await;

    let (status, res) = api.call(Method::POST, "", Some(json!({})), &bob).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    let (status, res) = api.call(Method::POST, "", Some(body()), &carl).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

// ---- get

#[sqlx::test]
async fn get_full_shape_with_conditional_keys(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;

    let got = api.get(&id).await;

    let keys: Vec<&str> = got
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    for k in [
        "id",
        "alias",
        "company_name",
        "coc_number",
        "vat_number",
        "type",
        "email",
        "email_validation",
        "website",
        "phone_number",
        "relation_nr",
        "import_id",
        "import_source",
        "allow_invoicing_on_account",
        "disable_order_workflow_emails",
        "terms_and_periods",
        "used_terms_and_periods",
        "default_language",
        "has_reverse_charge",
        "addresses",
        "financial_account",
        "created_at",
        "last_updated_at",
        "vat_groups",
        "meta",
        "relationships",
    ] {
        assert!(keys.contains(&k), "missing {k}: {keys:?}");
    }
    assert!(
        !keys.contains(&"supplier_data"),
        "supplier_data shown while empty"
    );
    // No workspace country: Laravel compares "NL" with "" and treats the business as foreign.
    assert_eq!(got["default_vat_rate"], "0");
    assert_eq!(got["has_reverse_charge"], false, "no VAT number");
    assert_eq!(got["email_validation"], Value::Null);
    assert_eq!(
        got["terms_and_periods"],
        json!({ "invoice_payment_term": null, "credit_invoice_payment_term": null,
                "collective_invoice_payment_term": null, "estimate_validity_period": null })
    );
    assert_eq!(
        got["used_terms_and_periods"],
        json!({ "invoice_payment_term": null, "credit_invoice_payment_term": 30,
                "collective_invoice_payment_term": 30, "estimate_validity_period": 30 })
    );
    assert_eq!(got["financial_account"], Value::Null);
    assert_eq!(got["vat_groups"], json!([]));
    let rel = &got["relationships"];
    assert_eq!(rel["contacts"], json!([]));
    assert_eq!(rel["source_type"], Value::Null);
    assert_eq!(rel["supplier_discount_groups"], json!([]));
    assert_eq!(rel["relation_discount_groups"], json!([]));
    assert_eq!(rel["custom_sections"], json!([]));
    assert_eq!(rel["custom_section_forms"], json!([]));
    assert_eq!(
        rel["unpaid_invoices"],
        json!({ "count": 0, "allowed_for_collective": 0, "allowed_on_account": false })
    );
    assert_eq!(rel["counts"], json!({ "files": 0 }));
    let address = &got["addresses"][0];
    assert_eq!(address["country"], "NL");
    assert_eq!(
        address["formatted"],
        "Midwaystraat 11 Midwaystraat1\n1339NM Almere\nNetherlands"
    );
    assert_eq!(address["type"], "other");
    assert_eq!(address["main_address"], false);
}

#[sqlx::test]
async fn slim_and_unpaid_flags_drop_keys(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;

    let (_, slim) = api
        .call(
            Method::GET,
            &format!("/{id}?slim=1&include_unpaid_invoices=0"),
            None,
            &api.owner,
        )
        .await;

    let rel = slim["data"]["relationships"].as_object().unwrap();
    for gone in [
        "supplier_discount_groups",
        "relation_discount_groups",
        "unpaid_invoices",
    ] {
        assert!(!rel.contains_key(gone), "{gone} present in slim");
    }
    assert!(rel.contains_key("contacts") && rel.contains_key("custom_sections"));
}

#[sqlx::test]
async fn reverse_charge_and_vat_rates_follow_workspace_country(pool: PgPool) {
    let api = api(pool).await;
    sqlx::query(
        "UPDATE workspaces SET profile = '{\"address\":{\"country\":\"NL\"},\"language\":\"nl\"}'
         WHERE id = $1::uuid",
    )
    .bind(&api.ws)
    .execute(&api.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO vat_groups (workspace_id, name, vat_amount, default_group, country) VALUES
         ($1::uuid, 'Hoog', 21, true, 'NL'), ($1::uuid, 'Laag', 9, false, 'NL'),
         ($1::uuid, 'Standard', 21, true, 'BE'), ($1::uuid, 'Zero', 0, false, 'BE')",
    )
    .bind(&api.ws)
    .execute(&api.pool)
    .await
    .unwrap();
    let mut domestic = body();
    domestic["vat_number"] = json!("NL123456789B01");
    let mut foreign = domestic.clone();
    foreign["addresses"][0]["country"] = json!("BE");
    foreign["vat_number"] = json!("BE0123456789");

    let nl = api.get(&api.create(domestic, "relation_id").await).await;
    let be = api.get(&api.create(foreign, "relation_id").await).await;

    assert_eq!(nl["has_reverse_charge"], false);
    assert_eq!(nl["default_vat_rate"], "21");
    assert_eq!(nl["addresses"][0]["reverse_charge"], false);
    assert_eq!(nl["addresses"][0]["shows_reverse_charge_notice"], false);
    assert_eq!(
        nl["addresses"][0]["vat_rates"],
        json!([
            { "vat_amount": 21.0, "name": "Hoog", "default_group": true },
            { "vat_amount": 9.0, "name": "Laag", "default_group": false },
            { "vat_amount": 0, "name": "BTW-verlegd", "default_group": false },
        ])
    );
    assert_eq!(nl["vat_groups"].as_array().unwrap().len(), 2);
    assert_eq!(nl["vat_groups"][0]["vat_amount"], "21");
    assert_eq!(be["has_reverse_charge"], true);
    assert_eq!(be["default_vat_rate"], "0");
    assert_eq!(be["addresses"][0]["reverse_charge"], true);
    assert_eq!(be["addresses"][0]["shows_reverse_charge_notice"], true);
    assert_eq!(
        be["addresses"][0]["vat_rates"].as_array().unwrap().len(),
        2,
        "has a 0% row"
    );
}

#[sqlx::test]
async fn get_is_member_only_and_tenant_scoped(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;
    let bob = member_with(&api.app, &api.pool, &api.ws, "bob@example.com", &[]).await;
    let (eve, eve_ws) = signup(&api.app, "eve@example.com", "evil").await;

    let (status, _) = api.call(Method::GET, &format!("/{id}"), None, &bob).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "any member may read one relation (Laravel parity)"
    );
    let (status, _) = send(
        &api.app,
        Method::GET,
        &format!("/v1/workspace/{eve_ws}/relations/{id}"),
        None,
        Some(&eve),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE relations SET deleted_at = now() WHERE id = $1::uuid")
        .bind(&id)
        .execute(&api.pool)
        .await
        .unwrap();
    for bad in [id.as_str(), "nope", "01900000-0000-7000-8000-000000000001"] {
        let (status, _) = api
            .call(Method::GET, &format!("/{bad}"), None, &api.owner)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{bad}");
    }
}

// ---- list + search

/// Relation with an optional contact; returns its id.
async fn relation(api: &Api, company: &str, email: &str, contact: Option<Value>) -> String {
    let mut b = json!({ "type": "business", "company_name": company, "email": email,
                        "addresses": [nl_address()] });
    if let Some(c) = contact {
        b["contact"] = c;
    }
    api.create(b, "relation_id").await
}

fn ids(res: &Value) -> Vec<String> {
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test]
async fn list_newest_first_in_index_shape(pool: PgPool) {
    let api = api(pool).await;
    let a = relation(
        &api,
        "Alpha",
        "a@x.nl",
        Some(json!({ "first_name": "ann", "last_name": "A" })),
    )
    .await;
    let b = relation(&api, "Beta", "b@x.nl", None).await;
    sqlx::query("UPDATE relations SET is_lead = true WHERE id = $1::uuid")
        .bind(&a)
        .execute(&api.pool)
        .await
        .unwrap();

    let res = api.list("?tab=overview&view_mode=list").await;

    assert_eq!(
        ids(&res),
        vec![b.clone(), a.clone()],
        "leads listed too, newest first"
    );
    assert_eq!(
        res["meta"],
        json!({ "current_page": 1, "from": 1, "last_page": 1, "per_page": 30, "to": 2, "total": 2 })
    );
    let row = &res["data"][1];
    assert_eq!(row["relation_nr"], "RE-0001");
    assert_eq!(row["activities_count"], 0);
    assert_eq!(row["has_activities"], false);
    assert_eq!(row["has_note_activity"], false);
    assert_eq!(row["tags_count"], 0);
    assert_eq!(row["open_orders"], Value::Null);
    assert_eq!(row["open_order_value"], "0.00");
    assert_eq!(row["supplier_data"], Value::Null);
    assert_eq!(row["email_validation"], Value::Null);
    assert_eq!(row["addresses"][0]["reverse_charge"], Value::Null);
    assert_eq!(
        row["addresses"][0]["formatted"],
        "Midwaystraat 11 Midwaystraat1\n1339NM Almere\nNetherlands"
    );
    let rel = row["relationships"].as_object().unwrap();
    assert_eq!(rel["relation_type"]["name"], "Supplier");
    assert_eq!(rel["contacts"][0]["full_name"], "Ann A");
    assert_eq!(rel["tags"], json!([]));
    assert!(!rel.contains_key("owner"));
    assert!(row.get("has_advice").is_none());
}

#[sqlx::test]
async fn list_limit_like_laravel(pool: PgPool) {
    let api = api(pool).await;
    for i in 0..3 {
        relation(&api, &format!("C{i}"), "c@x.nl", None).await;
    }

    let two = api.list("?limit=2&page=2").await;
    assert_eq!(
        two["meta"],
        json!({ "current_page": 2, "from": 3, "last_page": 2, "per_page": 2, "to": 3, "total": 3 })
    );
    for (q, per_page) in [
        ("?limit=500", 50),
        ("?limit=0", 30),
        ("?limit=abc", 30),
        ("?limit=199", 199),
    ] {
        assert_eq!(api.list(q).await["meta"]["per_page"], per_page, "{q}");
    }
    let past = api.list("?page=9").await;
    assert_eq!(
        (past["data"].clone(), past["meta"]["total"].clone()),
        (json!([]), json!(3))
    );
}

#[sqlx::test]
async fn list_needs_relation_list_and_stays_in_workspace(pool: PgPool) {
    let api = api(pool).await;
    let mine = relation(&api, "Mine", "m@x.nl", None).await;
    let bob = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "bob@example.com",
        &["relation.create"],
    )
    .await;
    let carl = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "carl@example.com",
        &["relation.list"],
    )
    .await;
    let (eve, eve_ws) = signup(&api.app, "eve@example.com", "evil").await;
    let uri = format!("/v1/workspace/{eve_ws}/relations");
    send(&api.app, Method::POST, &uri, Some(body()), Some(&eve)).await;

    assert_eq!(
        api.call(Method::GET, "", None, &bob).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, res) = api.call(Method::GET, "", None, &carl).await;
    assert_eq!((status, ids(&res)), (StatusCode::OK, vec![mine.clone()]));
    let (_, theirs) = send(&api.app, Method::GET, &uri, None, Some(&eve)).await;
    assert!(!ids(&theirs).contains(&mine));
    assert_eq!(
        api.call(Method::GET, "", None, &eve).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn supplier_list_and_advice_flags(pool: PgPool) {
    let api = api(pool).await;
    let supplier = relation(&api, "Parts BV", "p@x.nl", None).await;
    let client = type_id(&api, "Client").await;
    let mut c = body();
    c["relation_type_id"] = json!(client);
    api.create(c, "relation_id").await;
    let mut nameless = json!({ "type": "personal", "alias": "Anon" });
    nameless["relation_type_id"] = json!(type_id(&api, "Supplier").await);
    api.create(nameless, "relation_id").await;

    let res = api.list("?suppliers=1").await;
    assert_eq!(
        ids(&res),
        vec![supplier.clone()],
        "supplier type with a company name only"
    );
    assert_eq!(res["data"][0]["open_orders"], 0);
    assert_eq!(res["data"][0]["open_order_value"], "0.00");

    let advice = api.list("?suppliers=true&include_advice=1").await;
    let row = &advice["data"][0];
    assert_eq!(
        (row["has_advice"].clone(), row["advice_count"].clone()),
        (json!(false), json!(0))
    );
    assert_eq!(row["relationships"], json!({ "tags": [] }));
}

#[sqlx::test]
async fn search_matches_relation_contact_and_type_fields(pool: PgPool) {
    let api = api(pool).await;
    let cotit = relation(
        &api,
        "COTIT Retail",
        "info@cotit.eu",
        Some(
            json!({ "first_name": "john", "last_name": "Doe", "email": "john@doe.nl",
                     "phones": [{ "type": "mobile", "number": "06 12345678" }] }),
        ),
    )
    .await;
    let other = relation(&api, "Bakkerij 50% off", "bak@x.nl", None).await;
    let search = |q: &str| {
        let enc: String = q
            .chars()
            .map(|c| match c {
                ' ' => "%20".to_owned(),
                '"' => "%22".to_owned(),
                '%' => "%25".to_owned(),
                c => c.to_string(),
            })
            .collect();
        format!("?q={enc}")
    };

    for (q, want) in [
        ("cotit", vec![cotit.clone()]),
        ("RETAIL", vec![cotit.clone()]),
        ("john doe", vec![cotit.clone()]),
        ("doe.nl", vec![cotit.clone()]),
        ("\"info@cotit.eu\"", vec![cotit.clone()]),
        ("Supplier", vec![other.clone(), cotit.clone()]),
        ("50%", vec![other.clone()]),
        ("RE-0002", vec![other.clone()]),
        ("0612345678", vec![cotit.clone()]),
        ("nobody", vec![]),
        ("null undefined", vec![]),
        ("*", vec![other.clone(), cotit.clone()]),
    ] {
        assert_eq!(ids(&api.list(&search(q)).await), want, "q={q}");
    }
}

#[sqlx::test]
async fn filters_last_name_zipcode_company(pool: PgPool) {
    let api = api(pool).await;
    let a = relation(
        &api,
        "Acme",
        "a@x.nl",
        Some(json!({ "first_name": "j", "last_name": "Jansen" })),
    )
    .await;
    let mut spaced = body();
    spaced["addresses"][0]["zipcode"] = json!("1234 AB");
    let b = api.create(spaced, "relation_id").await;

    assert_eq!(ids(&api.list("?last_name=jansen").await), vec![a.clone()]);
    assert_eq!(ids(&api.list("?zipcode=1339nm").await), vec![a.clone()]);
    assert_eq!(ids(&api.list("?zipcode=1234AB").await), vec![b.clone()]);
    assert_eq!(ids(&api.list("?company_name=acme").await), vec![a.clone()]);
    assert_eq!(
        ids(&api.list("?company_name=acm").await),
        Vec::<String>::new()
    );
}

#[sqlx::test]
async fn search_registry_answers_for_relations(pool: PgPool) {
    let api = api(pool).await;
    let supplier = relation(&api, "Parts BV", "p@x.nl", None).await;
    let mut c = body();
    c["relation_type_id"] = json!(type_id(&api, "Client").await);
    c["company_name"] = json!("Parts Client");
    let client = api.create(c, "relation_id").await;
    let uri = |q: &str| {
        format!(
            "/v1/workspace/{}/searches?domain=relation&q=parts{q}",
            api.ws
        )
    };

    let (status, all) = send(&api.app, Method::GET, &uri(""), None, Some(&api.owner)).await;
    assert_eq!(status, StatusCode::OK, "{all}");
    assert_eq!(ids(&all), vec![client, supplier.clone()]);
    assert!(all["data"][0]["relationships"]["contacts"].is_array());
    let (_, suppliers) = send(
        &api.app,
        Method::GET,
        &uri("&is_supplier=true"),
        None,
        Some(&api.owner),
    )
    .await;
    assert_eq!(ids(&suppliers), vec![supplier]);
}

#[sqlx::test]
async fn list_stays_fast_at_volume(pool: PgPool) {
    let api = api(pool).await;
    let ty = type_id(&api, "Client").await;
    sqlx::query(
        "INSERT INTO relations (workspace_id, relation_type_id, relation_number, company_name, email)
         SELECT $1::uuid, $2::uuid, n, 'Company ' || n, 'c' || n || '@x.nl'
         FROM generate_series(1, 3000) n",
    )
    .bind(&api.ws)
    .bind(&ty)
    .execute(&api.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO contacts (workspace_id, relation_id, first_name, last_name)
         SELECT workspace_id, id, 'First', 'Last' || relation_number FROM relations",
    )
    .execute(&api.pool)
    .await
    .unwrap();

    let page = api.list("").await;
    let found = api.list("?q=last2999").await;

    assert_eq!(page["meta"]["total"], 3000);
    assert_eq!(page["data"].as_array().unwrap().len(), 30);
    assert_eq!(found["meta"]["total"], 1);
}

// ---- update

async fn update(api: &Api, id: &str, b: Value) -> (StatusCode, Value) {
    api.update(id, b).await
}

/// The minimal update body Laravel requires (`type` + `relation_type_id`).
async fn required(api: &Api, kind: &str) -> Value {
    json!({ "type": kind, "relation_type_id": type_id(api, "Client").await })
}

#[sqlx::test]
async fn update_patches_only_sent_keys(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;
    let mut b = required(&api, "personal").await;
    b["alias"] = Value::Null;
    b["website"] = json!("https://cotit.eu");
    b["address"] = json!({});
    b["default_validity_period"] = json!(30);

    let (status, res) = update(&api, &id, b).await;

    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let got = api.get(&id).await;
    assert_eq!(got["type"], "personal");
    assert_eq!(got["relationships"]["relation_type"]["name"], "Client");
    assert_eq!(got["alias"], Value::Null, "sent null clears");
    assert_eq!(got["website"], "https://cotit.eu");
    assert_eq!(got["company_name"], "COTIT", "absent key kept");
    assert_eq!(got["email"], "user@cotit.eu");
    assert_eq!(got["relation_nr"], "RE-0001", "number never changes");
    assert_eq!(got["addresses"].as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn update_replaces_addresses_and_normalizes_phone(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;
    let mut b = required(&api, "business").await;
    b["addresses"] = json!([
        { "address": "Rue", "house_number": "1", "zipcode": "1000", "city": "Brussel", "country": "be", "main_address": true },
        { "address": "Weg", "house_number": "2", "zipcode": "1339NM", "city": "Almere", "country": "NL" },
    ]);
    b["phone_number"] = json!("0471 12 34 56");

    assert_eq!(update(&api, &id, b).await.0, StatusCode::OK);

    let got = api.get(&id).await;
    assert_eq!(got["addresses"].as_array().unwrap().len(), 2);
    assert_eq!(got["addresses"][0]["country"], "BE");
    assert_eq!(
        got["phone_number"], "+32471123456",
        "country of the new main address"
    );
}

#[sqlx::test]
async fn update_requires_type_and_relation_type(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;

    let (status, res) = update(&api, &id, json!({ "alias": "x" })).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["type"],
        json!(["The type field is required."])
    );
    assert_eq!(
        res["errors"]["relation_type_id"],
        json!(["The relation type id field is required."])
    );
    let mut lead = required(&api, "business").await;
    lead["relation_type_id"] = json!(type_id(&api, "Website").await);
    assert_eq!(
        update(&api, &id, lead).await.1["errors"]["relation_type_id"],
        json!(["The relation type id is not valid"])
    );
    let mut nulled = required(&api, "business").await;
    nulled["allow_invoicing_on_account"] = Value::Null;
    assert_eq!(
        update(&api, &id, nulled).await.1["errors"]["allow_invoicing_on_account"],
        json!(["The allow invoicing on account field is required."])
    );
}

#[sqlx::test]
async fn update_terms_and_supplier_data(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;
    let mut bad = required(&api, "business").await;
    bad["terms_and_periods"] =
        json!({ "invoice_payment_term": 0, "estimate_validity_period": 366 });
    bad["supplier_data"] = json!({ "payment_term": "someday", "order_type": "pigeon",
                                   "lead_time": "soon",
                                   "fixed_shipping_cost": { "enabled": true } });

    let e = update(&api, &id, bad).await.1["errors"].clone();

    assert_eq!(
        e["terms_and_periods.invoice_payment_term"],
        json!(["The invoice payment term must be at least 1."])
    );
    assert_eq!(
        e["terms_and_periods.estimate_validity_period"],
        json!(["The estimate validity period must not be greater than 365."])
    );
    assert_eq!(
        e["supplier_data.payment_term"],
        json!(["The selected payment term is invalid."])
    );
    assert_eq!(
        e["supplier_data.order_type"],
        json!(["The selected order type is invalid."])
    );
    assert_eq!(
        e["supplier_data.lead_time"],
        json!(["The lead time must be an integer."])
    );
    assert_eq!(
        e["supplier_data.fixed_shipping_cost.amount_base"],
        json!(["The amount base field is required."])
    );

    let mut ok = required(&api, "business").await;
    ok["terms_and_periods"] =
        json!({ "invoice_payment_term": "21", "estimate_validity_period": null });
    ok["supplier_data"] = json!({ "payment_term": "thirty_days", "lead_time": 5, "order_type": "ftp",
                                  "order_ftp": { "host": "ftp.acme.nl", "password": "s3cret" } });
    assert_eq!(update(&api, &id, ok).await.0, StatusCode::OK);
    let got = api.get(&id).await;
    assert_eq!(got["terms_and_periods"]["invoice_payment_term"], 21);
    assert_eq!(got["used_terms_and_periods"]["invoice_payment_term"], 21);
    assert_eq!(
        got["used_terms_and_periods"]["estimate_validity_period"],
        30
    );
    assert_eq!(got["supplier_data"]["order_ftp"]["password"], Value::Null);
    assert_eq!(got["supplier_data"]["payment_term"], "thirty_days");
}

#[sqlx::test]
async fn ftp_password_is_write_only(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;
    let mut set = required(&api, "business").await;
    set["supplier_data"] = json!({ "order_ftp": { "host": "h", "password": "s3cret" } });
    update(&api, &id, set).await;

    // vimdesk-web posts back what it loaded: the password as null.
    let mut echo = required(&api, "business").await;
    echo["supplier_data"] = api.get(&id).await["supplier_data"].clone();
    echo["supplier_data"]["order_ftp"]["path"] = json!("/po");
    assert_eq!(update(&api, &id, echo).await.0, StatusCode::OK);

    let ftp = stored(&api, &id).await["supplier_data"]["order_ftp"].clone();
    assert_eq!(
        ftp,
        json!({ "host": "h", "path": "/po", "password": "s3cret" })
    );
    let mut replace = required(&api, "business").await;
    replace["supplier_data"] = json!({ "order_ftp": { "host": "h", "password": "new" } });
    update(&api, &id, replace).await;
    assert_eq!(
        stored(&api, &id).await["supplier_data"]["order_ftp"]["password"],
        "new"
    );
}

#[sqlx::test]
async fn update_vat_checked_only_for_stored_eu_country(pool: PgPool) {
    let api = api(pool).await;
    let nl = api.create(body(), "relation_id").await;
    let mut us_body = body();
    us_body["addresses"][0]["country"] = json!("US");
    let us = api.create(us_body, "relation_id").await;
    let vat = |v: &str| {
        let v = v.to_owned();
        let api = &api;
        async move {
            let mut b = required(api, "business").await;
            b["vat_number"] = json!(v);
            b
        }
    };

    let (status, res) = update(&api, &nl, vat("NL123").await).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["vat_number"],
        json!(["The Vat number must be a valid number issued by any Member State."])
    );
    assert_eq!(
        update(&api, &nl, vat("NL123456789B01").await).await.0,
        StatusCode::OK
    );
    assert_eq!(
        update(&api, &us, vat("EIN 12-345").await).await.0,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn update_and_delete_permissions_and_ids(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;
    let bob = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "bob@example.com",
        &["relation.list"],
    )
    .await;
    let carl = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "carl@example.com",
        &["relation.update", "relation.delete"],
    )
    .await;
    let path = format!("/{id}");

    for method in [Method::POST, Method::DELETE] {
        let (status, _) = api.call(method.clone(), &path, Some(json!({})), &bob).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method}");
    }
    let b = required(&api, "business").await;
    assert_eq!(
        api.call(Method::POST, &path, Some(b.clone()), &carl)
            .await
            .0,
        StatusCode::OK
    );
    let (eve, eve_ws) = signup(&api.app, "eve@example.com", "evil").await;
    let foreign = format!("/v1/workspace/{eve_ws}/relations/{id}");
    for method in [Method::POST, Method::DELETE] {
        let mut fb = b.clone();
        fb["relation_type_id"] = json!("01900000-0000-7000-8000-000000000001");
        let (status, _) = send(&api.app, method.clone(), &foreign, Some(fb), Some(&eve)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "foreign {method}");
    }
    api.delete(&id).await;
    for bad in [id.as_str(), "nope", "01900000-0000-7000-8000-000000000001"] {
        for method in [Method::POST, Method::DELETE] {
            let (status, _) = api
                .call(
                    method.clone(),
                    &format!("/{bad}"),
                    Some(b.clone()),
                    &api.owner,
                )
                .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {bad}");
        }
    }
}

#[sqlx::test]
async fn delete_keeps_contacts_and_number(pool: PgPool) {
    let api = api(pool).await;
    let mut b = body();
    b["contact"] = json!({ "first_name": "a", "last_name": "b" });
    let id = api.create(b, "relation_id").await;

    api.delete(&id).await;

    assert_eq!(api.list("").await["meta"]["total"], 0);
    assert_eq!(
        contacts_in_db(&api, &id).await.len(),
        1,
        "Laravel left contacts in place"
    );
    let next = api.create(body(), "relation_id").await;
    assert_eq!(api.get(&next).await["relation_nr"], "RE-0002");
}

// ---- contacts

async fn contacts(
    api: &Api,
    relation: &str,
    path: &str,
    method: Method,
    b: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    api.call(method, &format!("/{relation}/contacts{path}"), b, token)
        .await
}

#[sqlx::test]
async fn contact_create_update_delete(pool: PgPool) {
    let api = api(pool).await;
    let rel = api.create(body(), "relation_id").await;

    let (status, res) = contacts(
        &api,
        &rel,
        "",
        Method::POST,
        Some(
            json!({ "first_name": "jan", "last_name": "jansen", "email": "Jan@X.nl",
                     "phone_number": "06 1234 5678", "job_title": "Buyer", "default_contact": true,
                     "default_for": "default_for_invoice",
                     "phones": [{ "type": "work", "number": "020 1234567" }] }),
        ),
        &api.owner,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let cid = res["data"]["contact_id"].as_str().unwrap().to_owned();
    let shown = &api.get(&rel).await["relationships"]["contacts"][0];
    assert_eq!(shown["id"], cid.as_str());
    assert_eq!(shown["full_name"], "Jan Jansen");
    assert_eq!(shown["email"], "jan@x.nl");
    assert_eq!(shown["default_for"], "default_for_invoice");
    assert_eq!(
        shown["phones"],
        json!([{ "type": "work", "number": "+31201234567" }])
    );
    assert!(
        shown.get("phone_number").is_none(),
        "Laravel never returned it"
    );

    let (status, res) = contacts(
        &api,
        &rel,
        &format!("/{cid}"),
        Method::POST,
        Some(json!({ "job_title": "Owner" })),
        &api.owner,
    )
    .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let c = &contacts_in_db(&api, &rel).await[0];
    assert_eq!(c["job_title"], "Owner");
    assert_eq!(
        c["email"], "jan@x.nl",
        "Laravel cleared it on every update without email"
    );
    assert_eq!(c["phone_number"], "+31612345678");

    let (status, res) = contacts(
        &api,
        &rel,
        &format!("/{cid}"),
        Method::DELETE,
        None,
        &api.owner,
    )
    .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert_eq!(api.get(&rel).await["relationships"]["contacts"], json!([]));
}

#[sqlx::test]
async fn one_default_contact_after_create_and_update(pool: PgPool) {
    let api = api(pool).await;
    let rel = api.create(body(), "relation_id").await;
    let add = |first: &'static str| {
        let api = &api;
        let rel = rel.clone();
        async move {
            let b = json!({ "first_name": first, "last_name": "x", "default_contact": true });
            let (s, r) = contacts(api, &rel, "", Method::POST, Some(b), &api.owner).await;
            assert_eq!(s, StatusCode::CREATED, "{r}");
            r["data"]["contact_id"].as_str().unwrap().to_owned()
        }
    };
    let first = add("a").await;
    let second = add("b").await;
    let defaults = |api: &Api| {
        let pool = api.pool.clone();
        let rel = rel.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT id::text FROM contacts WHERE relation_id = $1::uuid AND default_contact",
            )
            .bind(rel)
            .fetch_all(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(defaults(&api).await, vec![second.clone()]);

    let b = json!({ "default_contact": "1" });
    contacts(
        &api,
        &rel,
        &format!("/{first}"),
        Method::POST,
        Some(b),
        &api.owner,
    )
    .await;

    assert_eq!(defaults(&api).await, vec![first]);
}

#[sqlx::test]
async fn contact_rules_and_scoping(pool: PgPool) {
    let api = api(pool).await;
    let rel = api.create(body(), "relation_id").await;
    let other = api.create(body(), "relation_id").await;
    let (_, made) = contacts(
        &api,
        &rel,
        "",
        Method::POST,
        Some(json!({ "first_name": "a", "last_name": "b" })),
        &api.owner,
    )
    .await;
    let cid = made["data"]["contact_id"].as_str().unwrap().to_owned();

    let (status, res) = contacts(
        &api,
        &rel,
        "",
        Method::POST,
        Some(json!({ "email": "x", "default_for": "invoice" })),
        &api.owner,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["first_name"],
        json!(["The first name field is required."])
    );
    assert_eq!(
        res["errors"]["last_name"],
        json!(["The last name field is required."])
    );
    assert_eq!(
        res["errors"]["email"],
        json!(["The email must be a valid email address."])
    );
    assert_eq!(
        res["errors"]["default_for"],
        json!(["The selected default for: invoice is invalid."])
    );

    for (method, b) in [
        (Method::POST, Some(json!({ "job_title": "x" }))),
        (Method::DELETE, None),
    ] {
        let (status, _) = contacts(
            &api,
            &other,
            &format!("/{cid}"),
            method.clone(),
            b.clone(),
            &api.owner,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "contact via another relation, {method}"
        );
        let (status, _) = contacts(&api, &rel, "/undefined", method.clone(), b, &api.owner).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "POS sends /contacts/undefined, {method}"
        );
    }
    let (status, _) = contacts(
        &api,
        "01900000-0000-7000-8000-000000000001",
        "",
        Method::POST,
        Some(json!({ "first_name": "a", "last_name": "b" })),
        &api.owner,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown relation");

    let bob = member_with(
        &api.app,
        &api.pool,
        &api.ws,
        "bob@example.com",
        &["relation.list"],
    )
    .await;
    for (path, method) in [
        ("".to_owned(), Method::POST),
        (format!("/{cid}"), Method::POST),
        (format!("/{cid}"), Method::DELETE),
    ] {
        let (status, _) = contacts(&api, &rel, &path, method.clone(), Some(json!({})), &bob).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }
    let (eve, eve_ws) = signup(&api.app, "eve@example.com", "evil").await;
    let foreign = format!("/v1/workspace/{eve_ws}/relations/{rel}/contacts");
    let (status, _) = send(
        &api.app,
        Method::POST,
        &foreign,
        Some(json!({ "first_name": "a", "last_name": "b" })),
        Some(&eve),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let original_contacts = contacts_in_db(&api, &rel).await;
    for workspace in [&api.ws, &eve_ws] {
        let path = format!("/v1/workspace/{workspace}/relations/{rel}/contacts/{cid}");
        for (method, body) in [
            (
                Method::POST,
                Some(
                    json!({"first_name":"Foreign","last_name":"Changed","job_title":"Foreign probe"}),
                ),
            ),
            (Method::DELETE, None),
        ] {
            let (status, reply) = send(&api.app, method.clone(), &path, body, Some(&eve)).await;
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "foreign live contact {method} {path}: {reply}"
            );
        }
    }
    assert_eq!(
        contacts_in_db(&api, &rel).await,
        original_contacts,
        "foreign contact writes leave owner data unchanged"
    );
}

// ---- financial summary

#[sqlx::test]
async fn financial_summary_zero_documents(pool: PgPool) {
    let api = api(pool).await;
    let mut b = body();
    b["financial_account"] = json!({ "credit_limit": "1500.5" });
    let id = api.create(b, "relation_id").await;

    let (status, lite) = api
        .call(
            Method::GET,
            &format!("/{id}/financial-summary?lite=1"),
            None,
            &api.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{lite}");
    assert_eq!(
        lite["data"],
        json!({ "relation_id": id, "projected_ltv": null, "total_spend": 0.0, "outstanding_amount": 0.0,
                "outstanding_count": 0, "total_customer_value": 0.0, "outstanding_balance": 0.0,
                "credit_limit": 1500.5, "available_credit": 1500.5, "account_manager_name": null,
                "customer_since": null, "last_payment_date": null, "loyalty_points": 0 })
    );

    let (_, full) = api
        .call(
            Method::GET,
            &format!("/{id}/financial-summary"),
            None,
            &api.owner,
        )
        .await;
    let d = &full["data"];
    for k in [
        "first_send_date",
        "last_send_date",
        "months_since_last",
        "account_manager_name",
        "last_payment_date",
    ] {
        assert_eq!(d[k], Value::Null, "{k}");
    }
    assert_eq!(d["orders_count"], 0);
    assert_eq!(d["expected_lifespan_years"], 5);
    assert_eq!(d["projected_ltv"], 0.0);
    assert!(
        d["customer_since"].as_str().unwrap().len() == 19,
        "Y-m-d H:i:s like the MySQL row"
    );

    let bob = member_with(&api.app, &api.pool, &api.ws, "bob@example.com", &[]).await;
    assert_eq!(
        api.call(Method::GET, &format!("/{id}/financial-summary"), None, &bob)
            .await
            .0,
        StatusCode::OK
    );
    let (eve, eve_ws) = signup(&api.app, "eve@example.com", "evil").await;
    let foreign = format!("/v1/workspace/{eve_ws}/relations/{id}/financial-summary");
    assert_eq!(
        send(&api.app, Method::GET, &foreign, None, Some(&eve))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

// ---- review fixes

#[sqlx::test]
async fn ftp_password_dropped_when_server_or_login_changes(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;
    let mut set = required(&api, "business").await;
    set["supplier_data"] =
        json!({ "order_ftp": { "host": "ftp.acme.nl", "username": "po", "password": "s3cret" } });
    update(&api, &id, set).await;

    // Blank (spaces) with the same server keeps it.
    let mut same = required(&api, "business").await;
    same["supplier_data"] =
        json!({ "order_ftp": { "host": "ftp.acme.nl", "username": "po", "password": "  " } });
    update(&api, &id, same).await;
    assert_eq!(
        stored(&api, &id).await["supplier_data"]["order_ftp"]["password"],
        "s3cret"
    );

    // Same blank password, other host: the old secret must not follow it.
    let mut moved = required(&api, "business").await;
    moved["supplier_data"] =
        json!({ "order_ftp": { "host": "evil.example", "username": "po", "password": null } });
    update(&api, &id, moved).await;
    assert_eq!(
        stored(&api, &id).await["supplier_data"]["order_ftp"]["password"],
        Value::Null
    );
}

#[sqlx::test]
async fn list_sizes_capped(pool: PgPool) {
    let api = api(pool).await;
    let mut many = body();
    many["addresses"] = json!(vec![nl_address(); 51]);
    assert_eq!(
        errors(&api, many).await["addresses"],
        json!(["The addresses must not have more than 50 items."])
    );
    let mut phones = body();
    phones["contact"] = json!({ "phones": vec![json!({ "type": "home", "number": "1" }); 21] });
    assert_eq!(
        errors(&api, phones).await["contact.phones"],
        json!(["The contact.phones must not have more than 20 items."])
    );
}

#[sqlx::test]
async fn inline_contact_meta_error_keyed_under_contact(pool: PgPool) {
    let api = api(pool).await;
    let mut b = body();
    b["contact"] = json!({ "first_name": "a", "meta": [1] });

    let e = errors(&api, b).await;

    assert_eq!(
        e["contact.meta"],
        json!(["The contact.meta must be an array."])
    );
    assert!(e.get("meta").is_none(), "{e}");
}

#[sqlx::test]
async fn source_type_must_be_a_lead_source(pool: PgPool) {
    let api = api(pool).await;
    let mut ok = body();
    ok["source_type_id"] = json!(type_id(&api, "Website").await);
    let id = api.create(ok, "relation_id").await;
    assert_eq!(
        api.get(&id).await["relationships"]["source_type"]["name"],
        "Website"
    );

    let mut bad = body();
    bad["source_type_id"] = json!(type_id(&api, "Client").await);
    assert_eq!(
        errors(&api, bad).await["source_type_id"],
        json!(["The source type id is not valid"])
    );
}

#[sqlx::test]
async fn contacts_of_a_deleted_relation_are_404(pool: PgPool) {
    let api = api(pool).await;
    let mut b = body();
    b["contact"] = json!({ "first_name": "a", "last_name": "b" });
    let id = api.create(b, "relation_id").await;
    let cid: String =
        sqlx::query_scalar("SELECT id::text FROM contacts WHERE relation_id = $1::uuid")
            .bind(&id)
            .fetch_one(&api.pool)
            .await
            .unwrap();
    api.delete(&id).await;

    let (status, _) = api
        .call(
            Method::DELETE,
            &format!("/{id}/contacts/{cid}"),
            None,
            &api.owner,
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn junk_zipcode_filter_matches_nothing(pool: PgPool) {
    let api = api(pool).await;
    relation(&api, "Acme", "a@x.nl", None).await;

    assert_eq!(ids(&api.list("?zipcode=---").await), Vec::<String>::new());
}

#[sqlx::test]
async fn legacy_address_values_still_render(pool: PgPool) {
    let api = api(pool).await;
    let id = api.create(body(), "relation_id").await;
    sqlx::query(
        r#"UPDATE relations SET addresses = '[{"address":"A","house_number":11,"zipcode":"1","city":"C",
                                              "country":"NL","main_address":null,"type":null}]'
           WHERE id = $1::uuid"#,
    )
    .bind(&id)
    .execute(&api.pool)
    .await
    .unwrap();

    let got = api.get(&id).await;

    assert_eq!(got["addresses"][0]["house_number"], "11");
    assert_eq!(got["addresses"][0]["main_address"], false);
    assert_eq!(api.list("").await["meta"]["total"], 1);
}
