//! Demo seed (G15): the RUAHA demo story in workspace `ruaha-demo`, built through the API's own
//! routes in-process, so every rule, provenance stamp and audit row applies. CLI only
//! (`genesy-360-api seed-farm-demo`), never an HTTP route. Runs once: a workspace of that name
//! that isn't flagged as demo is refused; a demo one is left alone.
//!
//! The story: Salima registers the Ilundo households, ops (Asha) verifies them, Juma (a second
//! Ilundo officer) redeems a voucher. Three vouchers worth TZS 10,000 are issued, TZS 2,000
//! redeemed; Neema still has two surveys to answer. One expected harvest was revised.

use std::net::SocketAddr;

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Method, Request},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

use crate::{AppState, build_app, error::AppError, modules::auth::password};

/// The demo workspace's name.
pub const WORKSPACE: &str = "ruaha-demo";

/// Staff accounts: (email, first name, last name, role, village code).
pub const STAFF: &[(&str, &str, &str, &str, Option<&str>)] = &[
    ("asha.ops@ruaha-demo.test", "Asha", "Mushi", "ops", None),
    (
        "salima@ruaha-demo.test",
        "Salima",
        "Hassan",
        "field_officer",
        Some("ILUNDO"),
    ),
    (
        "juma@ruaha-demo.test",
        "Juma",
        "Ally",
        "field_officer",
        Some("ILUNDO"),
    ),
    (
        "peter@ruaha-demo.test",
        "Peter",
        "Kisanga",
        "field_officer",
        Some("MGAMA"),
    ),
];
const OWNER: &str = "amina.admin@ruaha-demo.test";

/// What a run did. The passwords are random per run and exist only here: print them once.
pub struct Report {
    pub workspace_id: Uuid,
    /// False: the demo was already there and nothing changed (no passwords then).
    pub created: bool,
    /// Every demo staff account's password.
    pub staff_password: Option<String>,
    /// The password every demo farmer chose.
    pub farmer_password: Option<String>,
}

/// 16 random hex characters plus one of each class the staff password rule wants.
fn random_password() -> Result<String, AppError> {
    let (plain, _) = crate::modules::auth::token::generate()?;
    Ok(format!("Demo-{}-Aa1!", &plain.expose()[..16]))
}

/// In-process client: requests go through the full router, as from 127.0.0.1.
struct Api {
    app: Router,
    staff_password: String,
    farmer_password: String,
}

impl Api {
    async fn call(
        &self,
        method: Method,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> Result<Value, AppError> {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/v1{uri}"));
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header("content-type", "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        };
        let mut req = req.map_err(|e| AppError::Internal(e.to_string()))?;
        let peer: SocketAddr = ([127, 0, 0, 1], 0).into();
        req.extensions_mut().insert(ConnectInfo(peer));
        let res = self
            .app
            .clone()
            .oneshot(req)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let status = res.status();
        let bytes = to_bytes(res.into_body(), 1 << 20)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "demo seed: {method} {uri} -> {status}: {value}"
            )));
        }
        Ok(value)
    }

    async fn post(&self, uri: &str, body: Value, token: &str) -> Result<Value, AppError> {
        Ok(self
            .call(Method::POST, uri, Some(body), Some(token))
            .await?["data"]
            .take())
    }

    async fn login(&self, email: &str) -> Result<String, AppError> {
        let body = json!({ "email": email, "password": self.staff_password });
        let res = self.call(Method::POST, "/login", Some(body), None).await?;
        text(&res["data"], "access_token")
    }

    async fn farmer(
        &self,
        ws: &str,
        person: &str,
        phone: &str,
        officer: &str,
    ) -> Result<(), AppError> {
        let issued = self
            .post(
                &format!("/workspace/{ws}/farm/persons/{person}/login"),
                json!({}),
                officer,
            )
            .await?;
        let temp = text(&issued, "temp_password")?;
        let res = self
            .call(
                Method::POST,
                "/farm/login",
                Some(json!({ "phone": phone, "password": temp })),
                None,
            )
            .await?;
        let token = text(&res["data"], "access_token")?;
        let change = json!({ "current_password": temp, "password": self.farmer_password });
        self.call(Method::POST, "/farm/password", Some(change), Some(&token))
            .await?;
        Ok(())
    }
}

