//! Farm productive-use (PUE) requests: a farmer asks to rent or buy a machine, ops reviews it,
//! and the database keeps a Level 1 energy estimate (rated kW × quantity × hours × days)
//! that nobody can hand-write. Villages show prospective and approved load against their
//! capacity. RUAHA ids in docs/ruaha-rls-mapping.md.

mod common;

use axum::http::{Method, StatusCode};
use common::{
    farm::{Cast, Farm},
    send, signup,
};
use serde_json::{Value, json};
use sqlx::PgPool;

struct Pue {
    f: Farm,
    c: Cast,
    /// 15 kW maize mill, buy only, 2 500 000.00 TZS.
    mill: String,
    /// 1.5 kW pump, rent only, 15 000.00 TZS a day, 6 h × 6 days typical.
    pump: String,
    /// A machine with no rated power: no estimate.
    sheller: String,
    /// Neema, a signed-in Ilundo farmer.
    neema: String,
}

async fn machine(f: &Farm, category: &str, tzs: &str, body: Value) -> String {
    let mut body = body;
    body["category_id"] = json!(category);
    body["currency_id"] = json!(tzs);
    let (status, res) = f.call(Method::POST, "/equipment", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["id"].as_str().unwrap().to_owned()
}

async fn pue(pool: PgPool) -> Pue {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let tzs = f.tzs().await;
    let (status, res) = f
        .call(
            Method::POST,
            "/equipment-categories",
            Some(json!({ "code": "AGRO", "name_en": "Agro-processing", "name_sw": "Usindikaji" })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let category = res["data"]["id"].as_str().unwrap().to_owned();
    let mill = machine(
        &f,
        &category,
        &tzs,
        json!({ "code": "MILL", "name_en": "Maize mill", "name_sw": "Kinu",
            "rated_power_kw": "15", "typical_hours_per_day": "8", "typical_days_per_week": "6",
            "indicative_price_minor": 250_000_000 }),
    )
    .await;
    let pump = machine(
        &f,
        &category,
        &tzs,
        json!({ "code": "PUMP", "name_en": "Irrigation pump", "name_sw": "Pampu",
            "rated_power_kw": "1.5", "typical_hours_per_day": "6", "typical_days_per_week": "6",
            "can_rent": true, "can_buy": false, "indicative_rent_per_day_minor": 1_500_000 }),
    )
    .await;
    let sheller = machine(
        &f,
        &category,
        &tzs,
        json!({ "code": "SHELL", "name_en": "Hand sheller", "name_sw": "Kipukuchua",
            "indicative_price_minor": 10_000_000 }),
    )
    .await;
    let neema = f
        .farmer_for("neema@ruaha.test", &f.ilundo, &c.neema.person)
        .await;
    Pue {
        f,
        c,
        mill,
        pump,
        sheller,
        neema,
    }
}

impl Pue {
    async fn request(&self, token: &str, body: Value) -> (StatusCode, Value) {
        self.f
            .call(Method::POST, "/pue-requests", Some(body), token)
            .await
    }

    async fn requested(&self, token: &str, body: Value) -> String {
        let (status, res) = self.request(token, body).await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["id"].as_str().unwrap().to_owned()
    }

    async fn act(&self, id: &str, action: &str, token: &str) -> (StatusCode, Value) {
        let tail = format!("/pue-requests/{id}/transitions");
        self.f
            .call(
                Method::POST,
                &tail,
                Some(json!({ "action": action, "note": "checked" })),
                token,
            )
            .await
    }

    async fn moved(&self, id: &str, actions: &[&str]) {
        for action in actions {
            let (status, res) = self.act(id, action, &self.f.ops).await;
            assert_eq!(status, StatusCode::OK, "{action}: {res}");
        }
    }

    async fn get(&self, id: &str, token: &str) -> (StatusCode, Value) {
        let tail = format!("/pue-requests/{id}");
        self.f.call(Method::GET, &tail, None, token).await
    }
}

fn ids(list: &Value) -> Vec<&str> {
    list["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect()
}

// ---- create ----

#[sqlx::test]
async fn a_farmer_requests_a_machine_for_themselves(pool: PgPool) {
    let p = pue(pool).await;
    let (status, res) = p
        .request(
            &p.neema,
            json!({ "equipment_id": p.mill, "purpose": "Mill for the group" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let r = &res["data"];
    assert_eq!(r["status"], "submitted");
    assert!(r["submitted_at"].is_string(), "{res}");
    assert_eq!(r["acquisition"], "buy");
    assert_eq!(r["quantity"], 1);
    assert_eq!(r["village_id"], p.f.ilundo);
    assert_eq!(r["person"]["id"], p.c.neema.person);
    assert_eq!(r["person"]["name"], "Neema Mwakalinga");
    assert_eq!(r["equipment"]["id"], p.mill);
    assert_eq!(r["equipment"]["code"], "MILL");
    assert_eq!(r["purpose"], "Mill for the group");
    assert_eq!(r["decided_at"], Value::Null);
    // Hours and days fall back to the catalogue's typical figures.
    let e = &r["estimate"];
    assert_eq!(e["rated_power_kw"], "15.000", "{res}");
    assert_eq!(e["hours_per_day"], "8.00");
    assert_eq!(e["days_per_week"], "6.0");
    assert_eq!(e["est_power_kw"], "15.000");
    assert_eq!(e["est_kwh_per_day"], "120.000");
    assert_eq!(e["est_kwh_per_week"], "720.000");
    assert_eq!(e["method"], "level1_rated_x_hours");
    assert_eq!(e["source"], "model_estimated");
}

#[sqlx::test]
async fn request_provenance_is_normalised_and_estimates_are_never_hand_written(pool: PgPool) {
    let p = pue(pool).await;
    let (status, res) = p
        .request(
            &p.neema,
            json!({ "equipment_id": p.mill, "source": "field_verified",
                "verification": "verified", "captured_by": "019a0000-0000-7000-8000-000000000001",
                "decided_by": "019a0000-0000-7000-8000-000000000001", "decided_at": "2026-01-01T00:00:00Z",
                "estimate": { "est_power_kw": "0.001", "rated_power_kw": "0.001" } }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let r = &res["data"];
    assert_eq!(r["provenance"]["source"], "farmer_reported");
    assert_eq!(r["provenance"]["verification"], "unverified");
    assert_eq!(
        r["provenance"]["captured_by"]["id"],
        p.f.user_id("neema@ruaha.test").await
    );
    assert_eq!(r["decided_by"], Value::Null);
    assert_eq!(r["decided_at"], Value::Null);
    assert_eq!(r["estimate"]["est_power_kw"], "15.000");
}

#[sqlx::test]
async fn a_request_is_created_only_as_draft_or_submitted(pool: PgPool) {
    let p = pue(pool).await;
    for status in ["approved", "under_review", "withdrawn", "rejected"] {
        let (code, res) = p
            .request(
                &p.neema,
                json!({ "equipment_id": p.mill, "status": status }),
            )
            .await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{status}: {res}");
        assert_eq!(
            res["message"], "a request may only be created as draft or submitted",
            "{res}"
        );
    }
    let (code, res) = p
        .request(
            &p.neema,
            json!({ "equipment_id": p.mill, "status": "draft" }),
        )
        .await;
    assert_eq!(code, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["status"], "draft");
    assert_eq!(res["data"]["submitted_at"], Value::Null);
}

#[sqlx::test]
async fn a_machine_is_requested_only_as_the_catalogue_offers_it(pool: PgPool) {
    let p = pue(pool).await;
    let (status, res) = p
        .request(
            &p.neema,
            json!({ "equipment_id": p.mill, "acquisition": "rent" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], "this equipment is not offered for rent");
    let (status, res) = p
        .request(
            &p.neema,
            json!({ "equipment_id": p.pump, "acquisition": "buy" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], "this equipment is not offered for sale");
    let (status, res) = p
        .request(
            &p.neema,
            json!({ "equipment_id": p.pump, "acquisition": "rent" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["acquisition"], "rent");
}

#[sqlx::test]
async fn request_figures_are_checked(pool: PgPool) {
    let p = pue(pool).await;
    for (body, field) in [
        (json!({}), "equipment_id"),
        (json!({ "equipment_id": "nope" }), "equipment_id"),
        (
            json!({ "equipment_id": "019a0000-0000-7000-8000-0000000000ff" }),
            "equipment_id",
        ),
        (json!({ "equipment_id": p.mill, "quantity": 0 }), "quantity"),
        (
            json!({ "equipment_id": p.mill, "hours_per_day": "25" }),
            "hours_per_day",
        ),
        (
            json!({ "equipment_id": p.mill, "days_per_week": "7.5" }),
            "days_per_week",
        ),
        (
            json!({ "equipment_id": p.mill, "acquisition": "lease" }),
            "acquisition",
        ),
    ] {
        let (status, res) = p.request(&p.neema, body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert!(res["errors"][field].is_array(), "{body}: {res}");
    }
}

#[sqlx::test]
async fn a_retried_create_replays_and_a_changed_body_conflicts(pool: PgPool) {
    let p = pue(pool).await;
    let id = "019a0000-0000-7000-8000-0000000000e1";
    let body = json!({ "id": id, "equipment_id": p.mill, "quantity": 1 });
    let (status, _) = p.request(&p.neema, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, res) = p.request(&p.neema, body).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["id"], id);
    let (status, res) = p
        .request(
            &p.neema,
            json!({ "id": id, "equipment_id": p.mill, "quantity": 2 }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
    assert_eq!(res["message"], "This id is already in use.");
}

#[sqlx::test]
async fn staff_file_a_request_for_a_person_of_their_village(pool: PgPool) {
    let p = pue(pool).await;
    let body =
        json!({ "equipment_id": p.mill, "person_id": p.c.neema.person, "farm_id": p.c.neema.farm });
    let (status, res) = p.request(&p.f.officer_ilu, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["person"]["id"], p.c.neema.person);
    assert_eq!(res["data"]["farm_id"], p.c.neema.farm);
    assert_eq!(res["data"]["village_id"], p.f.ilundo);
    // The Mgama officer cannot file for an Ilundo person.
    let (status, res) = p.request(&p.f.officer_mga, body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["person_id"].is_array(), "{res}");
    // A farm of another village is refused.
    let (status, res) = p
        .request(
            &p.f.officer_ilu,
            json!({ "equipment_id": p.mill, "person_id": p.c.neema.person, "farm_id": p.c.joseph.farm }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["farm_id"].is_array(), "{res}");
    // Staff must name the person.
    let (status, res) = p
        .request(&p.f.officer_ilu, json!({ "equipment_id": p.mill }))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["person_id"].is_array(), "{res}");
}

#[sqlx::test]
async fn a_farmer_always_requests_as_themselves(pool: PgPool) {
    let p = pue(pool).await;
    let (status, res) = p
        .request(
            &p.neema,
            json!({ "equipment_id": p.mill, "person_id": p.c.asha.person }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["person"]["id"], p.c.neema.person);
}

// ---- transitions ----

#[sqlx::test]
async fn a_farmer_cannot_review_or_approve_their_own_request(pool: PgPool) {
    let p = pue(pool).await;
    let id = p
        .requested(&p.neema, json!({ "equipment_id": p.mill }))
        .await;
    let (status, res) = p.act(&id, "approve", &p.neema).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], "illegal transition submitted -> approved");
    let (status, res) = p.act(&id, "start_review", &p.neema).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    assert_eq!(res["message"], "only ops or admin may review a request");
    p.moved(&id, &["start_review"]).await;
    let (status, res) = p.act(&id, "approve", &p.neema).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    let (_, res) = p.get(&id, &p.neema).await;
    assert_eq!(res["data"]["status"], "under_review");
}

#[sqlx::test]
async fn ops_review_stamps_the_decision(pool: PgPool) {
    let p = pue(pool).await;
    let id = p
        .requested(&p.neema, json!({ "equipment_id": p.mill }))
        .await;
    p.moved(&id, &["start_review"]).await;
    let (status, res) =
        p.f.call(
            Method::POST,
            &format!("/pue-requests/{id}/transitions"),
            Some(json!({ "action": "approve", "note": "fits the village capacity" })),
            &p.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let r = &res["data"];
    assert_eq!(r["status"], "approved");
    assert!(r["decided_at"].is_string(), "{res}");
    assert_eq!(r["decided_by"]["id"], p.f.user_id("ops@ruaha.test").await);
    assert_eq!(r["decision_note"], "fits the village capacity");
    let (status, res) = p.act(&id, "reject", &p.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], "illegal transition approved -> rejected");
}

#[sqlx::test]
async fn a_farmer_submits_and_withdraws_their_own_request(pool: PgPool) {
    let p = pue(pool).await;
    let draft = p
        .requested(
            &p.neema,
            json!({ "equipment_id": p.mill, "status": "draft" }),
        )
        .await;
    let (status, res) = p.act(&draft, "submit", &p.neema).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["status"], "submitted");
    assert!(res["data"]["submitted_at"].is_string());
    let (status, res) = p.act(&draft, "withdraw", &p.neema).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["status"], "withdrawn");
    let (status, res) = p.act(&draft, "submit", &p.neema).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], "illegal transition withdrawn -> submitted");
    let (status, res) = p.act(&draft, "cancel", &p.neema).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["action"].is_array(), "{res}");
}

// ---- estimates ----

#[sqlx::test]
async fn the_estimate_keeps_the_figures_it_was_computed_from(pool: PgPool) {
    let p = pue(pool).await;
    let (status, res) = p
        .request(
            &p.neema,
            json!({ "equipment_id": p.pump, "acquisition": "rent", "quantity": 2,
                "hours_per_day": "4", "days_per_week": "5" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    let e = &res["data"]["estimate"];
    assert_eq!(e["est_power_kw"], "3.000");
    assert_eq!(e["est_kwh_per_day"], "12.000");
    assert_eq!(e["est_kwh_per_week"], "60.000");
    // The catalogue changes underneath; the stored estimate stays explainable.
    sqlx::query("UPDATE farm_equipment SET rated_power_kw = 9 WHERE id = $1::uuid")
        .bind(&p.pump)
        .execute(&p.f.pool)
        .await
        .unwrap();
    let (_, res) = p.get(&id, &p.neema).await;
    assert_eq!(res["data"]["estimate"]["rated_power_kw"], "1.500");
    assert_eq!(res["data"]["estimate"]["est_power_kw"], "3.000");
    // A status change does not recompute it either.
    p.moved(&id, &["start_review"]).await;
    let (_, res) = p.get(&id, &p.neema).await;
    assert_eq!(res["data"]["estimate"]["est_power_kw"], "3.000");
}

#[sqlx::test]
async fn a_machine_without_rated_power_has_no_estimate(pool: PgPool) {
    let p = pue(pool).await;
    let (status, res) = p
        .request(&p.neema, json!({ "equipment_id": p.sheller }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["estimate"], Value::Null);
}

// ---- who reads ----

#[sqlx::test]
async fn requests_are_read_by_their_farmer_and_village_staff(pool: PgPool) {
    let p = pue(pool).await;
    let mine = p
        .requested(&p.neema, json!({ "equipment_id": p.mill }))
        .await;
    let asha = p
        .requested(
            &p.f.officer_ilu,
            json!({ "equipment_id": p.mill, "person_id": p.c.asha.person, "status": "draft" }),
        )
        .await;
    let joseph = p
        .requested(
            &p.f.officer_mga,
            json!({ "equipment_id": p.mill, "person_id": p.c.joseph.person }),
        )
        .await;
    assert_eq!(ids(&p.f.list("/pue-requests", &p.neema).await), vec![&mine]);
    let (status, _) = p.get(&asha, &p.neema).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let ilu = p.f.list("/pue-requests", &p.f.officer_ilu).await;
    assert_eq!(ilu["meta"]["total"], 2, "{ilu}");
    assert!(!ids(&ilu).contains(&joseph.as_str()));
    let (status, _) = p.get(&mine, &p.f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = p.act(&mine, "withdraw", &p.f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let all = p.f.list("/pue-requests", &p.f.ops).await;
    assert_eq!(all["meta"]["total"], 3);
    // Newest first, filters by status list and village.
    assert_eq!(ids(&all)[0], joseph);
    let open =
        p.f.list("/pue-requests?status=submitted,under_review", &p.f.ops)
            .await;
    assert_eq!(open["meta"]["total"], 2, "{open}");
    let ilundo =
        p.f.list(
            &format!("/pue-requests?village_id={}", p.f.ilundo),
            &p.f.ops,
        )
        .await;
    assert_eq!(ilundo["meta"]["total"], 2, "{ilundo}");
}

#[sqlx::test]
async fn requests_of_another_workspace_are_404(pool: PgPool) {
    let p = pue(pool).await;
    let id = p
        .requested(&p.neema, json!({ "equipment_id": p.mill }))
        .await;
    let (other, other_ws) = signup(&p.f.app, "other@else.test", "elsewhere").await;
    sqlx::query(
        "INSERT INTO farm_memberships (workspace_id, user_id, role)
         SELECT $1::uuid, id, 'admin' FROM users WHERE email = 'other@else.test'",
    )
    .bind(&other_ws)
    .execute(&p.f.pool)
    .await
    .unwrap();
    let theirs = format!("/v1/workspace/{other_ws}/farm/pue-requests/{id}");
    let (status, _) = send(&p.f.app, Method::GET, &theirs, None, Some(&other)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        &p.f.app,
        Method::POST,
        &format!("{theirs}/transitions"),
        Some(json!({ "action": "start_review" })),
        Some(&other),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&p.f.app, Method::GET, &theirs, None, Some(&p.f.ops)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Our machine is not theirs to request.
    let (status, res) = send(
        &p.f.app,
        Method::POST,
        &format!("/v1/workspace/{other_ws}/farm/pue-requests"),
        Some(json!({ "equipment_id": p.mill, "person_id": p.c.neema.person })),
        Some(&other),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (status, _) = send(&p.f.app, Method::GET, &p.f.url("/pue-requests"), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---- village energy and pipeline ----

#[sqlx::test]
async fn village_energy_keeps_prospective_and_approved_load_apart(pool: PgPool) {
    let p = pue(pool).await;
    let mill = p
        .requested(&p.neema, json!({ "equipment_id": p.mill }))
        .await;
    let pumps = p
        .requested(
            &p.f.officer_ilu,
            json!({ "equipment_id": p.pump, "acquisition": "rent", "quantity": 2,
                "person_id": p.c.asha.person }),
        )
        .await;
    for id in [&mill, &pumps] {
        p.moved(id, &["start_review", "approve"]).await;
    }
    // Still prospective: 1 mill submitted.
    p.requested(&p.neema, json!({ "equipment_id": p.mill }))
        .await;
    let tail = format!("/villages/{}/energy", p.f.ilundo);
    let (status, res) = p.f.call(Method::GET, &tail, None, &p.f.ops).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let e = &res["data"];
    assert_eq!(e["village_id"], p.f.ilundo);
    assert_eq!(e["capacity_kw"], "500.00");
    assert_eq!(e["capacity_basis"], "planned");
    assert_eq!(e["simultaneity_factor"], "0.600");
    assert_eq!(e["approved_kw_raw"], "18.000");
    // (15.0 + 2 × 1.5) × 0.600 = 10.800 kW
    assert_eq!(e["approved_peak_kw"], "10.800");
    assert_eq!(e["prospective_kw_raw"], "15.000");
    assert_eq!(e["prospective_peak_kw"], "9.000");
    assert_eq!(e["headroom_kw"], "489.200");
    // mill 15 × 8 h × 6 d = 720; pumps 3 × 6 h × 6 d = 108
    assert_eq!(e["approved_kwh_per_week"], "828.000");
    assert_eq!(e["approved_assets"], 2);
    // Mgama has its capacity row and no load.
    let tail = format!("/villages/{}/energy", p.f.mgama);
    let (status, res) = p.f.call(Method::GET, &tail, None, &p.f.ops).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["capacity_kw"], "150.00");
    assert_eq!(res["data"]["approved_peak_kw"], "0.000");
    assert_eq!(res["data"]["headroom_kw"], "150.000");
}

#[sqlx::test]
async fn a_new_village_has_energy_figures_from_the_start(pool: PgPool) {
    let f = Farm::new(pool).await;
    let id = f.village("NEW", "New village").await;
    let (status, res) = f
        .call(Method::GET, &format!("/villages/{id}/energy"), None, &f.ops)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["capacity_kw"], "150.00");
    assert_eq!(res["data"]["approved_assets"], 0);
}

#[sqlx::test]
async fn village_energy_and_pipeline_are_for_staff_of_the_village(pool: PgPool) {
    let p = pue(pool).await;
    for kind in ["energy", "pue-pipeline"] {
        let tail = format!("/villages/{}/{kind}", p.f.ilundo);
        let (status, res) = p.f.call(Method::GET, &tail, None, &p.neema).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{kind}: {res}");
        let (status, _) = p.f.call(Method::GET, &tail, None, &p.f.officer_mga).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{kind}");
        let (status, res) = p.f.call(Method::GET, &tail, None, &p.f.officer_ilu).await;
        assert_eq!(status, StatusCode::OK, "{kind}: {res}");
        let tail = format!("/villages/019a0000-0000-7000-8000-0000000000ff/{kind}");
        let (status, _) = p.f.call(Method::GET, &tail, None, &p.f.ops).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{kind}");
    }
}

#[sqlx::test]
async fn the_pipeline_adds_rent_per_day_apart_from_purchase_prices(pool: PgPool) {
    let p = pue(pool).await;
    p.requested(&p.neema, json!({ "equipment_id": p.mill, "quantity": 2 }))
        .await;
    p.requested(
        &p.neema,
        json!({ "equipment_id": p.pump, "acquisition": "rent", "quantity": 3 }),
    )
    .await;
    p.requested(
        &p.neema,
        json!({ "equipment_id": p.mill, "status": "draft" }),
    )
    .await;
    let tail = format!("/villages/{}/pue-pipeline", p.f.ilundo);
    let (status, res) = p.f.call(Method::GET, &tail, None, &p.f.ops).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let rows = res["data"].as_array().unwrap();
    let row = |s: &str| rows.iter().find(|r| r["status"] == s).unwrap();
    let submitted = row("submitted");
    assert_eq!(submitted["request_count"], 2, "{res}");
    assert_eq!(submitted["indicative_value"]["minor"], 500_000_000);
    assert_eq!(submitted["indicative_value"]["currency"], "TZS");
    assert_eq!(submitted["indicative_rent_per_day"]["minor"], 4_500_000);
    let draft = row("draft");
    assert_eq!(draft["request_count"], 1);
    assert_eq!(draft["indicative_rent_per_day"], Value::Null);
}

// ---- additions for the field port ----

#[sqlx::test]
async fn approve_and_reject_need_a_decision_note(pool: PgPool) {
    let p = pue(pool).await;
    let id = p
        .requested(&p.neema, json!({ "equipment_id": p.mill }))
        .await;
    p.moved(&id, &["start_review"]).await;
    let tail = format!("/pue-requests/{id}/transitions");
    for body in [
        json!({ "action": "approve" }),
        json!({ "action": "reject", "note": "  " }),
    ] {
        let (status, res) =
            p.f.call(Method::POST, &tail, Some(body.clone()), &p.f.ops)
                .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {res}");
        assert!(res["errors"]["note"].is_array(), "{body}: {res}");
    }
    let (_, res) = p.get(&id, &p.f.ops).await;
    assert_eq!(res["data"]["status"], "under_review");
    // Other moves need none.
    let draft = p
        .requested(
            &p.neema,
            json!({ "equipment_id": p.mill, "status": "draft" }),
        )
        .await;
    let (status, res) =
        p.f.call(
            Method::POST,
            &format!("/pue-requests/{draft}/transitions"),
            Some(json!({ "action": "submit" })),
            &p.neema,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn a_request_names_its_farm(pool: PgPool) {
    let p = pue(pool).await;
    let (_, res) = p
        .request(
            &p.f.officer_ilu,
            json!({ "equipment_id": p.mill, "person_id": p.c.neema.person, "farm_id": p.c.neema.farm }),
        )
        .await;
    assert_eq!(res["data"]["farm"]["id"], p.c.neema.farm, "{res}");
    assert_eq!(res["data"]["farm"]["label"], "Mwakalinga farm");
    let list = p.f.list("/pue-requests", &p.neema).await;
    assert_eq!(list["data"][0]["farm"]["label"], "Mwakalinga farm");
    let (_, res) = p.request(&p.neema, json!({ "equipment_id": p.mill })).await;
    assert_eq!(res["data"]["farm"], Value::Null);
}

#[sqlx::test]
async fn the_estimate_preview_is_the_estimate_a_request_gets(pool: PgPool) {
    let p = pue(pool).await;
    let inputs = json!({ "equipment_id": p.pump, "quantity": 2, "hours_per_day": "4" });
    let (status, res) =
        p.f.call(
            Method::POST,
            "/pue-requests/estimate",
            Some(inputs),
            &p.neema,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let preview = res["data"].clone();
    assert_eq!(preview["est_power_kw"], "3.000", "{res}");
    assert_eq!(preview["days_per_week"], "6.0");
    assert_eq!(preview["est_kwh_per_week"], "72.000");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_pue_requests")
        .fetch_one(&p.f.pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "a preview writes nothing");
    let (_, res) = p
        .request(
            &p.neema,
            json!({ "equipment_id": p.pump, "acquisition": "rent", "quantity": 2, "hours_per_day": "4" }),
        )
        .await;
    let stored = &res["data"]["estimate"];
    for k in [
        "rated_power_kw",
        "quantity",
        "hours_per_day",
        "days_per_week",
        "est_power_kw",
        "est_kwh_per_day",
        "est_kwh_per_week",
        "method",
    ] {
        assert_eq!(preview[k], stored[k], "{k}");
    }
    // A machine with no rated power gets no estimate; an unknown one is refused.
    let (status, res) =
        p.f.call(
            Method::POST,
            "/pue-requests/estimate",
            Some(json!({ "equipment_id": p.sheller })),
            &p.neema,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"], Value::Null);
    let (status, res) =
        p.f.call(
            Method::POST,
            "/pue-requests/estimate",
            Some(json!({ "equipment_id": "019a0000-0000-7000-8000-0000000000ff" })),
            &p.neema,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["equipment_id"].is_array(), "{res}");
}
