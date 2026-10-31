//! Survey vouchers: the code is the household's; staff look it up (every scan logged) and
//! redeem face to face. The registrar and the verifier of the household never redeem; a
//! random share is held for ops; the trail is append-only. RUAHA ids in
//! docs/ruaha-rls-mapping.md.

mod common;

use axum::http::{Method, StatusCode};
use common::{
    farm::{Farm, Surveyed, client_ref},
    send,
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Neema answered a live survey: (survey, voucher id, code).
async fn setup(pool: PgPool, audit_percent: i64) -> (Farm, Surveyed, String, String, String) {
    let f = Farm::new(pool).await;
    let s = f.surveyed().await;
    let (id, q) = f.survey(&s.tzs, 200_000, audit_percent, false).await;
    let (status, res) = f
        .answer(
            &id,
            json!({ q[0].clone(): "home" }),
            &client_ref(),
            &s.neema,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let voucher = res["data"]["voucher_id"].as_str().unwrap().to_owned();
    let code = res["data"]["code"].as_str().unwrap().to_owned();
    (f, s, id, voucher, code)
}

async fn lookup(f: &Farm, code: &str, token: &str) -> (StatusCode, Value) {
    f.call(
        Method::POST,
        "/vouchers/lookup",
        Some(json!({ "code": code })),
        token,
    )
    .await
}

async fn redeem(f: &Farm, code: &str, token: &str) -> (StatusCode, Value) {
    let body = json!({ "code": code, "id_type": "nida", "name_confirmed": true });
    f.call(Method::POST, "/vouchers/redeem", Some(body), token)
        .await
}

async fn timeline(f: &Farm, voucher: &str, token: &str) -> Vec<String> {
    let (status, res) = f
        .call(
            Method::GET,
            &format!("/vouchers/{voucher}/timeline"),
            None,
            token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap().to_owned())
        .collect()
}

// ---- the code ----

#[sqlx::test]
async fn only_the_household_reads_its_code(pool: PgPool) {
    let (f, s, survey, voucher, code) = setup(pool, 0).await;
    let mine = f.list("/vouchers/mine", &s.neema).await;
    assert_eq!(
        mine["data"][0]["code"],
        code.as_str(),
        "the household reads its own code"
    );
    assert!(
        mine["data"][0].get("audit_required").is_none(),
        "the audit flag is not shown"
    );
    assert_eq!(
        f.list("/vouchers/mine", &s.asha).await["meta"]["total"],
        0,
        "another household sees no voucher"
    );
    let (status, _) = f
        .call(
            Method::GET,
            &format!("/vouchers/{voucher}/timeline"),
            None,
            &s.asha,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another household cannot read it"
    );
    let (status, list) = f
        .call(
            Method::GET,
            &format!("/surveys/{survey}/vouchers"),
            None,
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert!(
        !list.to_string().contains(&code),
        "staff cannot read the code"
    );
    assert!(list["data"][0].get("code").is_none());
}

// ---- lookup ----

#[sqlx::test]
async fn lookups_are_staff_only_logged_and_blind_outside_the_village(pool: PgPool) {
    let (f, s, _, voucher, code) = setup(pool.clone(), 0).await;
    let (status, _) = lookup(&f, &code, &s.neema).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a farmer cannot look up vouchers"
    );
    let (status, res) = lookup(&f, &code, &f.officer_mga).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        res["data"]["found"], false,
        "an out-of-village scan finds nothing"
    );
    let (status, res) = lookup(&f, &code.to_lowercase(), &s.juma).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["data"]["found"], true);
    assert_eq!(res["data"]["can_redeem"], true);
    assert_eq!(res["data"]["amount"]["minor"], 200_000);
    let (status, res) = lookup(&f, &code, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        res["data"]["blocked_reason"],
        "you registered this household, so another staff member must redeem this voucher",
        "the registering officer is told they may not redeem"
    );
    let kinds: Vec<(String,)> = sqlx::query_as(
        "SELECT kind FROM farm_voucher_events WHERE voucher_id = $1::uuid ORDER BY occurred_at",
    )
    .bind(&voucher)
    .fetch_all(&pool)
    .await
    .unwrap();
    let kinds: Vec<&str> = kinds.iter().map(|k| k.0.as_str()).collect();
    assert_eq!(
        kinds,
        ["issued", "refused", "scanned", "scanned"],
        "every scan is on the record"
    );
    let (status, _) = send(
        &f.app,
        Method::POST,
        &f.url("/vouchers/lookup"),
        Some(json!({ "code": code })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn lookups_are_rate_limited_per_user(pool: PgPool) {
    let (f, s, _, _, _) = setup(pool, 0).await;
    let mut limited = false;
    for _ in 0..40 {
        let (status, _) = lookup(&f, "0000000000", &s.juma).await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            limited = true;
            break;
        }
        assert_eq!(status, StatusCode::OK);
    }
    assert!(limited, "30 lookups a minute per user, then 429");
    let (status, _) = lookup(&f, "0000000000", &f.ops).await;
    assert_eq!(status, StatusCode::OK, "another user is unaffected");
}

// ---- redeem ----

#[sqlx::test]
async fn three_different_staff_and_an_id_check(pool: PgPool) {
    let (f, s, _, _, code) = setup(pool, 0).await;
    for (token, message) in [
        (
            &f.officer_ilu,
            "you registered this household, so another staff member must redeem this voucher",
        ),
        (
            &f.ops,
            "you verified this household, so another staff member must redeem this voucher",
        ),
    ] {
        let (status, res) = redeem(&f, &code, token).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(res["message"], message);
    }
    let (status, res) = f
        .call(
            Method::POST,
            "/vouchers/redeem",
            Some(json!({ "code": code, "name_confirmed": true })),
            &s.juma,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["message"], "record which ID document you checked");
    let (status, res) = f
        .call(
            Method::POST,
            "/vouchers/redeem",
            Some(json!({ "code": code, "id_type": "voter", "name_confirmed": false })),
            &s.juma,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "confirm that the name on the ID matches the household"
    );
    let (status, _) = redeem(&f, &code, &s.neema).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "farmers never redeem");

    let (status, res) = redeem(&f, &code.to_lowercase(), &s.juma).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a third staff member redeems, typed in lower case: {res}"
    );
    assert_eq!(res["data"]["replayed"], false);
    assert_eq!(res["data"]["redeemed_by"]["name"], "Ada Lovelace");
}

