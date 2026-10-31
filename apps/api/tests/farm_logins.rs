//! Farmer logins (G11): an officer issues a temporary password for a registered person's phone;
//! the farmer signs in with phone + password, must choose their own at first sign-in; lockout
//! after repeated failures. Phone-only accounts reach farm routes and nothing else.
//! RUAHA ids in docs/ruaha-rls-mapping.md.

mod common;

use axum::http::{Method, StatusCode};
use common::{
    farm::{Cast, Farm},
    send,
};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn setup(pool: PgPool) -> (Farm, Cast) {
    let f = Farm::new(pool).await;
    let c = f.cast().await;
    (f, c)
}

async fn issue(f: &Farm, person: &str, token: &str) -> (StatusCode, Value) {
    f.call(
        Method::POST,
        &format!("/persons/{person}/login"),
        None,
        token,
    )
    .await
}

async fn issued(f: &Farm, person: &str) -> Value {
    let (status, res) = issue(f, person, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    res["data"].clone()
}

async fn sign_in(f: &Farm, phone: &str, password: &str) -> (StatusCode, Value) {
    send(
        &f.app,
        Method::POST,
        "/v1/farm/login",
        Some(json!({ "phone": phone, "password": password })),
        None,
    )
    .await
}

async fn change(f: &Farm, token: &str, current: &str, new: &str) -> (StatusCode, Value) {
    send(
        &f.app,
        Method::POST,
        "/v1/farm/password",
        Some(json!({ "current_password": current, "password": new })),
        Some(token),
    )
    .await
}

fn temp(login: &Value) -> String {
    login["temp_password"].as_str().unwrap().to_owned()
}

// ---- issue ----

#[sqlx::test]
async fn an_officer_issues_a_first_login_from_the_registered_phone(pool: PgPool) {
    let (f, c) = setup(pool.clone()).await;
    let login = issued(&f, &c.neema.person).await;
    assert_eq!(
        login["phone"], "+255712000001",
        "the login name is the normalised phone"
    );
    assert_eq!(login["kind"], "initial");
    assert_eq!(temp(&login).len(), 8);

    let (email, phone, must_change): (Option<String>, String, bool) = sqlx::query_as(
        "SELECT u.email, u.phone, u.must_change_password FROM users u
         JOIN farm_persons p ON p.user_id = u.id WHERE p.id = $1::uuid",
    )
    .bind(&c.neema.person)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (email, phone.as_str(), must_change),
        (None, "+255712000001", true),
        "no synthetic email"
    );

    let (role, village): (String, String) = sqlx::query_as(
        "SELECT m.role, m.village_id::text FROM farm_memberships m
         JOIN farm_persons p ON p.user_id = m.user_id WHERE p.id = $1::uuid",
    )
    .bind(&c.neema.person)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (role.as_str(), village.as_str()),
        ("farmer", f.ilundo.as_str())
    );

    let (kind, by): (String, String) =
        sqlx::query_as("SELECT kind, issued_by::text FROM farm_login_issues")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (kind.as_str(), by.as_str()),
        ("initial", f.user_id("salima@ruaha.test").await.as_str())
    );
    let stored: String = sqlx::query_scalar("SELECT temp_password_hash FROM farm_login_issues")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!stored.contains(&temp(&login)), "only the hash is stored");
}

#[sqlx::test]
async fn issuing_again_resets_instead_of_creating_a_second_login(pool: PgPool) {
    let (f, c) = setup(pool.clone()).await;
    let first = issued(&f, &c.neema.person).await;
    let (_, session) = sign_in(&f, "0712000001", &temp(&first)).await;
    let old_token = session["data"]["access_token"].as_str().unwrap().to_owned();

    let again = issued(&f, &c.neema.person).await;
    assert_eq!(again["kind"], "reset");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE phone = '+255712000001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "still exactly one login for the person");
    let (status, _) = f.call(Method::GET, "/villages", None, &old_token).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a reset signs the farmer out everywhere"
    );
    assert_eq!(
        sign_in(&f, "0712000001", &temp(&first)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        sign_in(&f, "0712000001", &temp(&again)).await.0,
        StatusCode::OK
    );
    let issues: i64 = sqlx::query_scalar("SELECT count(*) FROM farm_login_issues")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(issues, 2, "every issue is recorded");
}

