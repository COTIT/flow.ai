//! Surveys (G14): admin authors draft -> live -> closed, content frozen once live; a farmer
//! answers once per household and gets a voucher; eligibility rules shared by list and submit.
//! RUAHA ids in docs/ruaha-rls-mapping.md.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{
    farm::{Farm, Surveyed, client_ref},
    send,
};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn setup(pool: PgPool) -> (Farm, Surveyed) {
    let f = Farm::new(pool).await;
    let s = f.surveyed().await;
    (f, s)
}

fn reason(list: &Value, survey: &str) -> Value {
    list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["survey_id"] == survey)
        .map(|r| r["reason"].clone())
        .unwrap_or(json!("not listed"))
}

async fn eligibility(f: &Farm, token: &str) -> Value {
    f.list("/surveys/eligibility", token).await
}

// ---- authoring ----

#[sqlx::test]
async fn only_an_admin_authors_and_publishing_checks_the_content(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let body = json!({ "title_en": "X", "reward_minor": 100, "currency_id": s.tzs });
    let (status, _) = f.call(Method::POST, "/surveys", Some(body), &f.ops).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "ops cannot author surveys");

    let (id, _) = f.survey(&s.tzs, 500_000, 0, true).await;
    let empty = f
        .call(
            Method::POST,
            "/surveys",
            Some(json!({ "title_en": "Empty", "reward_minor": 1, "currency_id": s.tzs })),
            &f.owner,
        )
        .await;
    let empty_id = empty.1["data"]["id"].as_str().unwrap().to_owned();
    let (status, res) = f
        .call(
            Method::POST,
            &format!("/surveys/{empty_id}/publish"),
            None,
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "a survey needs at least one question before it goes live"
    );

    let one_option = json!({ "position": 1, "kind": "single_choice", "prompt_en": "?", "options": [{ "value": "a", "label_en": "A" }] });
    f.call(
        Method::POST,
        &format!("/surveys/{empty_id}/questions"),
        Some(one_option),
        &f.owner,
    )
    .await;
    let (status, res) = f
        .call(
            Method::POST,
            &format!("/surveys/{empty_id}/publish"),
            None,
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "every choice question needs at least two options"
    );

    let (status, res) = f
        .call(
            Method::POST,
            &format!("/surveys/{id}/publish"),
            None,
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"]["status"], "live");
    assert_eq!(
        res["data"]["published_by"]["id"],
        f.user_id("owner@ruaha.test").await.as_str()
    );
    assert!(
        res["data"]["published_at"].is_string(),
        "publishing stamps who and when"
    );
}