#[sqlx::test]
async fn a_household_member_never_redeems_their_own_voucher(pool: PgPool) {
    let (f, s, _, _, code) = setup(pool, 0).await;
    // Juma is also Rehema, a member of Neema's household.
    f.link("juma@ruaha.test", &s.c.rehema.person).await;
    let (status, res) = redeem(&f, &code, &s.juma).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "you cannot redeem a voucher for your own household"
    );
}

#[sqlx::test]
async fn redeem_is_safe_to_repeat_and_to_race(pool: PgPool) {
    let (f, s, _, voucher, code) = setup(pool.clone(), 0).await;
    // Five redeems at once; the API serialises them on the voucher row.
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..5 {
        let (app, uri, token) = (f.app.clone(), f.url("/vouchers/redeem"), s.juma.clone());
        let body = json!({ "code": code, "id_type": "nida", "name_confirmed": true });
        set.spawn(async move { send(&app, Method::POST, &uri, Some(body), Some(&token)).await });
    }
    let results = set.join_all().await;
    let fresh = results
        .iter()
        .filter(|(st, r)| *st == StatusCode::OK && r["data"]["replayed"] == false)
        .count();
    assert_eq!(fresh, 1, "{results:?}");
    assert!(
        results.iter().all(|(st, _)| *st == StatusCode::OK),
        "the same officer retrying is a replay: {results:?}"
    );
    let (status, again) = redeem(&f, &code, &s.juma).await;
    assert_eq!(
        (status, again["data"]["replayed"].clone()),
        (StatusCode::OK, json!(true))
    );
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM farm_voucher_events WHERE voucher_id = $1::uuid AND kind = 'redeemed'")
        .bind(&voucher)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "one redemption, one event");

    let redeemer = f
        .staff("halima@ruaha.test", "field_officer", Some(&f.ilundo))
        .await;
    let (status, res) = redeem(&f, &code, &redeemer).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        res["message"]
            .as_str()
            .unwrap()
            .starts_with("voucher already redeemed on "),
        "{res}"
    );
}

#[sqlx::test]
async fn a_held_voucher_needs_ops_in_person(pool: PgPool) {
    let (f, s, _, _, code) = setup(pool, 100).await;
    let (_, res) = lookup(&f, &code, &s.juma).await;
    assert_eq!(
        res["data"]["needs_ops"], true,
        "a held voucher tells the officer it needs ops"
    );
    let (status, res) = redeem(&f, &code, &s.juma).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "this voucher is held for an audit: ops or admin must redeem it in person"
    );
    let (status, res) = redeem(&f, &code, &f.owner).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "admin redeems a held voucher in person: {res}"
    );
}

#[sqlx::test]
async fn an_expired_voucher_is_refused(pool: PgPool) {
    let (f, s, _, voucher, code) = setup(pool.clone(), 0).await;
    sqlx::query(
        "UPDATE farm_vouchers SET expires_at = now() - interval '1 day' WHERE id = $1::uuid",
    )
    .bind(&voucher)
    .execute(&pool)
    .await
    .unwrap();
    let (status, res) = redeem(&f, &code, &s.juma).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        res["message"]
            .as_str()
            .unwrap()
            .starts_with("this voucher expired on "),
        "{res}"
    );
}