#[sqlx::test]
async fn issue_refusals(pool: PgPool) {
    let (f, c) = setup(pool).await;
    issued(&f, &c.neema.person).await;
    // Rehema's record carries Neema's phone: that phone already signs someone in.
    let body =
        json!({ "given_name": "Rehema", "family_name": "Mwakalinga", "phone": "0712000001" });
    let (status, _) = f
        .call(
            Method::PUT,
            &format!("/persons/{}", c.rehema.person),
            Some(body),
            &f.officer_ilu,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, res) = issue(&f, &c.rehema.person, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["message"], "this phone number already has an app login");

    let body = json!({ "given_name": "Asha", "family_name": "Kimaro", "phone": null });
    f.call(
        Method::PUT,
        &format!("/persons/{}", c.asha.person),
        Some(body),
        &f.officer_ilu,
    )
    .await;
    let (status, res) = issue(&f, &c.asha.person, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["message"], "add a phone number before issuing a login");
}

#[sqlx::test]
async fn a_staff_account_is_never_reset_from_a_person(pool: PgPool) {
    let (f, c) = setup(pool).await;
    f.link("peter@ruaha.test", &c.joseph.person).await;
    let (status, res) = issue(&f, &c.joseph.person, &f.ops).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["message"],
        "this person signs in as staff: their password cannot be reset here"
    );
}

#[sqlx::test]
async fn a_phone_with_a_login_in_another_workspace_is_refused(pool: PgPool) {
    let (f, c) = setup(pool.clone()).await;
    // Someone in another workspace already signs in with Neema's phone.
    sqlx::query(
        "INSERT INTO users (phone, first_name, last_name, password_hash) VALUES ('+255712000001', 'X', 'Y', 'h')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (status, res) = issue(&f, &c.neema.person, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["message"], "this phone number already has an app login");
}

#[sqlx::test]
async fn who_may_issue_and_read_the_history(pool: PgPool) {
    let (f, c) = setup(pool).await;
    let farmer = f
        .farmer_for("neema@ruaha.test", &f.ilundo, &c.neema.person)
        .await;
    let (status, _) = issue(&f, &c.rehema.person, &farmer).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a farmer cannot issue logins"
    );
    let (status, _) = issue(&f, &c.asha.person, &f.officer_mga).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a Mgama officer cannot issue an Ilundo login"
    );
    let (status, _) = send(
        &f.app,
        Method::POST,
        &f.url(&format!("/persons/{}/login", c.asha.person)),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    issued(&f, &c.asha.person).await;
    let history = format!("/persons/{}/login-history", c.asha.person);
    let (status, res) = f.call(Method::GET, &history, None, &f.officer_ilu).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"][0]["kind"], "initial");
    assert_eq!(res["data"][0]["issued_by"]["name"], "Ada Lovelace");
    assert_eq!(res["data"][0]["must_change_password"], true);
    let (status, _) = f.call(Method::GET, &history, None, &f.officer_mga).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = f.call(Method::GET, &history, None, &farmer).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "clients other than village staff can't read it"
    );
}

// ---- sign in ----

#[sqlx::test]
async fn a_farmer_signs_in_with_any_spelling_of_the_phone(pool: PgPool) {
    let (f, c) = setup(pool).await;
    let login = issued(&f, &c.neema.person).await;
    let (status, res) = sign_in(&f, "+255 712 000 001", &temp(&login)).await;
    assert_eq!(status, StatusCode::OK, "{res}");
    let d = &res["data"];
    assert_eq!(d["must_change_password"], true);
    assert_eq!(d["memberships"][0]["workspace_id"], f.ws.as_str());
    assert_eq!(d["memberships"][0]["role"], "farmer");
    assert_eq!(d["memberships"][0]["village_id"], f.ilundo.as_str());
    let token = d["access_token"].as_str().unwrap();
    let farms = f.list("/farms", token).await;
    assert_eq!(
        farms["data"][0]["id"],
        c.neema.farm.as_str(),
        "the login reads her own farm"
    );
}

#[sqlx::test]
async fn unknown_phone_and_wrong_password_look_the_same(pool: PgPool) {
    let (f, c) = setup(pool).await;
    issued(&f, &c.neema.person).await;
    let wrong = sign_in(&f, "0712000001", "not-it-at-all").await;
    let unknown = sign_in(&f, "0719999999", "not-it-at-all").await;
    let junk = sign_in(&f, "hello", "not-it-at-all").await;
    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong, unknown);
    assert_eq!(wrong, junk);
}

