//! Farm records: who reads what (farmer: own household and farms; officer: their village;
//! ops: everything), officer corrections, one-way verification with four eyes on households,
//! harvest revisions, and phone/GPS privacy. RUAHA ids in docs/ruaha-rls-mapping.md.

mod common;

use axum::http::{Method, StatusCode};
use common::{
    farm::{Cast, Farm},
    send, signup,
};
use serde_json::{Value, json};
use sqlx::PgPool;

fn ids(list: &Value) -> Vec<String> {
    let mut v: Vec<String> = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect();
    v.sort();
    v
}

fn sorted(mut v: Vec<&String>) -> Vec<String> {
    v.sort();
    v.into_iter().cloned().collect()
}

async fn setup(pool: PgPool) -> (Farm, Cast, String) {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let neema = f
        .farmer_for("neema@ruaha.test", &f.ilundo, &c.neema.person)
        .await;
    (f, c, neema)
}

// ---- who reads what ----

#[sqlx::test]
async fn a_farmer_reads_her_own_farm_and_household_only(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    assert_eq!(
        ids(&f.list("/farms", &neema).await),
        vec![c.neema.farm.clone()]
    );
    let (status, _) = f
        .call(
            Method::GET,
            &format!("/farms/{}", c.asha.farm),
            None,
            &neema,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "not the Kimaro farm");
    assert_eq!(
        ids(&f.list("/persons", &neema).await),
        sorted(vec![&c.neema.person, &c.rehema.person]),
        "herself and her household"
    );
    assert_eq!(
        ids(&f.list("/households", &neema).await),
        vec![c.neema.household.clone()]
    );
    assert_eq!(
        ids(&f.list("/plots", &neema).await),
        sorted(vec![&c.neema.plot, &c.rehema.plot])
    );
    assert_eq!(f.list("/crop-cycles", &neema).await["meta"]["total"], 2);
}

#[sqlx::test]
async fn officers_read_their_village_only(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let ilu = f.list("/persons", &f.officer_ilu).await;
    assert_eq!(ilu["meta"]["total"], 3, "the three Ilundo persons");
    let mga = f
        .list(&format!("/persons?village_id={}", f.mgama), &f.officer_ilu)
        .await;
    assert_eq!(mga["meta"]["total"], 0, "no Mgama persons");
    assert_eq!(
        ids(&f.list("/persons", &f.officer_mga).await),
        vec![c.joseph.person.clone()]
    );
    assert_eq!(
        ids(&f.list("/farms", &f.officer_mga).await),
        vec![c.joseph.farm.clone()]
    );
    assert_eq!(
        f.list("/persons", &f.ops).await["meta"]["total"],
        4,
        "ops reads everything"
    );
    for tail in [
        format!("/persons/{}", c.neema.person),
        format!("/households/{}", c.neema.household),
        format!("/farms/{}", c.neema.farm),
        format!("/plots/{}", c.neema.plot),
        format!("/crop-cycles/{}", c.neema.cycles[0]),
        format!("/crop-cycles/{}/harvest-reports", c.neema.cycles[0]),
    ] {
        let (status, _) = f.call(Method::GET, &tail, None, &f.officer_mga).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{tail}");
    }
}

