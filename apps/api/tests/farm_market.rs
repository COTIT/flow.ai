//! Farm market (RUAHA `buyer`, `buyer_demand`, `opportunity`): ops keeps the buyer order book
//! and proposes village opportunities against a demand. An opportunity is never a sale. RUAHA
//! ids in docs/ruaha-rls-mapping.md.

mod common;

use axum::http::{Method, StatusCode};
use common::{
    farm::{Cast, Farm},
    join_workspace, send, signup,
};
use serde_json::{Value, json};
use sqlx::PgPool;

pub struct Market {
    pub f: Farm,
    pub c: Cast,
    pub tzs: String,
    pub buyer: String,
    /// Open maize demand, 5000 kg, September–October.
    pub demand: String,
    /// Neema, a signed-in Ilundo farmer.
    pub neema: String,
}

async fn created(f: &Farm, tail: &str, body: Value) -> String {
    let (status, res) = f.call(Method::POST, tail, Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{tail}: {res}");
    res["data"]["id"].as_str().unwrap().to_owned()
}

pub async fn market(pool: PgPool) -> Market {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    let tzs = f.tzs().await;
    let buyer = created(
        &f,
        "/buyers",
        json!({ "name": "Iringa Millers", "channel": "direct" }),
    )
    .await;
    let demand = created(
        &f,
        "/buyer-demands",
        json!({ "buyer_id": buyer, "crop_id": c.maize, "quantity_kg": "5000",
            "window_start": "2026-09-01", "window_end": "2026-10-31",
            "indicative_price_per_kg_minor": 80_000, "currency_id": tzs }),
    )
    .await;
    let neema = f
        .farmer_for("neema@ruaha.test", &f.ilundo, &c.neema.person)
        .await;
    Market {
        f,
        c,
        tzs,
        buyer,
        demand,
        neema,
    }
}

impl Market {
    pub async fn propose(&self, village: &str) -> (StatusCode, Value) {
        let tail = format!("/buyer-demands/{}/opportunities", self.demand);
        self.f
            .call(
                Method::POST,
                &tail,
                Some(json!({ "village_id": village })),
                &self.f.ops,
            )
            .await
    }

    pub async fn proposed(&self, village: &str) -> String {
        let (status, res) = self.propose(village).await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["id"].as_str().unwrap().to_owned()
    }

    pub async fn set(&self, id: &str, status: &str, token: &str) -> (StatusCode, Value) {
        let tail = format!("/opportunities/{id}/status");
        self.f
            .call(
                Method::POST,
                &tail,
                Some(json!({ "status": status })),
                token,
            )
            .await
    }
}

// ---- buyers ----

#[sqlx::test]
async fn ops_keeps_the_buyer_book(pool: PgPool) {
    let m = market(pool).await;
    let (status, res) =
        m.f.call(
            Method::POST,
            "/buyers",
            Some(json!({ "name": "AFM hub", "channel": "afm", "contact_note": "via the coop" })),
            &m.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["channel"], "afm");
    assert_eq!(res["data"]["is_active"], true);
    let list = m.f.list("/buyers", &m.f.ops).await;
    assert_eq!(list["meta"]["total"], 2, "{list}");
    // Names are unique per workspace; the channel is a fixed label.
    let (status, res) =
        m.f.call(
            Method::POST,
            "/buyers",
            Some(json!({ "name": "AFM hub" })),
            &m.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["name"].is_array(), "{res}");
    let (status, res) =
        m.f.call(
            Method::POST,
            "/buyers",
            Some(json!({ "name": "X", "channel": "auction" })),
            &m.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["channel"].is_array(), "{res}");
}

#[sqlx::test]
async fn the_order_book_is_for_ops_and_admin_only(pool: PgPool) {
    let m = market(pool).await;
    for token in [&m.f.officer_ilu, &m.neema] {
        for tail in [
            "/buyers".to_owned(),
            "/buyer-demands".to_owned(),
            format!("/buyer-demands/{}", m.demand),
        ] {
            let (status, res) = m.f.call(Method::GET, &tail, None, token).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{tail}: {res}");
        }
        let (status, _) =
            m.f.call(
                Method::POST,
                "/buyers",
                Some(json!({ "name": "Sneaky" })),
                token,
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let list = m.f.list("/buyer-demands", &m.f.owner).await;
    assert_eq!(list["meta"]["total"], 1);
}

#[sqlx::test]
async fn a_retried_buyer_create_replays_and_a_changed_body_conflicts(pool: PgPool) {
    let m = market(pool).await;
    let id = "019a0000-0000-7000-8000-0000000000f1";
    let body = json!({ "id": id, "name": "Mbeya Foods" });
    let (status, _) =
        m.f.call(Method::POST, "/buyers", Some(body.clone()), &m.f.ops)
            .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, res) =
        m.f.call(Method::POST, "/buyers", Some(body), &m.f.ops)
            .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) =
        m.f.call(
            Method::POST,
            "/buyers",
            Some(json!({ "id": id, "name": "Other" })),
            &m.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
}

// ---- demand ----

#[sqlx::test]
async fn ops_records_a_demand_and_its_capture_is_the_servers(pool: PgPool) {
    let m = market(pool).await;
    let (status, res) =
        m.f.call(
            Method::POST,
            "/buyer-demands",
            Some(
                json!({ "buyer_id": m.buyer, "crop_id": m.c.maize, "quantity_kg": "1200.5",
                "window_start": "2026-11-01", "window_end": "2026-11-30",
                "quality_note": "grade 1", "delivery_point": "Iringa",
                "verification": "verified", "captured_by": "019a0000-0000-7000-8000-000000000001",
                "status": "matched" }),
            ),
            &m.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let d = &res["data"];
    assert_eq!(d["quantity_kg"], "1200.50");
    assert_eq!(d["buyer"]["name"], "Iringa Millers");
    assert_eq!(d["crop"]["id"], m.c.maize);
    assert_eq!(d["status"], "open");
    assert_eq!(d["verification"], "unverified");
    assert_eq!(d["captured_by"]["id"], m.f.user_id("ops@ruaha.test").await);
    assert_eq!(d["indicative_price_per_kg"], Value::Null);
    // The listed demand carries its price in minor units.
    let (_, res) =
        m.f.call(
            Method::GET,
            &format!("/buyer-demands/{}", m.demand),
            None,
            &m.f.ops,
        )
        .await;
    assert_eq!(res["data"]["indicative_price_per_kg"]["minor"], 80_000);
    assert_eq!(res["data"]["indicative_price_per_kg"]["currency"], "TZS");
    let open = m.f.list("/buyer-demands?status=open", &m.f.ops).await;
    assert_eq!(open["meta"]["total"], 2);
    let closed = m.f.list("/buyer-demands?status=closed", &m.f.ops).await;
    assert_eq!(closed["meta"]["total"], 0);
}

#[sqlx::test]
async fn demand_figures_are_checked(pool: PgPool) {
    let m = market(pool).await;
    let base = json!({ "buyer_id": m.buyer, "crop_id": m.c.maize, "quantity_kg": "100",
        "window_start": "2026-11-01", "window_end": "2026-11-30" });
    for (patch, field) in [
        (json!({ "quantity_kg": "0" }), "quantity_kg"),
        (json!({ "window_end": "2026-10-01" }), "window_end"),
        (json!({ "window_start": "soon" }), "window_start"),
        (
            json!({ "buyer_id": "019a0000-0000-7000-8000-0000000000ff" }),
            "buyer_id",
        ),
        (
            json!({ "crop_id": "019a0000-0000-7000-8000-0000000000ff" }),
            "crop_id",
        ),
        (
            json!({ "indicative_price_per_kg_minor": 100 }),
            "currency_id",
        ),
    ] {
        let mut body = base.clone();
        for (k, v) in patch.as_object().unwrap() {
            body[k] = v.clone();
        }
        let (status, res) =
            m.f.call(Method::POST, "/buyer-demands", Some(body), &m.f.ops)
                .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{patch}: {res}");
        assert!(res["errors"][field].is_array(), "{patch}: {res}");
    }
}

// ---- opportunities ----

#[sqlx::test]
async fn ops_proposes_a_village_against_a_demand(pool: PgPool) {
    let m = market(pool).await;
    let tail = format!("/buyer-demands/{}/opportunities", m.demand);
    let (status, res) =
        m.f.call(
            Method::POST,
            &tail,
            Some(json!({ "village_id": m.f.ilundo, "note": "good season",
                "offered_quantity_kg": "9999" })),
            &m.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let o = &res["data"];
    assert_eq!(o["status"], "proposed");
    assert_eq!(o["offered_quantity_kg"], "0.00");
    assert_eq!(o["village"]["id"], m.f.ilundo);
    assert_eq!(o["crop"]["id"], m.c.maize);
    assert_eq!(o["demand"]["id"], m.demand);
    assert_eq!(o["demand"]["buyer"]["name"], "Iringa Millers");
    assert_eq!(o["supply"], json!([]));
    // One opportunity per demand and village.
    let (status, res) = m.propose(&m.f.ilundo).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["village_id"].is_array(), "{res}");
}

#[sqlx::test]
async fn an_opportunity_is_born_proposed_for_the_demand_crop(pool: PgPool) {
    let m = market(pool).await;
    let tail = format!("/buyer-demands/{}/opportunities", m.demand);
    let (status, res) =
        m.f.call(
            Method::POST,
            &tail,
            Some(json!({ "village_id": m.f.ilundo, "status": "shared" })),
            &m.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], "an opportunity must start as proposed");
    let (status, res) =
        m.f.call(
            Method::POST,
            &tail,
            Some(json!({ "village_id": m.f.ilundo, "crop_id": m.c.banana })),
            &m.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["message"],
        "opportunity must match the demand crop and project"
    );
}

#[sqlx::test]
async fn only_ops_and_admin_write_opportunities(pool: PgPool) {
    let m = market(pool).await;
    let tail = format!("/buyer-demands/{}/opportunities", m.demand);
    let body = Some(json!({ "village_id": m.f.ilundo }));
    let (status, res) =
        m.f.call(Method::POST, &tail, body.clone(), &m.f.officer_ilu)
            .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    assert_eq!(
        res["message"],
        "only project ops or admin may write opportunities"
    );
    let (status, _) = m.f.call(Method::POST, &tail, body, &m.neema).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let id = m.proposed(&m.f.ilundo).await;
    let (status, res) = m.set(&id, "shared", &m.f.officer_ilu).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    assert_eq!(
        res["message"],
        "only project ops or admin may write opportunities"
    );
    // A farmer with nothing in it cannot even see it.
    let (status, _) = m.set(&id, "shared", &m.neema).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, res) =
        m.f.call(Method::GET, &format!("/opportunities/{id}"), None, &m.f.ops)
            .await;
    assert_eq!(res["data"]["status"], "proposed");
}

#[sqlx::test]
async fn opportunities_follow_ruahas_status_machine(pool: PgPool) {
    let m = market(pool).await;
    let id = m.proposed(&m.f.ilundo).await;
    let (status, res) = m.set(&id, "accepted", &m.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["message"],
        "illegal opportunity transition proposed -> accepted"
    );
    for to in ["shared", "accepted", "declined"] {
        let (status, res) = m.set(&id, to, &m.f.ops).await;
        assert_eq!(status, StatusCode::OK, "{to}: {res}");
        assert_eq!(res["data"]["status"], to);
    }
    let (status, res) = m.set(&id, "shared", &m.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["message"],
        "illegal opportunity transition declined -> shared"
    );
    let lapsed = m.proposed(&m.f.mgama).await;
    let (status, _) = m.set(&lapsed, "lapsed", &m.f.ops).await;
    assert_eq!(status, StatusCode::OK);
    for to in ["proposed", "shared", "accepted", "declined"] {
        let (status, res) = m.set(&lapsed, to, &m.f.ops).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{to}: {res}");
    }
    let (status, res) = m.set(&lapsed, "sold", &m.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["status"].is_array(), "{res}");
}

#[sqlx::test]
async fn staff_read_the_opportunities_of_their_villages(pool: PgPool) {
    let m = market(pool).await;
    let ilu = m.proposed(&m.f.ilundo).await;
    let mga = m.proposed(&m.f.mgama).await;
    let mine = m.f.list("/opportunities", &m.f.officer_ilu).await;
    assert_eq!(mine["meta"]["total"], 1, "{mine}");
    assert_eq!(mine["data"][0]["id"], ilu);
    let (status, _) =
        m.f.call(
            Method::GET,
            &format!("/opportunities/{mga}"),
            None,
            &m.f.officer_ilu,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let all = m.f.list("/opportunities", &m.f.ops).await;
    assert_eq!(all["meta"]["total"], 2);
    let by_village =
        m.f.list(
            &format!("/opportunities?village_id={}", m.f.mgama),
            &m.f.ops,
        )
        .await;
    assert_eq!(by_village["meta"]["total"], 1);
    // A farmer with no supply in any opportunity sees none.
    let none = m.f.list("/opportunities", &m.neema).await;
    assert_eq!(none["meta"]["total"], 0);
}

#[sqlx::test]
async fn the_market_of_another_workspace_is_404(pool: PgPool) {
    let m = market(pool).await;
    let id = m.proposed(&m.f.ilundo).await;
    let (other, other_ws) = signup(&m.f.app, "other@else.test", "elsewhere").await;
    sqlx::query(
        "INSERT INTO farm_memberships (workspace_id, user_id, role)
         SELECT $1::uuid, id, 'admin' FROM users WHERE email = 'other@else.test'",
    )
    .bind(&other_ws)
    .execute(&m.f.pool)
    .await
    .unwrap();
    let base = format!("/v1/workspace/{other_ws}/farm");
    for tail in [
        format!("/opportunities/{id}"),
        format!("/buyer-demands/{}", m.demand),
    ] {
        let (status, _) = send(
            &m.f.app,
            Method::GET,
            &format!("{base}{tail}"),
            None,
            Some(&other),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{tail}");
    }
    let (status, _) = send(
        &m.f.app,
        Method::POST,
        &format!("{base}/opportunities/{id}/status"),
        Some(json!({ "status": "shared" })),
        Some(&other),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        &m.f.app,
        Method::POST,
        &format!("{base}/buyer-demands/{}/opportunities", m.demand),
        Some(json!({ "village_id": m.f.ilundo })),
        Some(&other),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, res) = send(
        &m.f.app,
        Method::POST,
        &format!("{base}/buyer-demands"),
        Some(
            json!({ "buyer_id": m.buyer, "crop_id": m.c.maize, "quantity_kg": "1",
            "window_start": "2026-11-01", "window_end": "2026-11-30" }),
        ),
        Some(&other),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    let (status, _) = send(&m.f.app, Method::GET, &m.f.url("/buyers"), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---- supply lines (3b) ----

impl Market {
    /// Neema's current expected maize figure (3200 kg, September).
    async fn neema_expected(&self) -> String {
        self.c.neema.harvests[0].clone()
    }

    async fn attach(&self, opp: &str, harvest: &str, kg: &str, token: &str) -> (StatusCode, Value) {
        let tail = format!("/opportunities/{opp}/supply");
        self.f
            .call(
                Method::POST,
                &tail,
                Some(json!({ "harvest_report_id": harvest, "contributed_kg": kg })),
                token,
            )
            .await
    }

    /// A second open maize demand; its id.
    async fn second_demand(&self) -> String {
        created(
            &self.f,
            "/buyer-demands",
            json!({ "buyer_id": self.buyer, "crop_id": self.c.maize, "quantity_kg": "4000",
                "window_start": "2026-09-01", "window_end": "2026-09-30" }),
        )
        .await
    }

    async fn propose_for(&self, demand: &str, village: &str) -> String {
        let tail = format!("/buyer-demands/{demand}/opportunities");
        created(&self.f, &tail, json!({ "village_id": village })).await
    }

    async fn available(&self, token: &str) -> Value {
        let tail = format!(
            "/harvest-available?village_id={}&crop_id={}",
            self.f.ilundo, self.c.maize
        );
        self.f.list(&tail, token).await
    }
}

fn over(available: &str, committed: &str, requested: &str) -> String {
    format!(
        "over-commitment: {available} kg available, {committed} kg already committed, {requested} kg requested"
    )
}

#[sqlx::test]
async fn ops_attaches_supply_and_the_server_sums_it(pool: PgPool) {
    let m = market(pool).await;
    let opp = m.proposed(&m.f.ilundo).await;
    let harvest = m.neema_expected().await;
    let tail = format!("/opportunities/{opp}/supply");
    let (status, res) =
        m.f.call(
            Method::POST,
            &tail,
            Some(
                json!({ "harvest_report_id": harvest, "contributed_kg": "2000",
                "captured_by": "019a0000-0000-7000-8000-000000000001" }),
            ),
            &m.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let o = &res["data"];
    assert_eq!(o["offered_quantity_kg"], "2000.00");
    let line = &o["supply"][0];
    assert_eq!(line["harvest_report_id"], harvest);
    assert_eq!(line["contributed_kg"], "2000.00");
    assert_eq!(line["plot_label"], "Riverside");
    assert_eq!(line["farmer"]["person_id"], m.c.neema.person);
    assert_eq!(line["farmer"]["name"], "Neema Mwakalinga");
    assert_eq!(
        line["captured_by"]["id"],
        m.f.user_id("ops@ruaha.test").await
    );
    let rows = m.available(&m.f.ops).await;
    let row = &rows["data"][0];
    assert_eq!(row["harvest_report_id"], harvest, "{rows}");
    assert_eq!(row["quantity_kg"], "3200.00");
    assert_eq!(row["committed_kg"], "2000.00");
    assert_eq!(row["available_kg"], "1200.00");
    assert_eq!(row["farmer"]["person_id"], m.c.neema.person);
    assert_eq!(row["plot_label"], "Riverside");
}

#[sqlx::test]
async fn a_committed_harvest_cannot_be_promised_to_a_second_live_opportunity(pool: PgPool) {
    let m = market(pool).await;
    let first = m.proposed(&m.f.ilundo).await;
    let second = m.propose_for(&m.second_demand().await, &m.f.ilundo).await;
    let harvest = m.neema_expected().await;
    assert_eq!(
        m.attach(&first, &harvest, "2000", &m.f.ops).await.0,
        StatusCode::CREATED
    );
    let (status, res) = m.attach(&second, &harvest, "1500", &m.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], over("3200.00", "2000.00", "1500.00"));
    // A declined opportunity no longer holds its kg.
    assert_eq!(m.set(&first, "declined", &m.f.ops).await.0, StatusCode::OK);
    let (status, res) = m.attach(&second, &harvest, "1500", &m.f.ops).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    // A line on a dead opportunity is refused.
    let (status, res) = m.attach(&first, &harvest, "1", &m.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], "supply requires a live opportunity");
}

#[sqlx::test]
async fn parallel_attaches_on_one_harvest_never_over_commit(pool: PgPool) {
    let m = market(pool).await;
    let first = m.proposed(&m.f.ilundo).await;
    let second = m.propose_for(&m.second_demand().await, &m.f.ilundo).await;
    let harvest = m.neema_expected().await;
    let (a, b) = tokio::join!(
        m.attach(&first, &harvest, "2000", &m.f.ops),
        m.attach(&second, &harvest, "2000", &m.f.ops)
    );
    let mut codes = vec![a.0, b.0];
    codes.sort();
    assert_eq!(
        codes,
        vec![StatusCode::CREATED, StatusCode::UNPROCESSABLE_ENTITY],
        "{} / {}",
        a.1,
        b.1
    );
    let refused = if a.0 == StatusCode::CREATED {
        &b.1
    } else {
        &a.1
    };
    assert_eq!(refused["message"], over("3200.00", "2000.00", "2000.00"));
    let rows = m.available(&m.f.ops).await;
    assert_eq!(rows["data"][0]["committed_kg"], "2000.00");
}

#[sqlx::test]
async fn supply_must_be_a_current_expected_harvest_of_the_village_and_crop(pool: PgPool) {
    let m = market(pool).await;
    let opp = m.proposed(&m.f.ilundo).await;
    let (status, res) = m
        .attach(&opp, &m.c.joseph.harvests[0], "100", &m.f.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(
        res["message"],
        "supply must match the opportunity village, crop and harvest cycle"
    );
    let actual = m.f.actual(&m.c.neema.cycles[0], "3000").await;
    for harvest in [actual.as_str(), "019a0000-0000-7000-8000-0000000000ff"] {
        let (status, res) = m.attach(&opp, harvest, "100", &m.f.ops).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert_eq!(
            res["message"],
            "supply must reference a current, undeleted expected harvest report"
        );
    }
    let (status, res) = m
        .attach(&opp, &m.neema_expected().await, "0", &m.f.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["contributed_kg"].is_array(), "{res}");
}

#[sqlx::test]
async fn only_ops_and_admin_attach_supply(pool: PgPool) {
    let m = market(pool).await;
    let opp = m.proposed(&m.f.ilundo).await;
    let harvest = m.neema_expected().await;
    let (status, res) = m.attach(&opp, &harvest, "100", &m.f.officer_ilu).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{res}");
    assert_eq!(
        res["message"],
        "only project ops or admin may attach supply"
    );
    for token in [&m.f.officer_mga, &m.neema] {
        let (status, _) = m.attach(&opp, &harvest, "100", token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    let rows = m.available(&m.f.ops).await;
    assert_eq!(rows["data"][0]["committed_kg"], "0.00");
}

#[sqlx::test]
async fn a_farmer_sees_the_opportunity_their_supply_is_in_without_the_attacher(pool: PgPool) {
    let m = market(pool).await;
    let opp = m.proposed(&m.f.ilundo).await;
    assert_eq!(
        m.f.list("/opportunities", &m.neema).await["meta"]["total"],
        0
    );
    m.attach(&opp, &m.neema_expected().await, "2000", &m.f.ops)
        .await;
    let mine = m.f.list("/opportunities", &m.neema).await;
    assert_eq!(mine["meta"]["total"], 1, "{mine}");
    let o = &mine["data"][0];
    assert_eq!(o["id"], opp);
    assert_eq!(o["my_contribution_kg"], "2000.00");
    assert_eq!(o["supply"][0]["contributed_kg"], "2000.00");
    assert_eq!(o["supply"][0]["captured_by"], Value::Null, "{mine}");
    // Village staff resolve who attached it; staff elsewhere do not see the line at all.
    let (_, res) =
        m.f.call(
            Method::GET,
            &format!("/opportunities/{opp}"),
            None,
            &m.f.officer_ilu,
        )
        .await;
    assert!(
        res["data"]["supply"][0]["captured_by"]["name"].is_string(),
        "{res}"
    );
    let (status, _) =
        m.f.call(
            Method::GET,
            &format!("/opportunities/{opp}"),
            None,
            &m.f.officer_mga,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // The farmer still cannot move it.
    let (status, _) = m.set(&opp, "shared", &m.neema).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn mixed_membership_reads_supply_as_staff_only_in_staff_villages(pool: PgPool) {
    let m = market(pool).await;
    // Neema farms in Ilundo and is also the Mgama field officer.
    sqlx::query(
        "INSERT INTO farm_memberships (workspace_id, user_id, role, village_id)
         SELECT $1::uuid, id, 'field_officer', $2::uuid FROM users WHERE email = 'neema@ruaha.test'",
    )
    .bind(&m.f.ws)
    .bind(&m.f.mgama)
    .execute(&m.f.pool)
    .await
    .unwrap();
    // Staff roles count only for workspace members.
    join_workspace(&m.f.pool, &m.f.ws, "neema@ruaha.test").await;
    let ilu = m.proposed(&m.f.ilundo).await;
    let mga = m.proposed(&m.f.mgama).await;
    m.attach(&ilu, &m.neema_expected().await, "100", &m.f.ops)
        .await;
    let (status, res) = m
        .attach(&mga, &m.c.joseph.harvests[0], "100", &m.f.ops)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let (_, res) =
        m.f.call(
            Method::GET,
            &format!("/opportunities/{ilu}"),
            None,
            &m.neema,
        )
        .await;
    assert_eq!(
        res["data"]["supply"][0]["captured_by"],
        Value::Null,
        "{res}"
    );
    let (_, res) =
        m.f.call(
            Method::GET,
            &format!("/opportunities/{mga}"),
            None,
            &m.neema,
        )
        .await;
    assert!(
        res["data"]["supply"][0]["captured_by"]["name"].is_string(),
        "{res}"
    );
}

#[sqlx::test]
async fn a_supply_line_is_never_rewritten(pool: PgPool) {
    let m = market(pool).await;
    let opp = m.proposed(&m.f.ilundo).await;
    m.attach(&opp, &m.neema_expected().await, "100", &m.f.ops)
        .await;
    let err =
        sqlx::query("UPDATE farm_opportunity_supply SET captured_by = NULL, contributed_kg = 1")
            .execute(&m.f.pool)
            .await;
    assert!(err.is_err(), "a supply line must be append-only");
}

#[sqlx::test]
async fn a_superseded_figure_keeps_the_kg_committed_on_it(pool: PgPool) {
    let m = market(pool).await;
    let first = m.proposed(&m.f.ilundo).await;
    assert_eq!(
        m.attach(&first, &m.neema_expected().await, "2000", &m.f.ops)
            .await
            .0,
        StatusCode::CREATED
    );
    let (status, res) =
        m.f.call(
            Method::POST,
            &format!("/crop-cycles/{}/harvest-reports", m.c.neema.cycles[0]),
            Some(json!({ "kind": "expected", "quantity_kg": "2500", "source": "field_verified" })),
            &m.f.officer_ilu,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let v2 = res["data"]["id"].as_str().unwrap().to_owned();
    let rows = m.available(&m.f.ops).await;
    let row = &rows["data"][0];
    assert_eq!(row["harvest_report_id"], v2, "{rows}");
    assert_eq!(row["committed_kg"], "2000.00");
    assert_eq!(row["available_kg"], "500.00");
    let second = m.propose_for(&m.second_demand().await, &m.f.ilundo).await;
    let (status, res) = m.attach(&second, &v2, "600", &m.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], over("2500.00", "2000.00", "600.00"));
    // The old figure is no longer current: nothing more goes on it.
    let (status, res) = m
        .attach(&second, &m.neema_expected().await, "1", &m.f.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

#[sqlx::test]
async fn opportunity_kg_and_batch_kg_do_not_block_each_other(pool: PgPool) {
    let m = market(pool).await;
    let opp = m.proposed(&m.f.ilundo).await;
    let expected = m.neema_expected().await;
    // Promise the whole expected figure to the buyer…
    assert_eq!(
        m.attach(&opp, &expected, "3200", &m.f.ops).await.0,
        StatusCode::CREATED
    );
    // …and buy the whole actual figure into a supply batch.
    let actual = m.f.actual(&m.c.neema.cycles[0], "3000").await;
    let product = m.f.product("Raw maize").await;
    m.f.batch(
        "MZ-1",
        &m.c.maize,
        &product,
        &m.tzs,
        80_000,
        &[(&actual, "3000")],
    )
    .await;
    let committed = |id: String| {
        let pool = m.f.pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT farm_harvest_committed_kg($1::uuid)::text")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(committed(expected.clone()).await, "3200.00");
    assert_eq!(committed(actual.clone()).await, "3000.00");
    // A further promise on the expected figure is still refused by its own series.
    let second = m.propose_for(&m.second_demand().await, &m.f.ilundo).await;
    let (status, res) = m.attach(&second, &expected, "1", &m.f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert_eq!(res["message"], over("3200.00", "3200.00", "1.00"));
}

// ---- village supply and matches (3b) ----

#[sqlx::test]
async fn village_supply_and_matches_count_each_harvest_once(pool: PgPool) {
    let m = market(pool).await;
    let opp = m.proposed(&m.f.ilundo).await;
    m.attach(&opp, &m.neema_expected().await, "2000", &m.f.ops)
        .await;
    // A demand spanning five months still counts Neema's September figure once.
    let wide = created(
        &m.f,
        "/buyer-demands",
        json!({ "buyer_id": m.buyer, "crop_id": m.c.maize, "quantity_kg": "5000",
            "window_start": "2026-08-01", "window_end": "2026-12-31" }),
    )
    .await;
    let (status, res) =
        m.f.call(
            Method::GET,
            &format!("/buyer-demands/{wide}/matches"),
            None,
            &m.f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let rows = res["data"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "one row per village: {res}");
    let r = &rows[0];
    assert_eq!(r["village"]["id"], m.f.ilundo);
    assert_eq!(r["available_kg"], "1200.00");
    assert_eq!(r["committed_kg"], "2000.00");
    assert_eq!(r["coverable_kg"], "1200.00");
    assert_eq!(r["coverage_pct"], "24.0");
    assert_eq!(r["opportunity"], Value::Null);
    // By village, with the buyer named and this demand's opportunity linked.
    let tail = format!("/villages/{}/demand-matches", m.f.ilundo);
    let (status, res) = m.f.call(Method::GET, &tail, None, &m.f.ops).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let first = res["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["buyer_demand_id"] == m.demand)
        .unwrap();
    assert_eq!(first["buyer_name"], "Iringa Millers");
    assert_eq!(first["opportunity"]["id"], opp);
    assert_eq!(first["opportunity"]["status"], "proposed");
    let (status, _) = m.f.call(Method::GET, &tail, None, &m.f.officer_ilu).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Village supply by crop and month.
    let tail = format!("/villages/{}/supply", m.f.ilundo);
    let (status, res) = m.f.call(Method::GET, &tail, None, &m.f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let s = &res["data"][0];
    assert_eq!(s["crop_id"], m.c.maize);
    assert_eq!(s["window_month"], "2026-09-01");
    assert_eq!(s["expected_kg"], "3200.00");
    assert_eq!(s["committed_kg"], "2000.00");
    assert_eq!(s["available_kg"], "1200.00");
    assert_eq!(s["cycle_count"], 1);
    let (status, _) = m.f.call(Method::GET, &tail, None, &m.neema).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = m.f.call(Method::GET, &tail, None, &m.f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn a_farmer_reads_harvest_availability_of_their_own_cycles_only(pool: PgPool) {
    let m = market(pool).await;
    let all =
        m.f.list(
            &format!("/harvest-available?village_id={}", m.f.ilundo),
            &m.f.officer_ilu,
        )
        .await;
    assert_eq!(all["meta"]["total"], 1, "{all}");
    let own = m.f.list("/harvest-available", &m.neema).await;
    assert_eq!(own["meta"]["total"], 1, "{own}");
    let none = m.f.list("/harvest-available", &m.f.officer_mga).await;
    assert_eq!(none["meta"]["total"], 1, "Joseph's Mgama figure: {none}");
    assert_eq!(none["data"][0]["village_id"], m.f.mgama);
}