#[sqlx::test]
async fn a_live_survey_is_frozen_and_a_closed_one_stays_closed(pool: PgPool) {
    let (f, s) = setup(pool.clone()).await;
    let (id, questions) = f.survey(&s.tzs, 500_000, 0, false).await;
    let (status, res) = f
        .call(
            Method::PATCH,
            &format!("/surveys/{id}"),
            Some(json!({ "reward_minor": 900_000 })),
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["message"], "a live survey cannot be edited");
    let q = json!({ "position": 3, "kind": "text", "prompt_en": "More?" });
    let (status, res) = f
        .call(
            Method::POST,
            &format!("/surveys/{id}/questions"),
            Some(q),
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "questions can only change while the survey is a draft"
    );
    let (status, _) = f
        .call(
            Method::DELETE,
            &format!("/surveys/{id}/questions/{}", questions[1]),
            None,
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    // The database refuses it too.
    let err = sqlx::query("UPDATE farm_surveys SET reward_minor = 1 WHERE id = $1::uuid")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("a live survey cannot be edited"),
        "{err}"
    );

    let (status, res) = f
        .call(
            Method::POST,
            &format!("/surveys/{id}/close"),
            None,
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let (status, res) = f
        .call(
            Method::POST,
            &format!("/surveys/{id}/publish"),
            None,
            &f.owner,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a closed survey cannot reopen: {res}"
    );
    let (status, res) = f
        .answer(
            &id,
            json!({ questions[0].clone(): "home" }),
            &client_ref(),
            &s.neema,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a closed survey takes no answers"
    );
    assert_eq!(res["message"], "this survey is closed");
}

#[sqlx::test]
async fn farmers_see_live_surveys_never_drafts(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let (draft, _) = f.survey(&s.tzs, 1000, 0, true).await;
    let (live, _) = f.survey(&s.tzs, 1000, 0, false).await;
    let ids: Vec<Value> = f.list("/surveys", &s.neema).await["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].clone())
        .collect();
    assert!(
        ids.contains(&json!(live)) && !ids.contains(&json!(draft)),
        "{ids:?}"
    );
    let (status, _) = f
        .call(Method::GET, &format!("/surveys/{draft}"), None, &s.neema)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        f.list("/surveys", &f.ops).await["meta"]["total"],
        2,
        "ops sees every survey"
    );
}

// ---- answering ----

#[sqlx::test]
async fn a_verified_household_answers_once_and_gets_one_voucher(pool: PgPool) {
    let (f, s) = setup(pool.clone()).await;
    let (id, q) = f.survey(&s.tzs, 500_000, 0, false).await;
    assert!(
        reason(&eligibility(&f, &s.neema).await, &id).is_null(),
        "eligible"
    );

    let r = client_ref();
    let answers = json!({ q[0].clone(): "store", q[1].clone(): "12" });
    let (status, res) = f.answer(&id, answers.clone(), &r, &s.neema).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    let d = &res["data"];
    assert_eq!(d["amount"]["minor"], 500_000);
    assert_eq!(d["amount"]["currency"], "TZS");
    assert_eq!(d["code"].as_str().unwrap().len(), 10);
    assert_eq!(d["replayed"], false);

    let (source, by): (String, String) =
        sqlx::query_as("SELECT source, captured_by::text FROM farm_survey_responses")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        source, "farmer_reported",
        "the response carries its provenance"
    );
    assert_eq!(by, f.user_id_by_phone("+255712000001").await);

    let (status, again) = f.answer(&id, answers, &r, &s.neema).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a retried submit is a replay: {again}"
    );
    assert_eq!(again["data"]["replayed"], true);
    assert_eq!(again["data"]["voucher_id"], d["voucher_id"]);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_vouchers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "a retried submit creates nothing new");

    let row = eligibility(&f, &s.neema).await["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["survey_id"] == id.as_str())
        .unwrap()
        .clone();
    assert_eq!(
        row["response_id"], d["response_id"],
        "the list shows the answer and its voucher"
    );
    assert_eq!(row["voucher_id"], d["voucher_id"]);
    assert_eq!(row["eligible"], false);

    // Someone else's reference is not theirs to replay.
    let (status, res) = f
        .answer(&id, json!({ q[0].clone(): "home" }), &r, &s.asha)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "this answer reference belongs to someone else"
    );
}

#[sqlx::test]
async fn the_household_answers_once_whoever_answers(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let (id, q) = f.survey(&s.tzs, 1000, 0, false).await;
    f.answer(
        &id,
        json!({ q[0].clone(): "home" }),
        &client_ref(),
        &s.neema,
    )
    .await;
    let rehema = f.signed_in_farmer(&s.c.rehema.person, "0712000002").await;
    assert_eq!(
        reason(&eligibility(&f, &rehema).await, &id),
        "your household has already answered this survey",
        "the co-member sees the household already answered"
    );
    let (status, res) = f
        .answer(&id, json!({ q[0].clone(): "home" }), &client_ref(), &rehema)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "your household has already answered this survey"
    );
}

#[sqlx::test]
async fn answers_are_checked_against_the_questions(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let (id, q) = f.survey(&s.tzs, 1000, 0, false).await;
    let other = "019a0000-0000-7000-8000-000000000999";
    for (answers, message) in [
        (json!({ q[1].clone(): 3 }), "question 1 is required"),
        (
            json!({ q[0].clone(): "boat" }),
            "question 1: choose one of the listed options",
        ),
        (
            json!({ q[0].clone(): "home", q[1].clone(): "many" }),
            "question 2: enter a number",
        ),
        (
            json!({ q[0].clone(): "home", other: "x" }),
            "an answer does not match any question in this survey",
        ),
    ] {
        let (status, res) = f.answer(&id, answers, &client_ref(), &s.neema).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
        assert_eq!(res["message"], message);
    }
}