#[sqlx::test]
async fn another_workspace_reads_nothing(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let (outsider, _) = signup(&f.app, "out@else.test", "elsewhere").await;
    for tail in ["/persons".to_owned(), format!("/farms/{}", c.neema.farm)] {
        let (status, _) = f.call(Method::GET, &tail, None, &outsider).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{tail}");
    }
    let (status, _) = send(&f.app, Method::GET, &f.url("/persons"), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn farm_managers_are_read_within_scope(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    let managers = |list: &Value| -> usize {
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|farm| farm["managers"].as_array().unwrap().len())
            .sum()
    };
    let mga = f.list("/farms", &f.officer_mga).await;
    assert_eq!(managers(&mga), 1, "exactly the Mgama farm manager");
    let ilu = f.list("/farms", &f.officer_ilu).await;
    assert_eq!(managers(&ilu), 3, "Neema, Rehema, Asha; no Mgama manager");
    assert_eq!(managers(&f.list("/farms", &f.ops).await), 4);
    let own = f.list("/farms", &neema).await;
    assert_eq!(managers(&own), 2, "her own farm's managers");
    let _ = c;
}

#[sqlx::test]
async fn mixed_membership_gives_staff_reads_only_where_staff(pool: PgPool) {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    // Asha is a farmer in Ilundo and a field officer in Mgama.
    let asha = f
        .farmer_for("asha@ruaha.test", &f.ilundo, &c.asha.person)
        .await;
    common::join_workspace(&f.pool, &f.ws, "asha@ruaha.test").await;
    let asha_id = f.user_id("asha@ruaha.test").await;
    f.grant(&f.owner, &asha_id, "field_officer", Some(&f.mgama))
        .await;

    assert_eq!(
        ids(&f.list("/households", &asha).await),
        sorted(vec![&c.asha.household, &c.joseph.household]),
        "own household as farmer, Mgama as staff, not the Mwakalinga household"
    );
    let farms = f.list("/farms", &asha).await;
    assert_eq!(ids(&farms), sorted(vec![&c.asha.farm, &c.joseph.farm]));
    let (status, house) = f
        .call(
            Method::GET,
            &format!("/households/{}", c.joseph.household),
            None,
            &asha,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(house["data"]["members"].as_array().unwrap().len(), 1);
    let (status, _) = f
        .call(
            Method::GET,
            &format!("/households/{}", c.neema.household),
            None,
            &asha,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn names_of_actors_resolve_only_on_visible_records(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    let (status, farm) = f
        .call(
            Method::GET,
            &format!("/farms/{}", c.neema.farm),
            None,
            &neema,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        farm["data"]["provenance"]["captured_by"]["name"],
        "Ada Lovelace"
    );
    let (status, other) = f
        .call(
            Method::GET,
            &format!("/persons/{}", c.joseph.person),
            None,
            &neema,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{other}");
}

#[sqlx::test]
async fn phone_and_gps_are_for_village_staff_only(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    for (tail, keys) in [
        ("/persons", &["phone"][..]),
        ("/farms", &["latitude", "longitude"][..]),
        ("/plots", &["latitude", "longitude"][..]),
    ] {
        let list = f.list(tail, &neema).await;
        for row in list["data"].as_array().unwrap() {
            for k in keys {
                assert!(row[k].is_null(), "{tail}.{k} leaked to a farmer: {row}");
            }
        }
    }
    let staff = f.list("/persons", &f.officer_ilu).await;
    assert!(
        staff["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["phone"].is_string())
    );
    let farm = f.list("/farms", &f.officer_ilu).await;
    assert!(
        farm["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["latitude"].is_string())
    );
    let _ = c;
}

#[sqlx::test]
async fn a_farm_only_user_cannot_read_workspace_users(pool: PgPool) {
    let (f, _, neema) = setup(pool).await;
    let uri = format!("/v1/workspace/{}/users", f.ws);
    let (status, _) = send(&f.app, Method::GET, &uri, None, Some(&neema)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn soft_deleted_records_disappear_from_reads(pool: PgPool) {
    let (f, c, _) = setup(pool.clone()).await;
    sqlx::query("UPDATE farms SET deleted_at = now() WHERE id = $1::uuid")
        .bind(&c.asha.farm)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!ids(&f.list("/farms", &f.ops).await).contains(&c.asha.farm));
    let (status, _) = f
        .call(
            Method::GET,
            &format!("/farms/{}", c.asha.farm),
            None,
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---- verification ----

#[sqlx::test]
async fn verification_stamps_the_verifier_and_is_one_way(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    let tail = format!("/farms/{}/verification", c.neema.farm);
    let (status, _) = f.call(Method::POST, &tail, None, &neema).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a farmer cannot verify");
    let (status, _) = f.call(Method::POST, &tail, None, &f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "out of village");
    let (status, res) = f.call(Method::POST, &tail, None, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let p = &res["data"]["provenance"];
    assert_eq!(p["verification"], "verified");
    assert_eq!(
        p["verified_by"]["id"],
        f.user_id("salima@ruaha.test").await.as_str()
    );
    assert!(p["verified_at"].is_string());
    let (status, res) = f.call(Method::POST, &tail, None, &f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["message"], "record is not awaiting verification");
    let missing = "/farms/019a0000-0000-7000-8000-000000000999/verification";
    let (status, _) = f.call(Method::POST, missing, None, &f.ops).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Only the record kinds have a verification route.
    let (status, _) = f
        .call(
            Method::POST,
            &format!("/villages/{}/verification", f.ilundo),
            None,
            &f.ops,
        )
        .await;
    assert!(status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED);
}

#[sqlx::test]
async fn a_household_needs_a_second_pair_of_eyes(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let (status, res) = f
        .call(
            Method::POST,
            &format!("/households/{}/verification", c.neema.household),
            None,
            &f.officer_ilu,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "a household must be verified by someone other than the officer who registered it"
    );
    let (status, _) = f
        .call(
            Method::POST,
            &format!("/persons/{}/verification", c.neema.person),
            None,
            &f.officer_ilu,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the registrar may still verify the person"
    );
    let (status, _) = f
        .call(
            Method::POST,
            &format!("/households/{}/verification", c.neema.household),
            None,
            &f.ops,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a second staff member verifies the household"
    );
}

// ---- corrections ----

#[sqlx::test]
async fn a_correction_resets_verification_and_stamps_provenance(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let one = format!("/persons/{}", c.neema.person);
    f.call(Method::POST, &format!("{one}/verification"), None, &f.ops)
        .await;
    let body =
        json!({ "given_name": " Neema ", "family_name": "Mwakalinga", "phone": "0712999999" });
    let (status, res) = f.call(Method::PUT, &one, Some(body), &f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_eq!(d["given_name"], "Neema");
    assert_eq!(d["phone"], "+255712999999");
    let p = &d["provenance"];
    assert_eq!(p["verification"], "unverified");
    assert!(p["verified_by"].is_null());
    assert_eq!(p["source"], "field_verified");
    assert_eq!(
        p["captured_by"]["id"],
        f.user_id("salima@ruaha.test").await.as_str()
    );
}

#[sqlx::test]
async fn correction_rules(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    let person = format!("/persons/{}", c.neema.person);
    let farm = format!("/farms/{}", c.neema.farm);
    let ok_person = json!({ "given_name": "Neema", "family_name": "Mwakalinga", "phone": null });
    let (status, _) = f
        .call(Method::PUT, &person, Some(ok_person.clone()), &neema)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "farmers are read-only");
    let (status, _) = f
        .call(Method::PUT, &person, Some(ok_person), &f.officer_mga)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "out of village");
    for (tail, body) in [
        (
            &person,
            json!({ "given_name": "Neema", "family_name": "M", "verification": "verified" }),
        ),
        (
            &person,
            json!({ "given_name": "Neema", "family_name": "M", "user_id": f.user_id("ops@ruaha.test").await }),
        ),
        (
            &person,
            json!({ "given_name": "Neema", "family_name": "M", "source": "sensor_derived" }),
        ),
        (&person, json!({ "given_name": " ", "family_name": "M" })),
        (
            &farm,
            json!({ "label": "Farm", "latitude": "91", "longitude": "35" }),
        ),
    ] {
        let (status, res) = f
            .call(Method::PUT, tail, Some(body.clone()), &f.officer_ilu)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
    }
}

#[sqlx::test]
async fn a_cycle_correction_follows_the_crop_measure(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let tail = format!("/crop-cycles/{}", c.neema.cycles[1]);
    let body = json!({ "crop_id": c.banana, "area_ha": "1", "status": "growing" });
    let (status, res) = f.call(Method::PUT, &tail, Some(body), &f.officer_ilu).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let body = json!({ "crop_id": c.banana, "tree_count": 45, "status": "growing",
                       "harvest_start": "2026-10-01", "harvest_end": "2026-09-01" });
    let (status, res) = f.call(Method::PUT, &tail, Some(body), &f.officer_ilu).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let body = json!({ "crop_id": c.banana, "tree_count": 45, "status": "growing" });
    let (status, res) = f.call(Method::PUT, &tail, Some(body), &f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["tree_count"], 45);
}

// ---- harvests ----

#[sqlx::test]
async fn a_harvest_revision_keeps_one_current_row_and_the_history(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    let tail = format!("/crop-cycles/{}/harvest-reports", c.neema.cycles[0]);
    let body = json!({ "kind": "expected", "quantity_kg": "999", "source": "field_verified", "confidence": "high" });
    let (status, _) = f
        .call(Method::POST, &tail, Some(body.clone()), &neema)
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a farmer cannot revise a figure"
    );
    let (status, res) = f
        .call(Method::POST, &tail, Some(body), &f.officer_ilu)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["quantity_kg"], "999.00");
    assert_eq!(res["data"]["is_current"], true);

    let all = f.list(&tail, &f.officer_ilu).await;
    assert_eq!(all["meta"]["total"], 2, "old row kept");
    let current = f
        .list(&format!("{tail}?current=true"), &f.officer_ilu)
        .await;
    assert_eq!(current["meta"]["total"], 1);
    assert_eq!(current["data"][0]["quantity_kg"], "999.00");
    let old = all["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["is_current"] == false)
        .unwrap();
    assert_eq!(old["quantity_kg"], "3200.00");

    // The farmer reads her own figures too.
    assert_eq!(f.list(&tail, &neema).await["meta"]["total"], 2);
    let actual = json!({ "kind": "actual", "quantity_kg": "2900", "source": "field_verified" });
    let (status, _) = f
        .call(Method::POST, &tail, Some(actual), &f.officer_ilu)
        .await;
    assert_eq!(status, StatusCode::CREATED, "actual is its own series");
    assert_eq!(
        f.list(&format!("{tail}?current=true"), &neema).await["meta"]["total"],
        2
    );
}

#[sqlx::test]
async fn harvest_reports_verify_like_any_record(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    let tail = format!("/harvest-reports/{}/verification", c.neema.harvests[0]);
    let (status, res) = f.call(Method::POST, &tail, None, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["provenance"]["verification"], "verified");
}

// ---- crops ----

#[sqlx::test]
async fn crops_are_config_ops_writes_everyone_reads(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    assert_eq!(f.list("/crops", &neema).await["meta"]["total"], 3);
    let body = json!({ "code": "COFFEE", "name_en": "Coffee", "name_sw": "Kahawa" });
    let (status, _) = f
        .call(Method::POST, "/crops", Some(body.clone()), &f.officer_ilu)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let dup = json!({ "code": "MAIZE", "name_en": "Maize", "name_sw": "Mahindi" });
    let (status, res) = f.call(Method::POST, "/crops", Some(dup), &f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (status, res) = f
        .call(
            Method::PATCH,
            &format!("/crops/{}", c.poultry),
            Some(json!({ "is_active": false })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["is_active"], false);
}

#[sqlx::test]
async fn a_harvest_revision_with_a_client_ref_replays_and_refuses_a_different_body(pool: PgPool) {
    let (f, c, _) = setup(pool.clone()).await;
    let tail = format!("/crop-cycles/{}/harvest-reports", c.neema.cycles[0]);
    let r = common::farm::client_ref();
    let body = json!({ "kind": "actual", "quantity_kg": "2900", "source": "field_verified", "client_ref": r });
    let (status, first) = f
        .call(Method::POST, &tail, Some(body.clone()), &f.officer_ilu)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, again) = f
        .call(Method::POST, &tail, Some(body), &f.officer_ilu)
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the same ref returns the same revision: {again}"
    );
    assert_eq!(again["data"]["id"], first["data"]["id"]);
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM farm_harvest_reports WHERE kind = 'actual'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 1, "a replay writes nothing");

    let changed = json!({ "kind": "actual", "quantity_kg": "3000", "source": "field_verified", "client_ref": r });
    let (status, _) = f
        .call(Method::POST, &tail, Some(changed), &f.officer_ilu)
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "the same ref with another body"
    );
    let other = format!("/crop-cycles/{}/harvest-reports", c.asha.cycles[0]);
    let elsewhere = json!({ "kind": "actual", "quantity_kg": "2900", "source": "field_verified", "client_ref": r });
    let (status, _) = f
        .call(Method::POST, &other, Some(elsewhere), &f.officer_ilu)
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "the same ref on another cycle"
    );
}

#[sqlx::test]
async fn lists_filter_by_verification_farm_and_plot_within_scope(pool: PgPool) {
    let (f, c, _) = setup(pool).await;
    f.verify("persons", &c.neema.person, &f.ops).await;
    f.verify("farms", &c.asha.farm, &f.ops).await;
    let unverified = f
        .list("/persons?verification=unverified,pending", &f.officer_ilu)
        .await;
    assert_eq!(
        ids(&unverified),
        sorted(vec![&c.rehema.person, &c.asha.person])
    );
    let verified = f.list("/farms?verification=verified", &f.ops).await;
    assert_eq!(ids(&verified), vec![c.asha.farm.clone()]);
    assert_eq!(
        f.list("/households?verification=unverified", &f.officer_mga)
            .await["meta"]["total"],
        1
    );
    assert_eq!(
        f.list("/plots?verification=verified", &f.ops).await["meta"]["total"],
        0
    );
    let (status, res) = f
        .call(Method::GET, "/persons?verification=maybe", None, &f.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");

    let plots = f
        .list(&format!("/plots?farm_id={}", c.neema.farm), &f.officer_ilu)
        .await;
    assert_eq!(ids(&plots), sorted(vec![&c.neema.plot, &c.rehema.plot]));
    let cycles = f
        .list(
            &format!("/crop-cycles?plot_id={}", c.neema.plot),
            &f.officer_ilu,
        )
        .await;
    assert_eq!(cycles["meta"]["total"], 2);
    // A filter never widens the scope: another village's farm gives an empty list.
    let foreign = f
        .list(&format!("/plots?farm_id={}", c.neema.farm), &f.officer_mga)
        .await;
    assert_eq!(foreign["meta"]["total"], 0);
    let foreign = f
        .list(
            &format!("/crop-cycles?plot_id={}", c.neema.plot),
            &f.officer_mga,
        )
        .await;
    assert_eq!(foreign["meta"]["total"], 0);
}

#[sqlx::test]
async fn holdings_read_a_persons_households_farms_plots_and_cycles_in_one_call(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    let tail = format!("/persons/{}/holdings", c.neema.person);
    for token in [&f.officer_ilu, &neema] {
        let (status, res) = f.call(Method::GET, &tail, None, token).await;
        assert_eq!(status, StatusCode::OK, "{res}");
        let d = &res["data"];
        let house = &d["households"][0];
        assert_eq!(house["id"], c.neema.household.as_str());
        let names: Vec<&str> = house["people"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["given_name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Neema", "Rehema"]);
        let farm = &d["farms"][0];
        assert_eq!(farm["id"], c.neema.farm.as_str());
        assert_eq!(farm["plots"].as_array().unwrap().len(), 2);
        let riverside = farm["plots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == c.neema.plot.as_str())
            .unwrap();
        let cycles = riverside["cycles"].as_array().unwrap();
        assert_eq!(cycles.len(), 2);
        let maize = cycles
            .iter()
            .find(|cy| cy["crop"]["name_en"] == "MAIZE")
            .unwrap();
        assert_eq!(maize["harvests"][0]["quantity_kg"], "3200.00");
    }
    // A farmer's holdings view keeps phone and GPS for staff only.
    let (_, own) = f.call(Method::GET, &tail, None, &neema).await;
    assert!(own["data"]["farms"][0]["latitude"].is_null());

    let (status, _) = f.call(Method::GET, &tail, None, &f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "another village");
    let (status, _) = f
        .call(
            Method::GET,
            &format!("/persons/{}/holdings", c.asha.person),
            None,
            &neema,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "another household");
    let (outsider, _) = signup(&f.app, "out@else.test", "elsewhere").await;
    let (status, _) = f.call(Method::GET, &tail, None, &outsider).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "another workspace");
}

// ---- persons search ----

#[sqlx::test]
async fn persons_search_by_name_and_phone_within_scope(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    let q = |q: &str| format!("/persons?q={}", urlencode(q));
    let hits = |list: Value| (list["meta"]["total"].as_i64().unwrap(), ids(&list));

    assert_eq!(
        hits(f.list(&q("mwakal"), &f.officer_ilu).await),
        (2, sorted(vec![&c.neema.person, &c.rehema.person])),
        "a family name part"
    );
    for term in ["NEEMA", "neema mwakalinga", " Neema "] {
        assert_eq!(
            hits(f.list(&q(term), &f.officer_ilu).await),
            (1, vec![c.neema.person.clone()]),
            "{term}"
        );
    }
    for phone in ["0712000003", "+255712000003", "712 000 003", "255712000003"] {
        assert_eq!(
            hits(f.list(&q(phone), &f.officer_ilu).await),
            (1, vec![c.asha.person.clone()]),
            "{phone}"
        );
    }
    assert_eq!(
        f.list(&q("mlowe"), &f.officer_ilu).await["meta"]["total"],
        0,
        "an officer never finds another village's person"
    );
    assert_eq!(f.list(&q("mlowe"), &f.ops).await["meta"]["total"], 1);
    assert_eq!(
        f.list(&q("%"), &f.ops).await["meta"]["total"],
        0,
        "wildcards are literal"
    );
    assert_eq!(
        f.list(&q(""), &f.ops).await["meta"]["total"],
        4,
        "blank = no filter"
    );
    assert_eq!(
        f.list(&format!("{}&verification=verified", q("mwakal")), &f.ops)
            .await["meta"]["total"],
        0,
        "combines with the other filters"
    );

    // A farmer reads her household's people but not their phones, so cannot search by them.
    assert_eq!(
        hits(f.list(&q("rehema"), &neema).await),
        (1, vec![c.rehema.person.clone()])
    );
    assert_eq!(
        f.list(&q("0712000002"), &neema).await["meta"]["total"],
        0,
        "a hidden phone is not searchable"
    );

    let (status, res) = f
        .call(Method::GET, &q(&"a".repeat(101)), None, &f.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["q"][0],
        "The q must not be greater than 100 characters."
    );

    let (outsider, _) = signup(&f.app, "out@else.test", "elsewhere").await;
    let (status, _) = f.call(Method::GET, &q("neema"), None, &outsider).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another workspace is refused"
    );
}

/// Percent-encode a query value (spaces, `+`, `%`).
fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ---- workspace harvest reports ----

#[sqlx::test]
async fn the_harvest_report_list_follows_the_cycle_rules(pool: PgPool) {
    let (f, c, neema) = setup(pool).await;
    let revise = |cycle: &str, kg: &str| {
        (
            format!("/crop-cycles/{cycle}/harvest-reports"),
            json!({ "kind": "expected", "quantity_kg": kg, "source": "field_verified" }),
        )
    };
    let (tail, body) = revise(&c.asha.cycles[0], "50");
    let (status, res) = f
        .call(Method::POST, &tail, Some(body), &f.officer_ilu)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let asha = res["data"]["id"].as_str().unwrap().to_owned();
    let (tail, body) = revise(&c.neema.cycles[0], "2800");
    let (status, res) = f
        .call(Method::POST, &tail, Some(body), &f.officer_ilu)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let newest = res["data"]["id"].as_str().unwrap().to_owned();
    let old = c.neema.harvests[0].clone();
    let joseph = c.joseph.harvests[0].clone();

    let list = f.list("/harvest-reports", &f.officer_ilu).await;
    assert_eq!(
        ids(&list),
        sorted(vec![&old, &newest, &asha]),
        "an officer reads their village's figures, superseded ones included"
    );
    assert_eq!(list["data"][0]["id"], newest.as_str(), "newest first");
    assert_eq!(
        ids(&f
            .list("/harvest-reports?current=true", &f.officer_ilu)
            .await),
        sorted(vec![&newest, &asha])
    );
    assert_eq!(
        ids(&f.list("/harvest-reports", &f.officer_mga).await),
        vec![joseph.clone()]
    );
    assert_eq!(
        ids(&f.list("/harvest-reports", &neema).await),
        sorted(vec![&old, &newest]),
        "a farmer reads her own farm's figures, not a neighbour's"
    );
    assert_eq!(f.list("/harvest-reports", &f.ops).await["meta"]["total"], 4);
    assert_eq!(
        ids(&f
            .list(&format!("/harvest-reports?village_id={}", f.mgama), &f.ops)
            .await),
        vec![joseph.clone()]
    );

    let (status, _) = f
        .call(
            Method::POST,
            &format!("/harvest-reports/{joseph}/verification"),
            None,
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let open = f
        .list("/harvest-reports?verification=unverified,pending", &f.ops)
        .await;
    assert_eq!(ids(&open), sorted(vec![&old, &newest, &asha]));

    let page = f
        .list("/harvest-reports?page_size=1&page=2", &f.officer_ilu)
        .await;
    assert_eq!(page["meta"]["total"], 3);
    assert_eq!(page["data"].as_array().unwrap().len(), 1);

    let (status, res) = f
        .call(
            Method::GET,
            "/harvest-reports?verification=nope",
            None,
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["verification"][0],
        "Unknown verification state nope."
    );

    let (outsider, _) = signup(&f.app, "out@else.test", "elsewhere").await;
    let (status, _) = f
        .call(Method::GET, "/harvest-reports", None, &outsider)
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another workspace is refused"
    );
}
