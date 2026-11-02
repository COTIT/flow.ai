//! Farm Control Tower reads (RUAHA `v_village_production`, `v_village_data_quality`): staff of
//! a village see its production by crop and month and how complete its records are. RUAHA ids
//! in docs/ruaha-rls-mapping.md.

use crate::common;

use axum::http::{Method, StatusCode};
use common::farm::Farm;
use serde_json::Value;
use sqlx::PgPool;

#[sqlx::test]
async fn production_groups_cycles_by_crop_and_harvest_month(pool: PgPool) {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let tail = format!("/villages/{}/production", f.ilundo);
    let (status, res) = f.call(Method::GET, &tail, None, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let rows = res["data"].as_array().unwrap();
    let row = |crop: &str| rows.iter().find(|r| r["crop_id"] == crop).unwrap();
    let maize = row(&c.maize);
    assert_eq!(maize["window_month"], "2026-09-01");
    assert_eq!(maize["cycle_count"], 1);
    assert_eq!(maize["plot_count"], 1);
    // The sum of cycle areas, not land: intercropped cycles share a plot.
    assert_eq!(maize["cycle_area_ha"], "1.6000");
    assert_eq!(maize["expected_kg"], "3200.00");
    assert_eq!(maize["actual_kg"], Value::Null);
    assert_eq!(maize["verified_cycles"], 0);
    let banana = row(&c.banana);
    assert_eq!(banana["window_month"], Value::Null);
    assert_eq!(banana["tree_count"], 40);
    let poultry = row(&c.poultry);
    assert_eq!(poultry["unit_count"], 50);
    // An actual figure shows next to the expected one.
    f.actual(&c.neema.cycles[0], "3000").await;
    let (_, res) = f.call(Method::GET, &tail, None, &f.ops).await;
    let maize = res["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["crop_id"] == c.maize)
        .unwrap()
        .clone();
    assert_eq!(maize["actual_kg"], "3000.00");
}

#[sqlx::test]
async fn data_quality_counts_how_complete_a_village_is(pool: PgPool) {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    f.verify("persons", &c.asha.person, &f.owner).await;
    let tail = format!("/villages/{}/data-quality", f.ilundo);
    let (status, res) = f.call(Method::GET, &tail, None, &f.ops).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let q = &res["data"];
    assert_eq!(q["village_id"], f.ilundo);
    assert_eq!(q["persons"], 3, "{res}");
    assert_eq!(q["persons_verified"], 1);
    assert_eq!(q["farms"], 2);
    assert_eq!(q["farms_with_gps"], 2);
    assert_eq!(q["cycles"], 3);
    assert_eq!(q["cycles_with_estimate"], 1);
}

#[sqlx::test]
async fn tower_reads_are_for_staff_of_the_village(pool: PgPool) {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let neema = f
        .farmer_for("neema@ruaha.test", &f.ilundo, &c.neema.person)
        .await;
    let other = Farm::new_named(f.pool.clone(), "else.test", "elsewhere").await;
    for kind in ["production", "data-quality"] {
        let tail = format!("/villages/{}/{kind}", f.ilundo);
        let (status, _) = f.call(Method::GET, &tail, None, &neema).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{kind}");
        let (status, _) = f.call(Method::GET, &tail, None, &f.officer_mga).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{kind}");
        let unknown = format!("/villages/019a0000-0000-7000-8000-0000000000ff/{kind}");
        let (status, _) = f.call(Method::GET, &unknown, None, &f.ops).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{kind}");
        let (status, _) = other.call(Method::GET, &tail, None, &other.ops).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{kind}");
    }
}

// ---- verify queue summary ----

#[sqlx::test]
async fn the_verify_queue_summary_counts_what_the_verify_lists_hold(pool: PgPool) {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let get = |token: String| {
        let f = &f;
        async move {
            let (status, res) = f
                .call(Method::GET, "/verify-queue/summary", None, &token)
                .await;
            assert_eq!(status, StatusCode::OK, "{res}");
            res["data"].clone()
        }
    };
    // Each kind must equal its list with ?verification=unverified,pending.
    for token in [f.ops.clone(), f.officer_ilu.clone(), f.officer_mga.clone()] {
        let s = get(token.clone()).await;
        let mut total = 0;
        for (kind, tail) in [
            ("persons", "/persons"),
            ("households", "/households"),
            ("farms", "/farms"),
            ("plots", "/plots"),
            ("crop_cycles", "/crop-cycles"),
            ("harvest_reports", "/harvest-reports"),
        ] {
            let list = f
                .list(&format!("{tail}?verification=unverified,pending"), &token)
                .await;
            assert_eq!(s[kind], list["meta"]["total"], "{kind}: {s}");
            total += list["meta"]["total"].as_i64().unwrap();
        }
        assert_eq!(s["total"], total, "{s}");
    }
    let all = get(f.ops.clone()).await;
    assert_eq!(all["persons"], 4, "{all}");
    // Verifying a record takes it out of the queue.
    f.verify("persons", &c.asha.person, &f.owner).await;
    assert_eq!(get(f.ops.clone()).await["persons"], 3);
    let mga = get(f.officer_mga.clone()).await;
    assert_eq!(mga["persons"], 1, "only Joseph: {mga}");
}

#[sqlx::test]
async fn the_verify_queue_summary_of_another_workspace_is_never_ours(pool: PgPool) {
    let f = Farm::new(pool).await;
    f.cast().await;
    let other = Farm::new_named(f.pool.clone(), "else.test", "elsewhere").await;
    let (status, res) = other
        .call(Method::GET, "/verify-queue/summary", None, &other.ops)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["total"], 0, "{res}");
    // Their ops aims at our workspace: no farm role there.
    let (status, _) = common::send(
        &f.app,
        Method::GET,
        &f.url("/verify-queue/summary"),
        None,
        Some(&other.ops),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = common::send(
        &f.app,
        Method::GET,
        &f.url("/verify-queue/summary"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
