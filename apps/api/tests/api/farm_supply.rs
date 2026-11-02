//! Supply batches: verified actual harvests bought at a price per kg become a raw lot with
//! per-farmer kg shares. Double-commit guard, four-eyes, idempotency, append-only rows.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{
    farm::{Cast, Farm, client_ref},
    send, signup,
};
use serde_json::{Value, json};
use sqlx::PgPool;

struct World {
    f: Farm,
    c: Cast,
    baraka: common::farm::Reg,
    product: String,
    tzs: String,
}

async fn product(f: &Farm, token: &str, ws: &str, name: &str) -> String {
    let (status, res) = send(
        &f.app,
        Method::POST,
        &format!("/v1/workspace/{ws}/products"),
        Some(json!({ "name": name, "price": 10 })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"]["product_id"].as_str().unwrap().to_owned()
}

/// Officer reports an actual figure on `cycle`; the owner verifies it. Its id.
async fn actual(f: &Farm, cycle: &str, kg: &str) -> String {
    let (status, res) = f
        .call(
            Method::POST,
            &format!("/crop-cycles/{cycle}/harvest-reports"),
            Some(json!({ "kind": "actual", "quantity_kg": kg, "source": "field_verified" })),
            &f.officer_ilu,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let id = res["data"]["id"].as_str().unwrap().to_owned();
    f.verify("harvest-reports", &id, &f.owner).await;
    id
}

async fn world(pool: PgPool) -> World {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let (status, res) = f
        .register(
            &f.officer_ilu,
            json!({
                "client_ref": client_ref(), "village_id": f.ilundo,
                "person": { "given_name": "Baraka", "family_name": "Sanga", "phone": "0712000009" },
                "household": { "label": "Sanga household" },
                "farm": { "label": "Sanga farm", "latitude": "-7.73", "longitude": "35.73" },
                "plot": { "label": "Sanga plot", "area_ha": "1.0" },
                "cycles": [{ "crop_id": c.maize, "area_ha": "1.0" }]
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let baraka = common::farm::Reg {
        person: res["data"]["person_id"].as_str().unwrap().to_owned(),
        household: res["data"]["household_id"].as_str().unwrap().to_owned(),
        farm: res["data"]["farm_id"].as_str().unwrap().to_owned(),
        plot: res["data"]["plot_id"].as_str().unwrap().to_owned(),
        cycles: vec![
            res["data"]["crop_cycle_ids"][0]
                .as_str()
                .unwrap()
                .to_owned(),
        ],
        harvests: vec![],
    };
    let product = product(&f, &f.owner, &f.ws, "Raw maize").await;
    let tzs = f.tzs().await;
    World {
        f,
        c,
        baraka,
        product,
        tzs,
    }
}

fn batch(w: &World, code: &str, lines: Value) -> Value {
    json!({
        "client_ref": client_ref(), "village_id": w.f.ilundo, "crop_id": w.c.maize,
        "code": code, "raw_product_id": w.product,
        "price_per_kg": 800000, "currency_id": w.tzs, "lines": lines
    })
}

async fn post(w: &World, body: &Value, token: &str) -> (StatusCode, Value) {
    w.f.call(Method::POST, "/supply-batches", Some(body.clone()), token)
        .await
}

/// Every body posted by ops at once.
async fn parallel(w: &World, bodies: Vec<Value>) -> Vec<(StatusCode, Value)> {
    let mut set = tokio::task::JoinSet::new();
    for b in bodies {
        let (app, url, ops) = (w.f.app.clone(), w.f.url("/supply-batches"), w.f.ops.clone());
        set.spawn(async move { send(&app, Method::POST, &url, Some(b), Some(&ops)).await });
    }
    set.join_all().await
}

fn errors(res: &Value) -> Vec<String> {
    let mut keys: Vec<String> = res["errors"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

#[sqlx::test]
async fn verified_actual_harvests_become_a_raw_lot_with_farmer_shares(pool: PgPool) {
    let w = world(pool).await;
    let neema = actual(&w.f, &w.c.neema.cycles[0], "300").await;
    let baraka = actual(&w.f, &w.baraka.cycles[0], "120.5").await;
    let body = batch(
        &w,
        "ILU-M-01",
        json!([{ "harvest_report_id": neema, "kg": "250" },
               { "harvest_report_id": baraka, "kg": "120.50" }]),
    );

    let (status, res) = post(&w, &body, &w.f.ops).await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    let d = &res["data"];
    assert_eq!(d["kg_total"], "370.50");
    assert_eq!(d["price_per_kg"], 800000);
    assert_eq!(d["lot"]["code"], "ILU-M-01");
    let lines = d["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    let by_person = |p: &str| {
        lines
            .iter()
            .find(|l| l["person_id"] == p)
            .map(|l| l["kg"].clone())
    };
    assert_eq!(by_person(&w.c.neema.person), Some(json!("250.00")));
    assert_eq!(by_person(&w.baraka.person), Some(json!("120.50")));

    let lot = d["lot"]["id"].as_str().unwrap();
    let (status, got) = send(
        &w.f.app,
        Method::GET,
        &format!("/v1/workspace/{}/lots/{lot}", w.f.ws),
        None,
        Some(&w.f.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(got["data"]["product_id"], w.product.as_str());

    let id = d["id"].as_str().unwrap();
    let (status, read) =
        w.f.call(
            Method::GET,
            &format!("/supply-batches/{id}"),
            None,
            &w.f.officer_ilu,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["data"], res["data"]);
}

#[sqlx::test]
async fn same_client_ref_replays_and_another_body_conflicts(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "300").await;
    let body = batch(
        &w,
        "ILU-M-02",
        json!([{ "harvest_report_id": h, "kg": "100" }]),
    );
    let (status, first) = post(&w, &body, &w.f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");

    let mut same = body.clone();
    same["lines"][0]["kg"] = json!("100.00");
    let (status, again) = post(&w, &same, &w.f.ops).await;
    assert_eq!(status, StatusCode::OK, "kg 100 == 100.00: {again}");
    assert_eq!(again["data"], first["data"]);

    let mut other = body.clone();
    other["lines"][0]["kg"] = json!("101");
    let (status, res) = post(&w, &other, &w.f.ops).await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");

    let (status, _) = post(&w, &body, &w.f.owner).await;
    assert_eq!(status, StatusCode::CONFLICT, "another member's client_ref");

    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_supply_batches")
        .fetch_one(&w.f.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test]
async fn parallel_copies_of_one_batch_write_it_once(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "300").await;
    let body = batch(
        &w,
        "ILU-M-03",
        json!([{ "harvest_report_id": h, "kg": "10" }]),
    );
    let results = parallel(&w, vec![body; 8]).await;
    let ids: std::collections::BTreeSet<_> = results
        .iter()
        .map(|(s, r)| {
            assert!(s.is_success(), "{s} {r}");
            r["data"]["id"].as_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(ids.len(), 1);
    let lots: i64 = sqlx::query_scalar("SELECT count(*) FROM lots")
        .fetch_one(&w.f.pool)
        .await
        .unwrap();
    assert_eq!(lots, 1);
}

#[sqlx::test]
async fn kg_beyond_the_harvest_is_refused_with_ruahas_message(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "100").await;
    let (status, _) = post(
        &w,
        &batch(
            &w,
            "ILU-M-04",
            json!([{ "harvest_report_id": h, "kg": "70" }]),
        ),
        &w.f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, res) = post(
        &w,
        &batch(
            &w,
            "ILU-M-05",
            json!([{ "harvest_report_id": h, "kg": "30.01" }]),
        ),
        &w.f.ops,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["lines.0.kg"][0],
        "over-commitment: 100.00 kg available, 70.00 kg already committed, 30.01 kg requested"
    );
    let (status, _) = post(
        &w,
        &batch(
            &w,
            "ILU-M-06",
            json!([{ "harvest_report_id": h, "kg": "30" }]),
        ),
        &w.f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "exactly the rest fits");
}

#[sqlx::test]
async fn parallel_batches_never_overcommit_one_harvest(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "100").await;
    let bodies: Vec<Value> = (0..5)
        .map(|i| {
            batch(
                &w,
                &format!("ILU-P-{i}"),
                json!([{ "harvest_report_id": h, "kg": "30" }]),
            )
        })
        .collect();
    let results = parallel(&w, bodies).await;
    let created = results
        .iter()
        .filter(|(s, _)| *s == StatusCode::CREATED)
        .count();
    assert_eq!(created, 3, "{results:?}");
    let kg: String = sqlx::query_scalar("SELECT farm_harvest_committed_kg($1::uuid)::text")
        .bind(&h)
        .fetch_one(&w.f.pool)
        .await
        .unwrap();
    assert_eq!(kg, "90.00");
}

#[sqlx::test]
async fn only_current_verified_actual_harvests_of_the_village_and_crop(pool: PgPool) {
    let w = world(pool).await;
    let f = &w.f;
    // Registration figure: expected, unverified.
    let expected = w.c.neema.harvests[0].clone();
    // Unverified actual.
    let (_, res) = f
        .call(
            Method::POST,
            &format!("/crop-cycles/{}/harvest-reports", w.baraka.cycles[0]),
            Some(json!({ "kind": "actual", "quantity_kg": "50", "source": "field_verified" })),
            &f.officer_ilu,
        )
        .await;
    let unverified = res["data"]["id"].as_str().unwrap().to_owned();
    // Superseded actual (a newer figure replaced it).
    let old = actual(f, &w.c.neema.cycles[0], "100").await;
    actual(f, &w.c.neema.cycles[0], "110").await;
    // Joseph is in Mgama.
    let mgama = {
        let (_, res) = f
            .call(
                Method::POST,
                &format!("/crop-cycles/{}/harvest-reports", w.c.joseph.cycles[0]),
                Some(json!({ "kind": "actual", "quantity_kg": "50", "source": "field_verified" })),
                &f.officer_mga,
            )
            .await;
        let id = res["data"]["id"].as_str().unwrap().to_owned();
        f.verify("harvest-reports", &id, &f.owner).await;
        id
    };

    for (h, why) in [
        (expected.as_str(), "expected"),
        (unverified.as_str(), "unverified"),
        (old.as_str(), "superseded"),
        (mgama.as_str(), "other village"),
        ("01900000-0000-7000-8000-000000000001", "unknown"),
    ] {
        let (status, res) = post(
            &w,
            &batch(&w, "ILU-X", json!([{ "harvest_report_id": h, "kg": "1" }])),
            &f.ops,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{why}: {res}");
        assert_eq!(errors(&res), ["lines.0.harvest_report_id"], "{why}");
    }

    let current = actual(f, &w.baraka.cycles[0], "60").await;
    let mut wrong_crop = batch(
        &w,
        "ILU-X",
        json!([{ "harvest_report_id": current, "kg": "1" }]),
    );
    wrong_crop["crop_id"] = json!(w.c.banana);
    let (status, res) = post(&w, &wrong_crop, &f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["lines.0.harvest_report_id"]);
}

#[sqlx::test]
async fn input_is_checked(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "100").await;
    let mut b = batch(
        &w,
        "bad code!",
        json!([{ "harvest_report_id": h, "kg": "0" },
               { "harvest_report_id": h, "kg": "1.234" }]),
    );
    b["price_per_kg"] = json!(0);
    let (status, res) = post(&w, &b, &w.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        errors(&res),
        [
            "code",
            "lines.0.kg",
            "lines.1.harvest_report_id",
            "lines.1.kg",
            "price_per_kg"
        ]
    );
    let (status, res) = post(&w, &batch(&w, "ILU-E", json!([])), &w.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(errors(&res), ["lines"]);
}

#[sqlx::test]
async fn only_ops_or_admin_who_did_not_verify_the_harvest(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "100").await;
    let body = batch(
        &w,
        "ILU-M-07",
        json!([{ "harvest_report_id": h, "kg": "10" }]),
    );

    let (status, _) = post(&w, &body, &w.f.officer_ilu).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "field officer");
    let (status, res) = post(&w, &body, &w.f.owner).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the verifier: {res}");
    let (status, _) = post(&w, &body, &w.f.ops).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[sqlx::test]
async fn another_workspace_neither_buys_our_harvest_nor_reads_our_batch(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "100").await;
    let (status, ours) = post(
        &w,
        &batch(
            &w,
            "ILU-M-08",
            json!([{ "harvest_report_id": h, "kg": "10" }]),
        ),
        &w.f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = ours["data"]["id"].as_str().unwrap();

    let other = Farm::new_named(w.f.pool.clone(), "else.test", "elsewhere").await;
    let village = other.ilundo.clone();
    let crop = other.crop("MAIZE", "area").await;
    let their_product = product(&other, &other.owner, &other.ws, "Theirs").await;
    let their_tzs = other.tzs().await;
    let body = json!({
        "client_ref": client_ref(), "village_id": village, "crop_id": crop, "code": "X-1",
        "raw_product_id": their_product, "price_per_kg": 1, "currency_id": their_tzs,
        "lines": [{ "harvest_report_id": h, "kg": "1" }]
    });
    let (status, res) = other
        .call(Method::POST, "/supply-batches", Some(body), &other.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["lines.0.harvest_report_id"]);

    let (status, _) = other
        .call(
            Method::GET,
            &format!("/supply-batches/{id}"),
            None,
            &other.ops,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list) = other
        .call(Method::GET, "/supply-batches", None, &other.ops)
        .await;
    assert_eq!(list["data"], json!([]));

    // Our batch with their product and currency.
    let mut b = batch(
        &w,
        "ILU-M-09",
        json!([{ "harvest_report_id": h, "kg": "1" }]),
    );
    b["raw_product_id"] = json!(their_product);
    b["currency_id"] = json!(their_tzs);
    let (status, res) = post(&w, &b, &w.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["currency_id", "raw_product_id"]);

    let (outsider, _) = signup(&w.f.app, "out@else.test", "outside").await;
    let (status, _) =
        w.f.call(Method::GET, "/supply-batches", None, &outsider)
            .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn officers_read_their_villages_farmers_read_nothing(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "100").await;
    let (_, res) = post(
        &w,
        &batch(
            &w,
            "ILU-M-10",
            json!([{ "harvest_report_id": h, "kg": "10" }]),
        ),
        &w.f.ops,
    )
    .await;
    let id = res["data"]["id"].as_str().unwrap();
    let tail = format!("/supply-batches/{id}");

    let (_, list) =
        w.f.call(Method::GET, "/supply-batches", None, &w.f.officer_ilu)
            .await;
    assert_eq!(list["data"].as_array().unwrap().len(), 1);
    assert_eq!(list["meta"]["total"], 1);
    let (_, list) =
        w.f.call(Method::GET, "/supply-batches", None, &w.f.officer_mga)
            .await;
    assert_eq!(list["data"], json!([]));
    let (status, _) = w.f.call(Method::GET, &tail, None, &w.f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let farmer =
        w.f.farmer_for("neema@ruaha.test", &w.f.ilundo, &w.c.neema.person)
            .await;
    let (status, _) = w.f.call(Method::GET, &tail, None, &farmer).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn batches_and_shares_are_append_only(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "100").await;
    let (status, _) = post(
        &w,
        &batch(
            &w,
            "ILU-M-11",
            json!([{ "harvest_report_id": h, "kg": "10" }]),
        ),
        &w.f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    for sql in [
        "UPDATE farm_supply_shares SET kg = 1",
        "DELETE FROM farm_supply_shares",
        "UPDATE farm_supply_batches SET price_per_kg = 1",
        "DELETE FROM farm_supply_batches",
    ] {
        let err = sqlx::query(sql).execute(&w.f.pool).await;
        assert!(err.is_err(), "{sql} must be refused");
    }
}

#[sqlx::test]
async fn errors_name_the_line_as_sent(pool: PgPool) {
    let w = world(pool).await;
    let a = actual(&w.f, &w.c.neema.cycles[0], "10").await;
    let b = actual(&w.f, &w.baraka.cycles[0], "10").await;
    // The bigger id first, so sorting would swap the indexes.
    let (first, second) = if a > b { (a, b) } else { (b, a) };
    let (status, res) = post(
        &w,
        &batch(
            &w,
            "ILU-I",
            json!([{ "harvest_report_id": first, "kg": "11" },
                   { "harvest_report_id": second, "kg": "1" }]),
        ),
        &w.f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["lines.0.kg"]);
}

#[sqlx::test]
async fn a_newer_figure_does_not_free_kg_already_bought(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "100").await;
    let (status, _) = post(
        &w,
        &batch(
            &w,
            "ILU-S-1",
            json!([{ "harvest_report_id": h, "kg": "80" }]),
        ),
        &w.f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let newer = actual(&w.f, &w.c.neema.cycles[0], "100").await;

    let (status, res) = post(
        &w,
        &batch(
            &w,
            "ILU-S-2",
            json!([{ "harvest_report_id": newer, "kg": "21" }]),
        ),
        &w.f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["errors"]["lines.0.kg"][0],
        "over-commitment: 100.00 kg available, 80.00 kg already committed, 21.00 kg requested"
    );
}

#[sqlx::test]
async fn money_and_kg_totals_stay_in_range(pool: PgPool) {
    let w = world(pool).await;
    let h = actual(&w.f, &w.c.neema.cycles[0], "9999999999").await;
    let mut b = batch(
        &w,
        "ILU-R-1",
        json!([{ "harvest_report_id": h, "kg": "9999999999" }]),
    );
    b["price_per_kg"] = json!(1_000_000_000_000_i64);
    let (status, res) = post(&w, &b, &w.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["price_per_kg"]);

    sqlx::query("UPDATE currencies SET active = false WHERE id = $1::uuid")
        .bind(&w.tzs)
        .execute(&w.f.pool)
        .await
        .unwrap();
    let (status, res) = post(
        &w,
        &batch(
            &w,
            "ILU-R-2",
            json!([{ "harvest_report_id": h, "kg": "1" }]),
        ),
        &w.f.ops,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(errors(&res), ["currency_id"]);
}

/// Publish the lot's passport (owner holds product.update); its public reply.
async fn public_page(w: &World, lot: &str, public_code: &str) -> Value {
    let base = format!("/v1/workspace/{}/lots/{lot}/passport", w.f.ws);
    let (status, res) = send(
        &w.f.app,
        Method::PUT,
        &base,
        Some(json!({ "title": "Ilundo maize" })),
        Some(&w.f.owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = send(
        &w.f.app,
        Method::POST,
        &format!("{base}/publish"),
        None,
        Some(&w.f.owner),
    )
    .await;
    assert!(status.is_success(), "{res}");
    let (status, res) = send(
        &w.f.app,
        Method::GET,
        &format!("/v1/public/lots/{public_code}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res
}

#[sqlx::test]
async fn the_public_page_shows_village_count_and_harvest_window_never_names(pool: PgPool) {
    let w = world(pool).await;
    let (status, res) =
        w.f.register(
            &w.f.officer_ilu,
            json!({
                "client_ref": client_ref(), "village_id": w.f.ilundo,
                "person": { "given_name": "Upendo", "family_name": "Mwita", "phone": "0712000010" },
                "household": { "label": "Mwita household" },
                "farm": { "label": "Mwita farm", "latitude": "-7.74", "longitude": "35.74" },
                "plot": { "label": "Mwita plot", "area_ha": "1.0" },
                "cycles": [{ "crop_id": w.c.maize, "area_ha": "1.0",
                             "harvest_start": "2026-08-20", "harvest_end": "2026-10-05" }]
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let upendo_cycle = res["data"]["crop_cycle_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let n = actual(&w.f, &w.c.neema.cycles[0], "100").await;
    let b = actual(&w.f, &w.baraka.cycles[0], "100").await;
    let u = actual(&w.f, &upendo_cycle, "100").await;
    sqlx::query("UPDATE farm_persons SET passport_profile_consent = true, passport_profile_consent_at = now()")
        .execute(&w.f.pool)
        .await
        .unwrap();

    let (_, three) = post(
        &w,
        &batch(
            &w,
            "ILU-PUB-3",
            json!([{ "harvest_report_id": n, "kg": "10" }, { "harvest_report_id": b, "kg": "10" },
                   { "harvest_report_id": u, "kg": "10" }]),
        ),
        &w.f.ops,
    )
    .await;
    let lot = &three["data"]["lot"];
    let page = public_page(
        &w,
        lot["id"].as_str().unwrap(),
        lot["public_code"].as_str().unwrap(),
    )
    .await;
    // Neema's cycle runs 2026-09-01..30, Baraka's has no window, Upendo's 2026-08-20..10-05.
    assert_eq!(
        page["data"]["farmers"],
        json!([{ "village": "Ilundo", "farmers": 3,
                 "harvest_start": "2026-08-20", "harvest_end": "2026-10-05" }])
    );
    let text = page.to_string();
    for private in [
        "Neema",
        "Mwakalinga",
        "Upendo",
        "0712",
        "person",
        "kg",
        "price",
    ] {
        assert!(!text.contains(private), "{private} leaked: {text}");
    }

    let (_, two) = post(
        &w,
        &batch(
            &w,
            "ILU-PUB-2",
            json!([{ "harvest_report_id": n, "kg": "10" }, { "harvest_report_id": b, "kg": "10" }]),
        ),
        &w.f.ops,
    )
    .await;
    let lot = &two["data"]["lot"];
    let page = public_page(
        &w,
        lot["id"].as_str().unwrap(),
        lot["public_code"].as_str().unwrap(),
    )
    .await;
    assert_eq!(
        page["data"]["farmers"][0]["farmers"],
        json!(null),
        "fewer than 3 farmers: no count"
    );
}