// ---- void ----

#[sqlx::test]
async fn ops_voids_with_a_reason_and_the_void_is_on_the_trail(pool: PgPool) {
    let (f, s, _, voucher, code) = setup(pool, 0).await;
    let tail = format!("/vouchers/{voucher}/void");
    let (status, _) = f
        .call(
            Method::POST,
            &tail,
            Some(json!({ "reason": "duplicate household" })),
            &s.juma,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a field officer cannot void");
    let (status, res) = f
        .call(Method::POST, &tail, Some(json!({ "reason": " " })), &f.ops)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["message"], "give a reason for voiding this voucher");
    let (status, res) = f
        .call(
            Method::POST,
            &tail,
            Some(json!({ "reason": "duplicate household" })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{res}");
    let (status, res) = redeem(&f, &code, &s.juma).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "this voucher was voided: duplicate household"
    );
    let (status, _) = f
        .call(
            Method::POST,
            &tail,
            Some(json!({ "reason": "again" })),
            &f.ops,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "only an unredeemed voucher can be voided"
    );
    let (_, trail) = f
        .call(
            Method::GET,
            &format!("/vouchers/{voucher}/timeline"),
            None,
            &s.neema,
        )
        .await;
    let void = trail["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "voided")
        .unwrap()
        .clone();
    assert_eq!(
        void["detail"]["reason"], "duplicate household",
        "the farmer sees the void and its reason"
    );
}

// ---- the trail ----

#[sqlx::test]
async fn the_trail_is_append_only_and_filtered_by_who_asks(pool: PgPool) {
    let (f, s, _, voucher, code) = setup(pool.clone(), 0).await;
    lookup(&f, &code, &f.officer_mga).await; // refused
    lookup(&f, &code, &s.juma).await; // scanned
    redeem(&f, &code, &s.juma).await;

    for sql in [
        "UPDATE farm_voucher_events SET actor_name = 'x'",
        "DELETE FROM farm_voucher_events",
    ] {
        let err = sqlx::query(sql).execute(&pool).await.unwrap_err();
        assert!(
            err.to_string().contains("voucher events cannot be changed"),
            "{err}"
        );
    }

    let farmer = timeline(&f, &voucher, &s.neema).await;
    assert_eq!(
        farmer,
        ["answered", "issued", "redeemed"],
        "the farmer sees no scans or staff steps"
    );
    let (_, raw) = f
        .call(
            Method::GET,
            &format!("/vouchers/{voucher}/timeline"),
            None,
            &s.neema,
        )
        .await;
    let redeemed = raw["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "redeemed")
        .unwrap()
        .clone();
    assert_eq!(
        redeemed["actor_name"], "Ada Lovelace",
        "the farmer sees who redeemed her voucher"
    );

    let staff = timeline(&f, &voucher, &s.juma).await;
    assert!(
        staff.contains(&"scanned".to_owned()) && !staff.contains(&"refused".to_owned()),
        "field staff: scans, no refusals: {staff:?}"
    );
    let ops = timeline(&f, &voucher, &f.ops).await;
    for kind in [
        "household_registered",
        "household_verified",
        "survey_published",
        "answered",
        "issued",
        "scanned",
        "refused",
        "redeemed",
        "login_initial",
    ] {
        assert!(
            ops.contains(&kind.to_owned()),
            "ops sees the whole trail ({kind}): {ops:?}"
        );
    }
    let (status, _) = f
        .call(
            Method::GET,
            &format!("/vouchers/{voucher}/timeline"),
            None,
            &f.officer_mga,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another village sees no trail"
    );
    let (status, _) = send(
        &f.app,
        Method::GET,
        &f.url(&format!("/vouchers/{voucher}/timeline")),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---- reconciliation ----

#[sqlx::test]
async fn redemptions_reconcile_per_officer_per_day_in_minor_units(pool: PgPool) {
    let (f, s, _, _, code) = setup(pool, 0).await;
    redeem(&f, &code, &s.juma).await;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let q = format!("?from=2026-01-01&to={}", today);
    let (status, totals) = f
        .call(
            Method::GET,
            &format!("/redemptions/totals{q}"),
            None,
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{totals}");
    assert_eq!(totals["data"][0]["vouchers"], 1);
    assert_eq!(totals["data"][0]["minor"], 200_000);
    assert_eq!(totals["data"][0]["currency"], "TZS");
    assert_eq!(totals["data"][0]["redeemed_by"]["name"], "Ada Lovelace");
    let (status, log) = f
        .call(Method::GET, &format!("/redemptions{q}"), None, &f.ops)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(log["data"][0]["id_type_seen"], "nida");
    for tail in ["/redemptions", "/redemptions/totals"] {
        let (status, _) = f
            .call(Method::GET, &format!("{tail}{q}"), None, &s.juma)
            .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "field officers do not see the cash log"
        );
    }
}