fn text(v: &Value, key: &str) -> Result<String, AppError> {
    v[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::Internal(format!("demo seed: no {key} in {v}")))
}

/// Seed the demo. Ok(created = false) when it is already there.
pub async fn run(mut state: AppState) -> Result<Report, AppError> {
    let existing = sqlx::query!(
        r#"SELECT id, coalesce(meta->>'demo', '') = 'true' AS "demo!" FROM workspaces
           WHERE name = $1 AND deleted_at IS NULL"#,
        WORKSPACE
    )
    .fetch_optional(&state.db)
    .await?;
    if let Some(w) = existing {
        return if w.demo {
            Ok(Report {
                workspace_id: w.id,
                created: false,
                staff_password: None,
                farmer_password: None,
            })
        } else {
            Err(AppError::Conflict(
                "workspace ruaha-demo exists and is not a demo workspace: refusing to seed",
            ))
        };
    }
    // In-process calls from one address: the public limits are for the internet, not for this.
    for limit in [
        &mut state.rate_limit,
        &mut state.private_rate_limit,
        &mut state.private_ip_rate_limit,
    ] {
        limit.burst = 100_000;
    }
    let db = state.db.clone();
    let api = Api {
        app: build_app(state),
        staff_password: random_password()?,
        farmer_password: random_password()?.to_lowercase(),
    };

    let signup = json!({ "first_name": "Amina", "last_name": "Admin", "email": OWNER,
                         "password": api.staff_password, "workspace_name": WORKSPACE });
    let session = api
        .call(Method::POST, "/create-workspace", Some(signup), None)
        .await?;
    let owner = text(&session["data"], "access_token")?;
    let ws_id: Uuid = session["data"]["workspace_ids"][0]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AppError::Internal("demo seed: no workspace id".into()))?;
    sqlx::query!(
        "UPDATE workspaces SET meta = coalesce(meta, '{}'::jsonb) || '{\"demo\": true}'::jsonb WHERE id = $1",
        ws_id
    )
    .execute(&db)
    .await?;
    let ws = ws_id.to_string();
    let farm = |tail: &str| format!("/workspace/{ws}/farm{tail}");

    let owner_id: Uuid = sqlx::query_scalar!("SELECT id FROM users WHERE email = $1", OWNER)
        .fetch_one(&db)
        .await?;
    api.post(
        &farm("/memberships"),
        json!({ "user_id": owner_id, "role": "admin" }),
        &owner,
    )
    .await?;
    let mut villages = std::collections::HashMap::new();
    for (code, name, lat, lon) in [
        ("ILUNDO", "Ilundo", "-7.7120", "35.6810"),
        ("MGAMA", "Mgama", "-7.8030", "35.7540"),
    ] {
        let v = api
            .post(
                &farm("/villages"),
                json!({ "code": code, "name": name, "latitude": lat, "longitude": lon }),
                &owner,
            )
            .await?;
        villages.insert(code, text(&v, "id")?);
    }

    // Staff join by direct insert: no invite mail goes out for a demo.
    let hash = password::hash(password::Password::generated(api.staff_password.clone())).await?;
    let mut tokens = std::collections::HashMap::new();
    for (email, first, last, role, village) in STAFF {
        let user: Uuid = sqlx::query_scalar!(
            "INSERT INTO users (email, first_name, last_name, password_hash, email_verified_at)
             VALUES ($1, $2, $3, $4, now()) RETURNING id",
            email,
            first,
            last,
            hash
        )
        .fetch_one(&db)
        .await?;
        sqlx::query!(
            "INSERT INTO workspace_members (workspace_id, user_id) VALUES ($1, $2)",
            ws_id,
            user
        )
        .execute(&db)
        .await?;
        let village = village.map(|c| villages[c].clone());
        api.post(
            &farm("/memberships"),
            json!({ "user_id": user, "role": role, "village_id": village }),
            &owner,
        )
        .await?;
        tokens.insert(*email, api.login(email).await?);
    }
    let (ops, salima, juma, peter) = (
        &tokens["asha.ops@ruaha-demo.test"],
        &tokens["salima@ruaha-demo.test"],
        &tokens["juma@ruaha-demo.test"],
        &tokens["peter@ruaha-demo.test"],
    );

    let mut crops = std::collections::HashMap::new();
    for (code, en, sw, measure) in [
        ("MAIZE", "Maize", "Mahindi", "area"),
        ("BANANA", "Banana", "Ndizi", "tree_count"),
        ("COFFEE", "Coffee", "Kahawa", "tree_count"),
    ] {
        let c = api
            .post(
                &farm("/crops"),
                json!({ "code": code, "name_en": en, "name_sw": sw, "measured_by": measure }),
                ops,
            )
            .await?;
        crops.insert(code, text(&c, "id")?);
    }

    let register = |body: Value| {
        let mut body = body;
        body["client_ref"] = json!(Uuid::now_v7());
        body
    };
    let ilundo = &villages["ILUNDO"];
    let neema = api
        .post(
            &farm("/registrations"),
            register(json!({
                "village_id": ilundo,
                "person": { "given_name": "Neema", "family_name": "Mwakalinga", "phone": "0712000001", "confidence": "high" },
                "household": { "label": "Mwakalinga household" },
                "farm": { "label": "Mwakalinga farm", "latitude": "-7.7101", "longitude": "35.6802" },
                "plot": { "label": "Riverside", "area_ha": "1.8" },
                "cycles": [
                    { "crop_id": crops["MAIZE"], "season_label": "2026 long rains", "area_ha": "1.6",
                      "harvest_start": "2026-09-01", "harvest_end": "2026-09-30",
                      "harvest": { "quantity_kg": "3200", "confidence": "medium", "reported_for": "2026-09-15" } },
                    { "crop_id": crops["BANANA"], "tree_count": 40 }
                ]
            })),
            salima,
        )
        .await?;
    api.post(
        &farm("/registrations"),
        register(json!({
            "village_id": ilundo,
            "person": { "given_name": "Rehema", "family_name": "Mwakalinga", "phone": "0712000002" },
            "household": { "existing_household_id": neema["household_id"] },
            "farm": { "existing_farm_id": neema["farm_id"] },
            "plot": { "label": "Hillside", "area_ha": "0.5" }
        })),
        salima,
    )
    .await?;
    let asha = api
        .post(
            &farm("/registrations"),
            register(json!({
                "village_id": ilundo,
                "person": { "given_name": "Asha", "family_name": "Kimaro", "phone": "0712000003" },
                "household": { "label": "Kimaro household" },
                "farm": { "label": "Kimaro farm", "latitude": "-7.7155", "longitude": "35.6833" },
                "plot": { "label": "Kimaro plot", "area_ha": "2.0" },
                "cycles": [{ "crop_id": crops["COFFEE"], "tree_count": 120 }]
            })),
            salima,
        )
        .await?;
    let bakari = api
        .post(
            &farm("/registrations"),
            register(json!({
                "village_id": ilundo,
                "person": { "given_name": "Bakari", "family_name": "Mfaume", "phone": "0712000004" },
                "household": { "label": "Mfaume household" },
                "farm": { "label": "Mfaume farm", "latitude": "-7.7188", "longitude": "35.6790" },
                "plot": { "label": "Mfaume plot", "area_ha": "1.2" },
                "cycles": [{ "crop_id": crops["MAIZE"], "area_ha": "1.2" }]
            })),
            salima,
        )
        .await?;
    api.post(
        &farm("/registrations"),
        register(json!({
            "village_id": villages["MGAMA"],
            "person": { "given_name": "Joseph", "family_name": "Mlowe", "phone": "0713000001" },
            "household": { "label": "Mlowe household" },
            "farm": { "label": "Mlowe farm", "latitude": "-7.8011", "longitude": "35.7522" },
            "plot": { "label": "Valley", "area_ha": "1.0" },
            "cycles": [{ "crop_id": crops["MAIZE"], "area_ha": "1.0", "harvest": { "quantity_kg": "1500" } }]
        })),
        peter,
    )
    .await?;

    // The maize estimate was revised: the first figure stays on record.
    let maize_cycle = neema["crop_cycle_ids"][0].as_str().unwrap_or_default();
    let revision = json!({ "kind": "expected", "quantity_kg": "4100", "source": "field_verified", "confidence": "high" });
    api.post(
        &farm(&format!("/crop-cycles/{maize_cycle}/harvest-reports")),
        revision,
        salima,
    )
    .await?;

    // A second pair of eyes on every Ilundo household: ops, not the registrar.
    for reg in [&neema, &asha, &bakari] {
        let household = text(reg, "household_id")?;
        api.post(
            &farm(&format!("/households/{household}/verification")),
            json!({}),
            ops,
        )
        .await?;
    }
    for (reg, phone) in [
        (&neema, "0712000001"),
        (&asha, "0712000003"),
        (&bakari, "0712000004"),
    ] {
        api.farmer(&ws, &text(reg, "person_id")?, phone, salima)
            .await?;
    }

    let tzs: Uuid = sqlx::query_scalar!(
        "INSERT INTO currencies (workspace_id, name, asset_type, decimals, active)
         VALUES ($1, 'Tanzanian shilling', 'TZS', 2, true) RETURNING id",
        ws_id
    )
    .fetch_one(&db)
    .await?;

    // Five surveys; amounts in minor units (TZS has 2 decimals).
    let mut surveys = Vec::new();
    for (title, sw, reward) in [
        (
            "Household energy use",
            "Matumizi ya nishati nyumbani",
            500_000,
        ),
        ("Water access", "Upatikanaji wa maji", 300_000),
        ("Market prices", "Bei za soko", 200_000),
        ("Maize storage", "Hifadhi ya mahindi", 100_000),
        ("Harvest labour", "Kazi za mavuno", 150_000),
    ] {
        let s = api
            .post(
                &farm("/surveys"),
                json!({ "title_en": title, "title_sw": sw, "reward_minor": reward, "currency_id": tzs, "audit_percent": 0 }),
                &owner,
            )
            .await?;
        let id = text(&s, "id")?;
        let q = json!({ "position": 1, "kind": "yes_no", "prompt_en": "Did you take part this season?",
                        "prompt_sw": "Ulishiriki msimu huu?" });
        let q = api
            .post(&farm(&format!("/surveys/{id}/questions")), q, &owner)
            .await?;
        api.post(&farm(&format!("/surveys/{id}/publish")), json!({}), &owner)
            .await?;
        surveys.push((id, text(&q, "id")?));
    }

    // Three households answer one survey each; those surveys then close.
    let mut codes = Vec::new();
    for ((survey, question), phone) in
        surveys
            .iter()
            .zip(["0712000001", "0712000003", "0712000004"])
    {
        let login = json!({ "phone": phone, "password": api.farmer_password });
        let res = api
            .call(Method::POST, "/farm/login", Some(login), None)
            .await?;
        let token = text(&res["data"], "access_token")?;
        let answer = json!({ "answers": { question.clone(): true }, "client_ref": Uuid::now_v7() });
        let issued = api
            .post(
                &farm(&format!("/surveys/{survey}/responses")),
                answer,
                &token,
            )
            .await?;
        codes.push(text(&issued, "code")?);
        api.post(
            &farm(&format!("/surveys/{survey}/close")),
            json!({}),
            &owner,
        )
        .await?;
    }
    // Juma neither registered nor verified the Mfaume household: he pays its TZS 2,000.
    let redeem = json!({ "code": codes[2], "id_type": "nida", "name_confirmed": true });
    api.post(&farm("/vouchers/redeem"), redeem, juma).await?;

    Ok(Report {
        workspace_id: ws_id,
        created: true,
        staff_password: Some(api.staff_password),
        farmer_password: Some(api.farmer_password),
    })
}