#[sqlx::test]
async fn eligibility_rules(pool: PgPool) {
    let (f, s) = setup(pool.clone()).await;
    let (id, q) = f.survey(&s.tzs, 1000, 0, false).await;
    let answer = json!({ q[0].clone(): "home" });

    // A temporary password answers nothing.
    let (_, issued) = f
        .call(
            Method::POST,
            &format!("/persons/{}/login", s.c.rehema.person),
            None,
            &f.officer_ilu,
        )
        .await;
    let temp = f
        .farm_sign_in(
            "0712000002",
            issued["data"]["temp_password"].as_str().unwrap(),
        )
        .await;
    assert_eq!(
        reason(&eligibility(&f, &temp).await, &id),
        "set your own password before answering surveys"
    );
    let (status, res) = f.answer(&id, answer.clone(), &client_ref(), &temp).await;
    assert_eq!(
        (status, res["message"].clone()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!("set your own password before answering surveys")
        )
    );

    // Verified by its own registrar (only possible by direct write): not eligible.
    sqlx::query("UPDATE farm_households SET verified_by = captured_by WHERE id = $1::uuid")
        .bind(&s.c.asha.household)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        reason(&eligibility(&f, &s.asha).await, &id),
        "your household must be verified by a second staff member"
    );
    sqlx::query("UPDATE farm_households SET verification = 'unverified', verified_by = NULL, verified_at = NULL WHERE id = $1::uuid")
        .bind(&s.c.asha.household)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        reason(&eligibility(&f, &s.asha).await, &id),
        "your household has not been verified yet"
    );

    // A household registered after the survey went live.
    sqlx::query(
        "UPDATE farm_households SET created_at = now() + interval '1 minute' WHERE id = $1::uuid",
    )
    .bind(&s.c.neema.household)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        reason(&eligibility(&f, &s.neema).await, &id),
        "this survey opened before your household was registered"
    );
    sqlx::query(
        "UPDATE farm_households SET created_at = now() - interval '1 day' WHERE id = $1::uuid",
    )
    .bind(&s.c.neema.household)
    .execute(&pool)
    .await
    .unwrap();

    // A person in two households.
    sqlx::query(
        "INSERT INTO farm_household_members (workspace_id, village_id, household_id, person_id)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid)",
    )
    .bind(&f.ws)
    .bind(&f.ilundo)
    .bind(&s.c.asha.household)
    .bind(&s.c.neema.person)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        reason(&eligibility(&f, &s.neema).await, &id),
        "your household record needs checking by an officer"
    );
}

#[sqlx::test]
async fn the_household_cap_stops_the_next_household(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let (status, sv) = f
        .call(Method::POST, "/surveys", Some(json!({ "title_en": "Cap", "reward_minor": 100, "currency_id": s.tzs, "max_households": 1 })), &f.owner)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = sv["data"]["id"].as_str().unwrap().to_owned();
    let (_, q) = f
        .call(
            Method::POST,
            &format!("/surveys/{id}/questions"),
            Some(json!({ "position": 1, "kind": "yes_no", "prompt_en": "Ok?" })),
            &f.owner,
        )
        .await;
    let q = q["data"]["id"].as_str().unwrap().to_owned();
    f.call(
        Method::POST,
        &format!("/surveys/{id}/publish"),
        None,
        &f.owner,
    )
    .await;
    let (status, _) = f
        .answer(&id, json!({ q.clone(): true }), &client_ref(), &s.neema)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, res) = f
        .answer(&id, json!({ q: true }), &client_ref(), &s.asha)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["message"], "this survey has reached its limit");
}

