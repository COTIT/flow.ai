//! CRM relations: create (inline contact, numbering, POS rules), get, list + search, update,
//! delete, financial summary. Spec: vimdesk-api tests/Feature/src/Domain/V1/CRM/Relation.

mod common;

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
        json!(["The email field must be a valid email address."])
    );
    assert_eq!(
        e["website"],
        json!(["The website field must be a valid URL."])
    );
    assert_eq!(
        e["default_language"],
        json!(["The selected default language is invalid."])
    );
    assert_eq!(
        e["phone_number"],
        json!(["The phone number field must not be greater than 55 characters."])
    );
    assert_eq!(
        e["vat_number"],
        json!(["The Vat number must be a valid number issued by any Member State."])
    );
    assert_eq!(
        e["alias"],
        json!(["The alias field must not be greater than 255 characters."])
    );
    assert_eq!(
        e["allow_invoicing_on_account"],
        json!(["The allow invoicing on account field must be true or false."])
    );
    assert_eq!(e["meta"], json!(["The meta field must be an array."]));
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
        json!(["The account number field must be a valid IBAN."])
    );
    assert_eq!(
        e["credit_limit"],
        json!(["The credit limit field must be at least 0."])
    );
    assert_eq!(
        e["contact.email"],
        json!(["The contact.email field must be a valid email address."])
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
