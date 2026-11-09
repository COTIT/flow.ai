//! Assistant review cards for farm and supply writes name farmers, villages, lots and partners
//! instead of ids, and refuse callers without the operation's role before reading any name.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{
    farm::{Cast, Farm},
    send, signup,
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Propose `operation` as `token`; (status, review facts or error body).
pub(crate) async fn propose(
    f: &Farm,
    token: &str,
    operation: &str,
    tail: &str,
    body: Value,
) -> (StatusCode, Value) {
    let base = format!("/v1/workspace/{}/assistant", f.ws);
    let (status, res) = send(
        &f.app,
        Method::POST,
        &format!("{base}/sessions"),
        Some(json!({"language":"en"})),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let session = res["data"]["id"].as_str().unwrap();
    let proposal = json!({"session_id": session, "operation_id": operation, "method": "POST",
                          "path": format!("/v1/workspace/{}{tail}", f.ws), "body": body});
    let (status, res) = send(
        &f.app,
        Method::POST,
        &format!("{base}/actions"),
        Some(proposal),
        Some(token),
    )
    .await;
    if status == StatusCode::CREATED {
        let facts = &res["data"]["review_facts"];
        assert_eq!(facts["operation_id"], operation, "{res}");
        (status, facts["facts"].clone())
    } else {
        (status, res)
    }
}

struct World {
    f: Farm,
    c: Cast,
    harvest: String,
    product: String,
    tzs: String,
}

async fn world(pool: PgPool) -> World {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let (_, cycle) = f.grower("Baraka", "0712000009", &c.maize).await;
    let harvest = f.actual(&cycle, "100").await;
    let product = f.product("Raw maize").await;
    let tzs = f.tzs().await;
    World {
        f,
        c,
        harvest,
        product,
        tzs,
    }
}

#[sqlx::test]
async fn a_supply_batch_card_names_village_crop_and_farmers(pool: PgPool) {
    let w = world(pool).await;
    let body = json!({
        "client_ref": common::farm::client_ref(), "village_id": w.f.ilundo, "crop_id": w.c.maize,
        "code": "ILU-M-09", "raw_product_id": w.product, "price_per_kg": 80000,
        "currency_id": w.tzs, "lines": [{ "harvest_report_id": w.harvest, "kg": "40.5" }]
    });

    let (status, facts) = propose(
        &w.f,
        &w.f.ops,
        "farm_supply_create",
        "/farm/supply-batches",
        body.clone(),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{facts}");
    assert_eq!(facts["kind"], "farm_supply");
    assert_eq!(facts["locked"]["label"], "Ilundo · MAIZE");
    let priced = &facts["priced"];
    assert_eq!(
        priced["lines"],
        json!([{ "farmer": "Baraka Grower", "kg": "40.50" }])
    );
    assert_eq!(priced["kg_total"], "40.50");
    assert_eq!(priced["price_per_kg_minor"], 80000);
    assert_eq!(priced["currency"], json!({ "code": "TZS", "decimals": 2 }));
    assert!(
        !facts.to_string().contains(&w.harvest),
        "no raw harvest id on the card"
    );

    let (status, _) = propose(
        &w.f,
        &w.f.officer_ilu,
        "farm_supply_create",
        "/farm/supply-batches",
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "officers don't buy");
    let mut stranger = body.clone();
    stranger["lines"][0]["harvest_report_id"] = json!(uuid::Uuid::now_v7());
    let (status, res) = propose(
        &w.f,
        &w.f.ops,
        "farm_supply_create",
        "/farm/supply-batches",
        stranger,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(
        res["errors"]["lines.0.harvest_report_id"].is_array(),
        "{res}"
    );
    let other = Farm::new_named(w.f.pool.clone(), "else.test", "elsewhere-farm").await;
    let (status, _) = propose(
        &other,
        &other.ops,
        "farm_supply_create",
        "/farm/supply-batches",
        body,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "another workspace's village"
    );
}

#[sqlx::test]
async fn a_harvest_revision_card_shows_the_figure_before_and_after(pool: PgPool) {
    let w = world(pool).await;
    let tail = format!("/farm/crop-cycles/{}/harvest-reports", w.c.neema.cycles[0]);
    let body = json!({ "kind": "expected", "quantity_kg": "3000", "source": "field_verified" });

    let (status, facts) = propose(
        &w.f,
        &w.f.officer_ilu,
        "farm_records_harvest_reports_revise",
        &tail,
        body.clone(),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{facts}");
    assert_eq!(facts["kind"], "harvest");
    assert_eq!(facts["locked"]["label"], "Neema Mwakalinga · MAIZE");
    assert_eq!(
        facts["changes"],
        json!([{ "field": "quantity_kg", "before": "3200.00", "after": "3000.00" }])
    );
    assert_eq!(facts["note"], "The previous figure stays on record.");

    let (status, _) = propose(
        &w.f,
        &w.f.officer_mga,
        "farm_records_harvest_reports_revise",
        &tail,
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "officer of another village");
    let other = Farm::new_named(w.f.pool.clone(), "else.test", "elsewhere-farm").await;
    let (status, _) = propose(
        &other,
        &other.ops,
        "farm_records_harvest_reports_revise",
        &tail,
        body,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "another workspace's cycle");
}

#[sqlx::test]
async fn link_and_dispatch_cards_name_the_partner_lot_and_warehouse(pool: PgPool) {
    let w = world(pool).await;
    let (_, shop) = signup(&w.f.app, "owner@afm.test", "afm").await;

    let grant = json!({ "to_workspace_id": shop, "fields": ["village", "farmer_count"] });
    let (status, facts) = propose(
        &w.f,
        &w.f.owner,
        "supply_links_grant",
        "/supply-links",
        grant.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{facts}");
    assert_eq!(facts["kind"], "supply_link");
    assert_eq!(facts["locked"]["label"], "afm");
    assert_eq!(facts["note"], "Farmer names and photos stay off.");
    let own = json!({ "to_workspace_id": w.f.ws, "fields": [] });
    let (status, res) = propose(&w.f, &w.f.owner, "supply_links_grant", "/supply-links", own).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (status, _) = propose(
        &w.f,
        &w.f.ops,
        "supply_links_grant",
        "/supply-links",
        grant.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "ops is no workspace admin");

    let (status, link) = send(
        &w.f.app,
        Method::POST,
        &format!("/v1/workspace/{}/supply-links", w.f.ws),
        Some(grant),
        Some(&w.f.owner),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{link}");
    let link = link["data"]["id"].as_str().unwrap();
    let batch =
        w.f.batch(
            "ILU-M-10",
            &w.c.maize,
            &w.product,
            &w.tzs,
            1,
            &[(&w.harvest, "10")],
        )
        .await;
    let lot = batch["lot"]["id"].as_str().unwrap();
    let warehouse = w.f.warehouse("Ilundo store").await;
    let tail = format!("/lots/{lot}/transfers");
    let body = json!({ "link_id": link, "warehouse_id": warehouse, "quantity": "4" });

    let (status, facts) =
        propose(&w.f, &w.f.ops, "supply_chain_dispatch", &tail, body.clone()).await;

    assert_eq!(status, StatusCode::CREATED, "{facts}");
    assert_eq!(facts["kind"], "dispatch");
    assert_eq!(
        facts["locked"],
        json!({ "label": "ILU-M-10", "product": "Raw maize", "partner": "afm", "warehouse": "Ilundo store" })
    );
    assert_eq!(
        facts["changes"],
        json!([{ "field": "quantity", "after": "4.00" }])
    );
    let (status, _) = propose(
        &w.f,
        &w.f.officer_ilu,
        "supply_chain_dispatch",
        &tail,
        body.clone(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "officer without product.update"
    );
    let mut unknown = body;
    unknown["link_id"] = json!(uuid::Uuid::now_v7());
    let (status, res) = propose(&w.f, &w.f.ops, "supply_chain_dispatch", &tail, unknown).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}