#[sqlx::test]
async fn no_route_writes_responses_directly_and_anonymous_reads_nothing(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let (id, _) = f.survey(&s.tzs, 1000, 0, false).await;
    for (method, tail) in [
        (Method::GET, "/surveys".to_owned()),
        (Method::GET, "/surveys/eligibility".to_owned()),
        (Method::GET, format!("/surveys/{id}")),
    ] {
        let (status, _) = send(&f.app, method, &f.url(&tail), None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{tail}");
    }
    for method in [Method::PUT, Method::PATCH, Method::DELETE] {
        let (status, _) = f
            .call(
                method.clone(),
                &format!("/surveys/{id}/responses"),
                Some(json!({})),
                &f.ops,
            )
            .await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
    }
    let (status, _) = f
        .answer(&id, json!({}), &client_ref(), &f.officer_ilu)
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "staff without a person answer nothing"
    );
}

#[sqlx::test]
async fn results_count_answers_and_money_in_minor_units(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let (id, q) = f.survey(&s.tzs, 500_000, 0, false).await;
    f.answer(
        &id,
        json!({ q[0].clone(): "home", q[1].clone(): 4 }),
        &client_ref(),
        &s.neema,
    )
    .await;
    f.answer(
        &id,
        json!({ q[0].clone(): "home", q[1].clone(): 8 }),
        &client_ref(),
        &s.asha,
    )
    .await;
    let (status, res) = f
        .call(Method::GET, &format!("/surveys/{id}/results"), None, &f.ops)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_eq!(d["responses"], 2);
    assert_eq!(d["issued"]["count"], 2);
    assert_eq!(d["issued"]["minor"], 1_000_000);
    assert_eq!(d["outstanding"]["minor"], 1_000_000);
    assert_eq!(d["redeemed"]["minor"], 0);
    let tally = d["tally"].as_array().unwrap();
    assert!(
        tally
            .iter()
            .any(|t| t["option_value"] == "home" && t["answers"] == 2),
        "{tally:?}"
    );
    let numbers = d["numbers"].as_array().unwrap();
    assert_eq!(numbers[0]["average"], "6.00");
    let (status, _) = f
        .call(
            Method::GET,
            &format!("/surveys/{id}/results"),
            None,
            &s.neema,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "farmers see no results");
}

// ---- field-app follow-ups ----

#[sqlx::test]
async fn a_retried_create_with_the_same_id_makes_one_draft(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let id = client_ref();
    let body =
        json!({ "id": id, "title_en": "Offline draft", "reward_minor": 100, "currency_id": s.tzs });
    let (status, res) = f
        .call(Method::POST, "/surveys", Some(body.clone()), &f.owner)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["id"], id.as_str(), "the client's id is kept");
    let owner = f.user_id("owner@ruaha.test").await;
    assert_eq!(res["data"]["created_by"]["id"], owner.as_str());
    assert!(res["data"]["created_by"]["name"].is_string(), "{res}");

    let mut again = body.clone();
    again["title_en"] = json!("Changed on retry");
    let (status, res) = f
        .call(Method::POST, "/surveys", Some(again), &f.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "a retry replays: {res}");
    assert_eq!(
        res["data"]["title_en"], "Offline draft",
        "the stored draft wins"
    );
    assert_eq!(f.list("/surveys", &f.owner).await["meta"]["total"], 1);

    let (status, res) = f
        .call(
            Method::POST,
            "/surveys",
            Some(
                json!({ "id": "nope", "title_en": "X", "reward_minor": 100, "currency_id": s.tzs }),
            ),
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"]["id"][0], "The id must be a valid UUID.");
    let (status, _) = f
        .call(
            Method::POST,
            "/surveys",
            Some(json!({ "id": client_ref(), "title_en": "X", "reward_minor": 100, "currency_id": s.tzs })),
            &s.juma,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "officers never author");

    let other = Farm::new_named(f.pool.clone(), "else.test", "elsewhere").await;
    let other_tzs = other.tzs().await;
    let (status, res) = other
        .call(
            Method::POST,
            "/surveys",
            Some(
                json!({ "id": id, "title_en": "X", "reward_minor": 100, "currency_id": other_tzs }),
            ),
            &other.owner,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "another workspace's id: {res}"
    );
    assert_eq!(res["message"], "This id is already in use.");
    assert_eq!(
        other.list("/surveys", &other.owner).await["meta"]["total"],
        0,
        "nothing leaks into the other workspace"
    );
}