#[sqlx::test]
async fn ten_failures_lock_the_phone_until_an_officer_resets(pool: PgPool) {
    let (f, c) = setup(pool).await;
    let login = issued(&f, &c.neema.person).await;
    for _ in 0..10 {
        assert_eq!(
            sign_in(&f, "0712000001", "wrong-password").await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        sign_in(&f, "0712000001", &temp(&login)).await.0,
        StatusCode::TOO_MANY_REQUESTS,
        "locked even with the right password"
    );
    for _ in 0..11 {
        sign_in(&f, "0719999999", "wrong-password").await;
    }
    assert_eq!(
        sign_in(&f, "0719999999", "wrong-password").await.0,
        StatusCode::TOO_MANY_REQUESTS,
        "unknown phones lock the same way"
    );
    let again = issued(&f, &c.neema.person).await;
    assert_eq!(
        sign_in(&f, "0712000001", &temp(&again)).await.0,
        StatusCode::OK,
        "a reset clears the lock"
    );
}

// ---- password change ----

#[sqlx::test]
async fn the_temporary_password_does_not_count_as_a_new_one(pool: PgPool) {
    let (f, c) = setup(pool.clone()).await;
    let login = issued(&f, &c.neema.person).await;
    let (_, s) = sign_in(&f, "0712000001", &temp(&login)).await;
    let token = s["data"]["access_token"].as_str().unwrap().to_owned();

    let (status, res) = change(&f, &token, &temp(&login), &temp(&login)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["errors"]["password"][0], "choose a new password first");

    let (status, res) = change(&f, &token, "wrong-current", "maji safi 2026").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
    assert!(res["errors"]["current_password"].is_array());

    let (status, res) = change(&f, &token, &temp(&login), "maji safi 2026").await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{res}");
    let must: bool =
        sqlx::query_scalar("SELECT must_change_password FROM users WHERE phone = '+255712000001'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!must, "a changed password clears the flag");
    assert_eq!(
        sign_in(&f, "0712000001", "maji safi 2026").await.0,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn farmer_password_rules(pool: PgPool) {
    let (f, c) = setup(pool).await;
    let login = issued(&f, &c.neema.person).await;
    let (_, s) = sign_in(&f, "0712000001", &temp(&login)).await;
    let token = s["data"]["access_token"].as_str().unwrap().to_owned();
    for bad in [
        "short",          // under 8
        &"x".repeat(129), // over 128
        "aaaaaaaa",       // one repeated character
        "12345678",       // a straight run
        "87654321",
        "abcdefgh",
        "password", // obvious
        "Password",
        "mama2000001", // her phone's last 7 digits
    ] {
        let (status, res) = change(&f, &token, &temp(&login), bad).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {res}");
    }
    let (status, _) = change(&f, &token, &temp(&login), "kahawa ya asubuhi").await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "no composition rule: words are fine"
    );
}

// ---- phone-only accounts outside the farm ----

#[sqlx::test]
async fn a_phone_only_account_reaches_farm_routes_and_nothing_else(pool: PgPool) {
    let (f, c) = setup(pool).await;
    let login = issued(&f, &c.neema.person).await;
    let (_, s) = sign_in(&f, "0712000001", &temp(&login)).await;
    let token = s["data"]["access_token"].as_str().unwrap().to_owned();

    for (method, uri) in [
        (Method::GET, "/v1/me".to_owned()),
        (Method::GET, "/v1/me/workspaces".to_owned()),
        (Method::POST, "/v1/invites".to_owned()),
        (
            Method::POST,
            "/v1/email/verification-notification".to_owned(),
        ),
    ] {
        let body = (method == Method::POST)
            .then(|| json!({ "id": "019a0000-0000-7000-8000-000000000001" }));
        let (status, res) = send(&f.app, method.clone(), &uri, body, Some(&token)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {res}");
    }
    for uri in [
        format!("/v1/workspace/{}/products", f.ws),
        format!("/v1/workspace/{}/invites", f.ws),
    ] {
        let (status, _) = send(&f.app, Method::GET, &uri, None, Some(&token)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
    let (status, _) = send(
        &f.app,
        Method::POST,
        &format!("/v1/workspace/{}/invites", f.ws),
        Some(json!({ "email": "x@y.test" })),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "invite create");
    // Workspace create is the email signup: a phone is not an email.
    let (status, _) = send(
        &f.app,
        Method::POST,
        "/v1/create-workspace",
        Some(
            json!({ "first_name": "N", "last_name": "M", "email": "+255712000001",
                     "password": common::PASSWORD, "workspace_name": "neema" }),
        ),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    assert_eq!(
        f.call(Method::GET, "/villages", None, &token).await.0,
        StatusCode::OK
    );
    let (status, _) = send(&f.app, Method::POST, "/v1/logout", None, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "a farmer can sign out");
}

/// Captured log output of everything this thread traces.
#[derive(Clone, Default)]
struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[sqlx::test]
async fn the_temporary_password_never_reaches_the_logs(pool: PgPool) {
    let (f, c) = setup(pool).await;
    let logs = Captured::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    tracing::info!("capture-self-check");

    let login = issued(&f, &c.neema.person).await;
    let password = temp(&login);
    let (status, _) = sign_in(&f, "0712000001", &password).await;
    assert_eq!(status, StatusCode::OK);

    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("capture-self-check"), "the capture works");
    assert!(
        !text.contains(&password),
        "temporary password logged:\n{text}"
    );
}