#[sqlx::test]
async fn the_list_carries_voucher_sums_for_staff_only(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let (id, q) = f.survey(&s.tzs, 500_000, 0, false).await;
    let mut vouchers = Vec::new();
    for token in [&s.neema, &s.asha] {
        let (status, res) = f
            .answer(&id, json!({ q[0].clone(): "home" }), &client_ref(), token)
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        vouchers.push(res["data"].clone());
    }
    let redeem = json!({ "code": vouchers[0]["code"], "id_type": "nida", "name_confirmed": true });
    let (status, res) = f
        .call(Method::POST, "/vouchers/redeem", Some(redeem), &s.juma)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let void = format!(
        "/vouchers/{}/void",
        vouchers[1]["voucher_id"].as_str().unwrap()
    );
    let (status, res) = f
        .call(
            Method::POST,
            &void,
            Some(json!({ "reason": "duplicate" })),
            &f.ops,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{res}");

    let row = |list: Value| {
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id.as_str())
            .unwrap()
            .clone()
    };
    let sum = row(f.list("/surveys", &f.ops).await)["summary"].clone();
    assert_eq!(
        sum["issued"],
        json!({ "count": 2, "minor": 1_000_000 }),
        "{sum}"
    );
    assert_eq!(sum["redeemed"], json!({ "count": 1, "minor": 500_000 }));
    assert_eq!(sum["outstanding"], json!({ "count": 0, "minor": 0 }));
    assert_eq!(sum["expired_count"], 0);
    assert_eq!(sum["void_count"], 1);
    let (_, results) = f
        .call(Method::GET, &format!("/surveys/{id}/results"), None, &f.ops)
        .await;
    for k in [
        "issued",
        "redeemed",
        "outstanding",
        "expired_count",
        "void_count",
    ] {
        assert_eq!(sum[k], results["data"][k], "{k} matches the results page");
    }
    assert_eq!(
        row(f.list("/surveys", &s.juma).await)["summary"],
        sum,
        "field officers see the sums too"
    );
    let farmer = row(f.list("/surveys", &s.neema).await);
    assert!(
        farmer.get("summary").is_some_and(Value::is_null),
        "farmers get null: {farmer}"
    );
    let one = f.list(&format!("/surveys/{id}"), &f.ops).await;
    assert_eq!(
        one["data"]["summary"], sum,
        "the single survey carries it too"
    );
}

#[sqlx::test]
async fn draft_edits_replace_and_remove_questions_preserve_requested_state(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let (id, questions) = f.survey(&s.tzs, 1000, 15, true).await;
    let tail = format!("/surveys/{id}");
    let (status, response) = f
        .call(
            Method::PATCH,
            &tail,
            Some(json!({
                "title_en":"Updated storage survey", "title_sw":null,
                "description_sw":"Maelezo", "reward_minor":125_000,
                "max_households":2, "audit_percent":0, "village_id":f.ilundo
            })),
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let saved = f.list(&tail, &f.owner).await["data"].clone();
    assert_eq!(saved["title_en"], "Updated storage survey");
    assert_eq!(saved["title_sw"], Value::Null);
    assert_eq!(saved["description_sw"], "Maelezo");
    assert_eq!(saved["reward"]["minor"], 125_000);
    assert_eq!(saved["reward"]["currency_id"], s.tzs);
    assert_eq!(saved["max_households"], 2);
    assert_eq!(saved["audit_percent"], 0);
    assert_eq!(saved["village_id"], f.ilundo);
    assert_eq!(saved["status"], "draft");
    assert_eq!(saved["questions"].as_array().unwrap().len(), 2);

    let replacement = json!({ "position":1, "kind":"multi_choice",
        "prompt_en":" Choose storage ", "prompt_sw":"Chagua hifadhi", "required":false,
        "options":[{"value":" home ","label_en":" Home ","label_sw":"Nyumbani"},
                   {"value":"store","label_en":"Store","label_sw":"Ghala"}] });
    let (status, response) = f
        .call(
            Method::PUT,
            &format!("{tail}/questions/{}", questions[0]),
            Some(replacement.clone()),
            &f.owner,
        )
        .await;
    assert_eq!((status, response), (StatusCode::NO_CONTENT, Value::Null));
    let saved = f.list(&tail, &f.owner).await["data"].clone();
    let first = saved["questions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["id"] == questions[0])
        .unwrap();
    assert_eq!(first["kind"], "multi_choice");
    assert_eq!(first["prompt_en"], "Choose storage");
    assert_eq!(first["prompt_sw"], "Chagua hifadhi");
    assert_eq!(first["required"], false);
    assert_eq!(
        first["options"],
        json!([
            {"value":"home","label_en":"Home","label_sw":"Nyumbani"},
            {"value":"store","label_en":"Store","label_sw":"Ghala"}
        ])
    );
    assert_eq!(saved["questions"][1]["id"], questions[1]);
    assert_eq!(saved["questions"][1]["kind"], "number");

    let (status, response) = f
        .call(
            Method::POST,
            &format!("{tail}/questions"),
            Some(json!({"position":3,"kind":"text","prompt_en":"Other storage?","required":false})),
            &f.owner,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{response}");
    let added = response["data"]["id"].as_str().unwrap().to_owned();
    let saved = f.list(&tail, &f.owner).await["data"].clone();
    assert_eq!(saved["questions"].as_array().unwrap().len(), 3);
    assert_eq!(saved["questions"][2]["id"], added);
    assert_eq!(saved["questions"][2]["prompt_en"], "Other storage?");

    let removed = format!("{tail}/questions/{}", questions[1]);
    let (status, response) = f.call(Method::DELETE, &removed, None, &f.owner).await;
    assert_eq!((status, response), (StatusCode::NO_CONTENT, Value::Null));
    let saved = f.list(&tail, &f.owner).await["data"].clone();
    let remaining: Vec<_> = saved["questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["id"].as_str().unwrap())
        .collect();
    assert_eq!(remaining, [questions[0].as_str(), added.as_str()]);
    assert_eq!(saved["questions"][0]["kind"], "multi_choice");
    assert_eq!(
        f.call(Method::DELETE, &removed, None, &f.owner).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.call(Method::PUT, &removed, Some(replacement), &f.owner)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn draft_edit_routes_reject_non_admin_foreign_targets_and_invalid_fields(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let (id, questions) = f.survey(&s.tzs, 1000, 0, true).await;
    let other = Farm::new_named(f.pool.clone(), "survey-other.test", "survey-other").await;
    let tail = format!("/surveys/{id}");
    let question = json!({"position":1,"kind":"text","prompt_en":"Changed"});
    let calls = [
        (
            Method::PATCH,
            tail.clone(),
            Some(json!({"title_en":"Changed"})),
        ),
        (
            Method::POST,
            format!("{tail}/questions"),
            Some(json!({"position":3,"kind":"text","prompt_en":"Added"})),
        ),
        (
            Method::PUT,
            format!("{tail}/questions/{}", questions[0]),
            Some(question.clone()),
        ),
        (
            Method::DELETE,
            format!("{tail}/questions/{}", questions[0]),
            None,
        ),
    ];
    for (method, path, body) in &calls {
        let (status, response) = f.call(method.clone(), path, body.clone(), &f.ops).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {response}");
        let (status, response) = other
            .call(method.clone(), path, body.clone(), &other.owner)
            .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "foreign {method} {path}: {response}"
        );
        let (status, _) = send(&f.app, method.clone(), &f.url(path), body.clone(), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
    for (method, path, body, field) in [
        (
            Method::PATCH,
            tail.clone(),
            json!({"title_en":null}),
            "title_en",
        ),
        (
            Method::POST,
            format!("{tail}/questions"),
            json!({"position":0,"kind":"text","prompt_en":"Invalid"}),
            "position",
        ),
        (
            Method::PUT,
            format!("{tail}/questions/{}", questions[0]),
            json!({"position":1,"kind":"invented","prompt_en":"Invalid"}),
            "kind",
        ),
    ] {
        let (status, response) = f.call(method, &path, Some(body), &f.owner).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert!(response["errors"][field].is_array(), "{response}");
    }
    let (another, other_questions) = f.survey(&s.tzs, 1000, 0, true).await;
    assert_ne!(another, id);
    for method in [Method::PUT, Method::DELETE] {
        let body = (method == Method::PUT).then(|| question.clone());
        let (status, _) = f
            .call(
                method,
                &format!("{tail}/questions/{}", other_questions[0]),
                body,
                &f.owner,
            )
            .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "question must belong to selected survey"
        );
    }
    let saved = f.list(&tail, &f.owner).await["data"].clone();
    assert_eq!(saved["title_en"], "Maize storage");
    assert_eq!(saved["questions"].as_array().unwrap().len(), 2);
    assert_eq!(saved["questions"][0]["id"], questions[0]);
    assert_eq!(saved["questions"][0]["kind"], "single_choice");
}

#[sqlx::test]
async fn voucher_list_returns_only_selected_survey_without_codes_and_enforces_scope(pool: PgPool) {
    let (f, s) = setup(pool).await;
    let (id, questions) = f.survey(&s.tzs, 500_000, 0, false).await;
    let mut expected = Vec::new();
    for token in [&s.neema, &s.asha] {
        let (status, response) = f
            .answer(
                &id,
                json!({questions[0].clone():"home"}),
                &client_ref(),
                token,
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{response}");
        expected.push(response["data"]["voucher_id"].as_str().unwrap().to_owned());
    }
    let (another, another_questions) = f.survey(&s.tzs, 1000, 0, false).await;
    let (status, response) = f
        .answer(
            &another,
            json!({another_questions[0].clone():"home"}),
            &client_ref(),
            &s.neema,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{response}");
    let unrelated = response["data"]["voucher_id"].clone();
    let tail = format!("/surveys/{id}/vouchers");
    let listed = f.list(&tail, &f.ops).await;
    assert_eq!(listed["meta"]["total"], 2);
    let rows = listed["data"].as_array().unwrap();
    let mut got: Vec<_> = rows
        .iter()
        .map(|v| v["voucher_id"].as_str().unwrap().to_owned())
        .collect();
    got.sort();
    expected.sort();
    assert_eq!(got, expected);
    for row in rows {
        assert_ne!(row["voucher_id"], unrelated);
        assert_eq!(row["amount"]["minor"], 500_000);
        assert_eq!(row["amount"]["currency"], "TZS");
        assert_eq!(row["status"], "issued");
        assert_eq!(row["audit_required"], false);
        assert_eq!(row["expired"], false);
        assert!(row.get("code").is_none() && row.get("code_hash").is_none());
        assert!(
            row["respondent_name"]
                .as_str()
                .is_some_and(|n| !n.is_empty())
        );
    }
    let first = f.list(&format!("{tail}?page_size=1"), &f.owner).await;
    let second = f
        .list(&format!("{tail}?page_size=1&page=2"), &f.owner)
        .await;
    assert_eq!(first["meta"]["total"], 2);
    assert_eq!(second["meta"]["total"], 2);
    assert_ne!(
        first["data"][0]["voucher_id"],
        second["data"][0]["voucher_id"]
    );
    for token in [&s.neema, &f.officer_ilu] {
        assert_eq!(
            f.call(Method::GET, &tail, None, token).await.0,
            StatusCode::FORBIDDEN
        );
    }
    let other = Farm::new_named(f.pool.clone(), "voucher-other.test", "voucher-other").await;
    assert_eq!(
        other.call(Method::GET, &tail, None, &other.owner).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.call(Method::GET, "/surveys/nope/vouchers", None, &f.ops)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&f.app, Method::GET, &f.url(&tail), None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
}
