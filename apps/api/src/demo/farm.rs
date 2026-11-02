//! RUAHA on top of `farm_demo`: three more villages (one small grid), their officers, a
//! user with two roles, 55 more people over three seasons with harvests and verification,
//! surveys and vouchers in every state, energy requests and the market. `payouts` pays credited
//! farmers once the sales have run.

use std::collections::HashMap;

use axum::http::Method;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::{days_ago, names};
use crate::{
    error::AppError,
    farm_demo::{Api, OWNER, text},
    modules::auth::password,
};

/// What the supply chain and the payouts need from the farm side.
pub struct Ruaha {
    pub db: PgPool,
    pub ws: String,
    pub ws_id: Uuid,
    /// Admin, ops (Asha) and Hamisi (ops plus Ilundo officer, who neither registered nor
    /// verified anyone, so he may pay out).
    pub owner: String,
    pub ops: String,
    pub hamisi: String,
    pub hamisi_id: Uuid,
    pub tzs: String,
    pub coffee_crop: String,
    /// Verified actual coffee harvests per village: (village id, code, [(harvest id, kg)]).
    pub coffee: Vec<(String, &'static str, Harvests)>,
    /// New staff logins: (email, role).
    pub staff: Vec<(&'static str, &'static str)>,
    pub farmer_phones: Vec<String>,
    pub ilundo: String,
}

/// A staff user to add: (email, first name, last name, [(role, village code)]).
type Member = (
    &'static str,
    &'static str,
    &'static str,
    Vec<(&'static str, Option<&'static str>)>,
);
/// Harvests as (report id, kg).
type Harvests = Vec<(String, i64)>;
/// A demand to post: (buyer, crop, kg, window from/to in days ahead, price per kg minor,
/// village, opportunity status).
type Demand = (
    usize,
    &'static str,
    &'static str,
    i64,
    i64,
    i64,
    &'static str,
    &'static str,
);

/// One cycle as registered: (crop code, season, measure, harvest, final status, actual kg).
type Cycle = (
    &'static str,
    &'static str,
    &'static str,
    i64,
    &'static str,
    i64,
);

/// Two cycles per registration, by `n % 5`; `size` varies the measure.
fn cycles(n: usize) -> [Cycle; 2] {
    let size = (n % 4) as i64;
    let trees = 60 + 20 * size;
    match n % 5 {
        0 => [
            (
                "MAIZE",
                "2026 long rains",
                "1.2",
                2400 + 100 * size,
                "harvested",
                2150 + 90 * size,
            ),
            (
                "BEANS",
                "2026 short rains",
                "0.5",
                350,
                if n.is_multiple_of(2) {
                    "planned"
                } else {
                    "growing"
                },
                0,
            ),
        ],
        1 => [
            (
                "COFFEE",
                "2026 main crop",
                "",
                trees * 3,
                "harvested",
                trees * 3 - 15,
            ),
            ("BANANA", "2026 long rains", "30", 900, "growing", 0),
        ],
        2 => [
            (
                "MAIZE",
                "2025 short rains",
                "0.8",
                1500,
                if n % 7 == 2 { "abandoned" } else { "harvested" },
                1300,
            ),
            ("SUNFLOWER", "2026 short rains", "0.6", 500, "growing", 0),
        ],
        3 => [
            (
                "COFFEE",
                "2026 main crop",
                "",
                trees * 3,
                "harvested",
                trees * 3 - 10,
            ),
            ("AVOCADO", "2026 long rains", "12", 600, "growing", 0),
        ],
        _ => [
            ("BEANS", "2026 long rains", "0.7", 450, "harvested", 420),
            ("MAIZE", "2026 short rains", "1.0", 2000, "planned", 0),
        ],
    }
}

/// (planted, harvest start, harvest end) of a season.
fn season(label: &str) -> (&'static str, &'static str, &'static str) {
    match label {
        "2025 short rains" => ("2025-10-15", "2026-01-15", "2026-02-15"),
        "2026 long rains" => ("2026-03-01", "2026-07-01", "2026-08-31"),
        "2026 main crop" => ("2026-01-10", "2026-06-01", "2026-09-30"),
        _ => ("2026-10-20", "2027-01-15", "2027-02-15"),
    }
}

/// `-7.7120` → `-77120` (ten-thousandths) and back.
fn ten_k(s: &str) -> i64 {
    s.replace('.', "").parse().unwrap_or(0)
}
fn coord(v: i64) -> String {
    let sign = if v < 0 { "-" } else { "" };
    format!("{sign}{}.{:04}", v.abs() / 10_000, v.abs() % 10_000)
}

fn id_of(list: &Value, key: &str, value: &str) -> Result<String, AppError> {
    list["data"]
        .as_array()
        .and_then(|v| v.iter().find(|v| v[key] == value))
        .map(|v| v["id"].as_str().unwrap_or_default().to_owned())
        .ok_or_else(|| AppError::Internal(format!("demo seed: no {value}")))
}

pub async fn seed(api: &Api, ws_id: Uuid, db: &PgPool) -> Result<Ruaha, AppError> {
    let ws = ws_id.to_string();
    let farm = |tail: &str| format!("/workspace/{ws}/farm{tail}");
    let owner = api.login(OWNER).await?;
    let ops = api.login("asha.ops@ruaha-demo.test").await?;

    let currencies = api
        .call(
            Method::GET,
            &format!("/workspace/{ws}/currencies"),
            None,
            Some(&owner),
        )
        .await?;
    let tzs = currencies["data"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["asset_type"] == "TZS"))
        .and_then(|c| c["id"].as_str().or_else(|| c["currency_id"].as_str()))
        .ok_or_else(|| AppError::Internal("demo seed: no TZS".into()))?
        .to_owned();

    // Villages: the two from farm_demo plus three; Mwangeza's grid is small (40 kW).
    let mut villages: Vec<(&'static str, String, i64, i64, String)> = Vec::new();
    let listed = api
        .call(Method::GET, &farm("/villages"), None, Some(&ops))
        .await?;
    for (code, lat, lon, prefix) in [
        ("ILUNDO", "-7.7120", "35.6810", "0712100"),
        ("MGAMA", "-7.8030", "35.7540", "0713100"),
    ] {
        villages.push((
            code,
            id_of(&listed, "code", code)?,
            ten_k(lat),
            ten_k(lon),
            prefix.into(),
        ));
    }
    for nv in names::VILLAGES {
        let body = json!({ "code": nv.code, "name": nv.name, "latitude": nv.lat, "longitude": nv.lon,
            "capacity": { "capacity_kw": nv.kw, "basis": "planned", "simultaneity_factor": "0.650",
                "source_note": "DEMO. Planned mini-grid, not measured.", "effective_from": days_ago(200) } });
        let v = api.post(&farm("/villages"), body, &owner).await?;
        villages.push((
            nv.code,
            text(&v, "id")?,
            ten_k(nv.lat),
            ten_k(nv.lon),
            nv.phones.into(),
        ));
    }
    let village = |code: &str| {
        villages
            .iter()
            .find(|v| v.0 == code)
            .map(|v| v.1.clone())
            .unwrap_or_default()
    };

    // Staff: an officer per new village, a second Mgama officer, Hamisi with two roles.
    let hash = password::hash(password::Password::generated(api.staff_password.clone())).await?;
    let mut staff: Vec<(&'static str, &'static str)> = Vec::new();
    let mut members: Vec<Member> = names::VILLAGES
        .iter()
        .map(|v| {
            (
                v.officer.0,
                v.officer.1,
                v.officer.2,
                vec![("field_officer", Some(v.code))],
            )
        })
        .collect();
    let (e, f, l) = names::MGAMA_OFFICER;
    members.push((e, f, l, vec![("field_officer", Some("MGAMA"))]));
    let (e, f, l) = names::TWO_ROLES;
    members.push((
        e,
        f,
        l,
        vec![("ops", None), ("field_officer", Some("ILUNDO"))],
    ));
    let mut tokens: HashMap<&str, String> = HashMap::new();
    let mut hamisi_id = Uuid::nil();
    for (email, first, last, roles) in &members {
        let user: Uuid = sqlx::query_scalar!(
            "INSERT INTO users (email, first_name, last_name, password_hash, email_verified_at)
             VALUES ($1, $2, $3, $4, now()) RETURNING id",
            email,
            first,
            last,
            hash
        )
        .fetch_one(db)
        .await?;
        sqlx::query!(
            "INSERT INTO workspace_members (workspace_id, user_id) VALUES ($1, $2)",
            ws_id,
            user
        )
        .execute(db)
        .await?;
        for (role, code) in roles {
            api.post(
                &farm("/memberships"),
                json!({ "user_id": user, "role": role, "village_id": code.map(village) }),
                &owner,
            )
            .await?;
        }
        staff.push((email, roles[0].0));
        if *email == names::TWO_ROLES.0 {
            hamisi_id = user;
        }
        tokens.insert(email, api.login(email).await?);
    }
    let officer_of = |code: &str| -> Result<String, AppError> {
        let email = match code {
            "ILUNDO" => "salima@ruaha-demo.test",
            "MGAMA" => "peter@ruaha-demo.test",
            _ => names::VILLAGES
                .iter()
                .find(|v| v.code == code)
                .map(|v| v.officer.0)
                .unwrap_or_default(),
        };
        Ok(email.to_owned())
    };
    for email in ["salima@ruaha-demo.test", "peter@ruaha-demo.test"] {
        tokens.insert(email, api.login(email).await?);
    }

    // Crops: farm_demo's three plus beans, sunflower and avocado.
    for (code, en, sw, measure) in [
        ("BEANS", "Beans", "Maharage", "area"),
        ("SUNFLOWER", "Sunflower", "Alizeti", "area"),
        ("AVOCADO", "Avocado", "Parachichi", "tree_count"),
    ] {
        api.post(
            &farm("/crops"),
            json!({ "code": code, "name_en": en, "name_sw": sw, "measured_by": measure }),
            &ops,
        )
        .await?;
    }
    let crop_list = api
        .call(Method::GET, &farm("/crops"), None, Some(&ops))
        .await?;
    let mut crops = HashMap::new();
    for code in ["MAIZE", "BANANA", "COFFEE", "BEANS", "SUNFLOWER", "AVOCADO"] {
        crops.insert(code, id_of(&crop_list, "code", code)?);
    }
    let measured = |code: &str| matches!(code, "COFFEE" | "BANANA" | "AVOCADO");

    // People: 14 Ilundo, 11 Mgama, 12 Kitowo, 10 Lugoda, 8 Mwangeza. Every 4th joins the
    // previous household and farm with a plot of their own.
    let mut coffee: HashMap<&str, Vec<(String, i64)>> = HashMap::new();
    let mut expected: HashMap<(&str, &str), Vec<(String, i64)>> = HashMap::new();
    let mut people: Vec<(&str, String)> = Vec::new();
    let mut farmer_phones: Vec<String> = ["0712000001", "0712000003", "0712000004"]
        .map(String::from)
        .to_vec();
    let mut n = 0usize;
    for (code, count) in [
        ("ILUNDO", 14),
        ("MGAMA", 11),
        ("KITOWO", 12),
        ("LUGODA", 10),
        ("MWANGEZA", 8),
    ] {
        let (_, vid, lat, lon, prefix) = villages
            .iter()
            .find(|v| v.0 == code)
            .cloned()
            .ok_or_else(|| AppError::Internal("demo seed: village".into()))?;
        let officer = tokens[officer_of(code)?.as_str()].clone();
        let mut last: Option<Value> = None;
        let mut family = "";
        for k in 0..count {
            n += 1;
            let joins = k % 4 == 3 && last.is_some();
            if !joins {
                family = names::TZ_FAMILY[(n * 3 + k) % names::TZ_FAMILY.len()];
            }
            let given = names::TZ_GIVEN[(n * 7) % names::TZ_GIVEN.len()];
            let phone = format!("{prefix}{:03}", k + 1);
            let (plat, plon) = (lat - 13 * (k as i64 % 9), lon + 11 * (k as i64 % 7));
            let (household, farm_body) = match (&last, joins) {
                (Some(prev), true) => (
                    json!({ "existing_household_id": prev["household_id"] }),
                    json!({ "existing_farm_id": prev["farm_id"] }),
                ),
                _ => (
                    json!({ "label": format!("{family} household") }),
                    json!({ "label": format!("{family} farm"), "latitude": coord(plat), "longitude": coord(plon) }),
                ),
            };
            let plan = cycles(n);
            let body_cycles: Vec<Value> = plan
                .iter()
                .map(|(crop, season_label, measure, kg, ..)| {
                    let (planted, start, end) = season(season_label);
                    let mut c = json!({ "crop_id": crops[crop], "season_label": season_label,
                        "planted_on": planted, "harvest_start": start, "harvest_end": end,
                        "harvest": { "quantity_kg": kg.to_string(), "confidence": "medium", "reported_for": start } });
                    if *crop == "COFFEE" {
                        c["tree_count"] = json!(kg / 3);
                    } else if measured(crop) {
                        c["tree_count"] = json!(measure.parse::<i64>().unwrap_or(10));
                    } else {
                        c["area_ha"] = json!(measure);
                    }
                    c
                })
                .collect();
            let reg = api
                .post(
                    &farm("/registrations"),
                    json!({ "client_ref": Uuid::now_v7(), "village_id": vid,
                        "person": { "given_name": given, "family_name": family, "phone": phone,
                                    "confidence": if k % 3 == 0 { "high" } else { "medium" } },
                        "household": household, "farm": farm_body,
                        "plot": { "label": format!("{} plot {}", ["Upper", "River", "Hill", "Lower"][k % 4], k + 1),
                                  "area_ha": format!("{}.{}", 1 + k % 2, (k * 3) % 10) },
                        "cycles": body_cycles }),
                    &officer,
                )
                .await?;
            let cycle_ids: Vec<String> = reg["crop_cycle_ids"]
                .as_array()
                .map(|v| {
                    v.iter()
                        .filter_map(|c| c.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let harvest_ids: Vec<String> = reg["harvest_report_ids"]
                .as_array()
                .map(|v| {
                    v.iter()
                        .filter_map(|c| c.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();

            for (i, (crop, season_label, measure, kg, status, actual)) in plan.iter().enumerate() {
                let cycle = cycle_ids.get(i).cloned().unwrap_or_default();
                if let Some(h) = harvest_ids.get(i) {
                    expected
                        .entry((code, crop))
                        .or_default()
                        .push((h.clone(), *kg));
                }
                // Registration makes a cycle `growing`; anything else is a correction.
                if *status != "growing" {
                    let (planted, start, end) = season(season_label);
                    let mut body = json!({ "crop_id": crops[crop], "status": status, "season_label": season_label,
                        "planted_on": planted, "harvest_start": start, "harvest_end": end });
                    if *crop == "COFFEE" {
                        body["tree_count"] = json!(kg / 3);
                    } else if measured(crop) {
                        body["tree_count"] = json!(measure.parse::<i64>().unwrap_or(10));
                    } else {
                        body["area_ha"] = json!(measure);
                    }
                    api.call(
                        Method::PUT,
                        &farm(&format!("/crop-cycles/{cycle}")),
                        Some(body),
                        Some(&officer),
                    )
                    .await?;
                }
                if *status == "harvested" {
                    let (_, start, _) = season(season_label);
                    let h = api
                        .post(
                            &farm(&format!("/crop-cycles/{cycle}/harvest-reports")),
                            json!({ "kind": "actual", "quantity_kg": actual.to_string(),
                                    "source": "field_verified", "reported_for": start }),
                            &officer,
                        )
                        .await?;
                    let h = text(&h, "id")?;
                    // The admin verifies, so ops may buy it into a supply batch.
                    api.post(
                        &farm(&format!("/harvest-reports/{h}/verification")),
                        json!({}),
                        &owner,
                    )
                    .await?;
                    if *crop == "COFFEE" {
                        coffee.entry(code).or_default().push((h, *actual));
                    }
                } else if n.is_multiple_of(6) {
                    // A revised estimate: the first figure stays on record.
                    api.post(
                        &farm(&format!("/crop-cycles/{cycle}/harvest-reports")),
                        json!({ "kind": "expected", "quantity_kg": (kg + 150).to_string(),
                                "source": "field_verified", "confidence": "high" }),
                        &officer,
                    )
                    .await?;
                }
                // Ops verifies most cycles; every 4th waits in the verify queue.
                if n % 4 != 1 {
                    api.post(
                        &farm(&format!("/crop-cycles/{cycle}/verification")),
                        json!({}),
                        &ops,
                    )
                    .await?;
                }
            }
            // Seven in ten households are verified by ops (four eyes: not the registrar), and every
            // farmer who signs in (surveys need a verified household).
            if n % 10 < 7 || k == 0 {
                for (kind, key) in [
                    ("persons", "person_id"),
                    ("farms", "farm_id"),
                    ("plots", "plot_id"),
                ] {
                    if kind == "farms" && joins {
                        continue;
                    }
                    let id = text(&reg, key)?;
                    api.post(
                        &farm(&format!("/{kind}/{id}/verification")),
                        json!({}),
                        &ops,
                    )
                    .await?;
                }
                if !joins {
                    let id = text(&reg, "household_id")?;
                    api.post(
                        &farm(&format!("/households/{id}/verification")),
                        json!({}),
                        &ops,
                    )
                    .await?;
                }
            }
            // The first farmer of each new village can sign in.
            if k == 0 && !matches!(code, "ILUNDO" | "MGAMA") {
                api.farmer(&ws, &text(&reg, "person_id")?, &phone, &officer)
                    .await?;
                farmer_phones.push(phone.clone());
            }
            people.push((code, text(&reg, "person_id")?));
            if !joins {
                last = Some(reg);
            }
        }
    }

    surveys(
        api,
        &ws,
        &owner,
        &ops,
        &tokens[names::TWO_ROLES.0],
        &tzs,
        &farmer_phones,
        db,
        ws_id,
    )
    .await?;
    energy(api, &ws, &ops, &tzs, &people, &tokens, &officer_of).await?;
    market(api, &ws, &ops, &tzs, &crops, &expected, &village).await?;

    let coffee = ["ILUNDO", "KITOWO", "LUGODA"]
        .into_iter()
        .map(|code| (village(code), code, coffee.remove(code).unwrap_or_default()))
        .collect();
    Ok(Ruaha {
        db: db.clone(),
        ws: ws.clone(),
        ws_id,
        owner,
        ops,
        hamisi: tokens[names::TWO_ROLES.0].clone(),
        hamisi_id,
        tzs,
        coffee_crop: crops["COFFEE"].clone(),
        coffee,
        staff,
        farmer_phones,
        ilundo: village("ILUNDO"),
    })
}

/// Eight surveys: two drafts, four live, two closed. Farmers who can sign in answer; some
/// vouchers are redeemed, one voided, two expired.
#[allow(clippy::too_many_arguments)]
async fn surveys(
    api: &Api,
    ws: &str,
    owner: &str,
    ops: &str,
    redeemer: &str,
    tzs: &str,
    phones: &[String],
    db: &PgPool,
    ws_id: Uuid,
) -> Result<(), AppError> {
    let farm = |tail: &str| format!("/workspace/{ws}/farm{tail}");
    let mut farmers = Vec::new();
    for phone in phones {
        let res = api
            .call(
                Method::POST,
                "/farm/login",
                Some(json!({ "phone": phone, "password": api.farmer_password })),
                None,
            )
            .await?;
        farmers.push(text(&res["data"], "access_token")?);
    }
    let mut vouchers: Vec<(String, String)> = Vec::new();
    let mut closing = Vec::new();
    for (i, (en, sw, reward)) in names::SURVEYS.iter().enumerate() {
        let s = api
            .post(
                &farm("/surveys"),
                json!({ "title_en": en, "title_sw": sw, "reward_minor": reward, "currency_id": tzs,
                        "audit_percent": 0, "max_households": 200,
                        "description_en": "DEMO survey. Answers stay in the project.",
                        "closes_at": format!("{}T18:00:00Z", days_ago(-30 - i as i64)) }),
                owner,
            )
            .await?;
        let id = text(&s, "id")?;
        let q1 = api
            .post(&farm(&format!("/surveys/{id}/questions")),
                json!({ "position": 1, "kind": "yes_no", "prompt_en": "Did this affect you this season?",
                        "prompt_sw": "Je, hili lilikuathiri msimu huu?" }), owner)
            .await?;
        let q2 = api
            .post(&farm(&format!("/surveys/{id}/questions")),
                json!({ "position": 2, "kind": "single_choice", "prompt_en": "How often?", "prompt_sw": "Mara ngapi?",
                        "options": [
                            { "value": "never", "label_en": "Never", "label_sw": "Kamwe" },
                            { "value": "sometimes", "label_en": "Sometimes", "label_sw": "Wakati mwingine" },
                            { "value": "often", "label_en": "Often", "label_sw": "Mara nyingi" }] }), owner)
            .await?;
        let q3 = api
            .post(&farm(&format!("/surveys/{id}/questions")),
                json!({ "position": 3, "kind": "number", "prompt_en": "How many people in the household helped?",
                        "prompt_sw": "Watu wangapi wa kaya walisaidia?" }), owner)
            .await?;
        if i < 2 {
            continue; // drafts
        }
        api.post(&farm(&format!("/surveys/{id}/publish")), json!({}), owner)
            .await?;
        let (q1, q2, q3) = (text(&q1, "id")?, text(&q2, "id")?, text(&q3, "id")?);
        for (f, token) in farmers.iter().enumerate() {
            if (f + i) % 3 == 0 {
                continue;
            }
            let often = ["never", "sometimes", "often"][(f + i) % 3];
            let answers =
                json!({ q1.clone(): f % 2 == 0, q2.clone(): often, q3.clone(): 1 + (f + i) % 5 });
            let issued = api
                .post(
                    &farm(&format!("/surveys/{id}/responses")),
                    json!({ "answers": answers, "client_ref": Uuid::now_v7() }),
                    token,
                )
                .await?;
            vouchers.push((text(&issued, "voucher_id")?, text(&issued, "code")?));
        }
        if i >= 6 {
            closing.push(id);
        }
    }
    for id in closing {
        api.post(&farm(&format!("/surveys/{id}/close")), json!({}), owner)
            .await?;
    }
    // Redeemed at the desk, one voided, two left to expire; the rest stay outstanding.
    for (_, code) in vouchers.iter().take(4) {
        api.post(
            &farm("/vouchers/redeem"),
            json!({ "code": code, "id_type": "nida", "name_confirmed": true }),
            redeemer,
        )
        .await?;
    }
    if let Some((id, _)) = vouchers.get(4) {
        api.post(
            &farm(&format!("/vouchers/{id}/void")),
            json!({ "reason": "DEMO: household answered twice on paper" }),
            ops,
        )
        .await?;
    }
    let expired: Vec<Uuid> = vouchers
        .iter()
        .skip(5)
        .take(2)
        .filter_map(|(id, _)| id.parse().ok())
        .collect();
    // No route ages a voucher: the expiry date moves back, as time would move it.
    sqlx::query!(
        "UPDATE farm_vouchers SET issued_at = now() - interval '40 days', expires_at = now() - interval '10 days'
         WHERE workspace_id = $1 AND id = ANY($2)",
        ws_id,
        &expired
    )
    .execute(db)
    .await?;
    Ok(())
}

/// More catalogue and loan listings, and PUE requests in every status.
async fn energy(
    api: &Api,
    ws: &str,
    ops: &str,
    tzs: &str,
    people: &[(&str, String)],
    tokens: &HashMap<&str, String>,
    officer_of: &dyn Fn(&str) -> Result<String, AppError>,
) -> Result<(), AppError> {
    let farm = |tail: &str| format!("/workspace/{ws}/farm{tail}");
    let solar = api
        .post(&farm("/equipment-categories"),
            json!({ "code": "SOLAR", "name_en": "Solar and storage", "name_sw": "Sola na hifadhi" }), ops)
        .await?;
    let cold = api
        .post(
            &farm("/equipment-categories"),
            json!({ "code": "COLD", "name_en": "Cold storage", "name_sw": "Hifadhi ya baridi" }),
            ops,
        )
        .await?;
    let cats = api
        .call(Method::GET, &farm("/equipment-categories"), None, Some(ops))
        .await?;
    let agro = id_of(&cats, "code", "AGRO")?;
    let mut equipment = Vec::new();
    for (cat, code, en, sw, kw, rent, buy_minor, rent_minor) in [
        (
            solar["id"].clone(),
            "SOLAR-HOME",
            "Solar home kit",
            "Kifaa cha sola nyumbani",
            "0.3",
            false,
            90_000_000_i64,
            0_i64,
        ),
        (
            cold["id"].clone(),
            "COLD-ROOM",
            "Village cold room",
            "Chumba baridi cha kijiji",
            "4",
            true,
            4_500_000_000,
            12_000_000,
        ),
        (
            json!(agro),
            "OIL-PRESS",
            "Sunflower oil press",
            "Mashine ya kukamua alizeti",
            "5.5",
            false,
            1_800_000_000,
            0,
        ),
        (
            json!(agro),
            "HULLER",
            "Coffee huller",
            "Mashine ya kukoboa kahawa",
            "7.5",
            true,
            2_600_000_000,
            9_000_000,
        ),
    ] {
        let mut body = json!({ "category_id": cat, "code": code, "name_en": en, "name_sw": sw,
            "rated_power_kw": kw, "typical_hours_per_day": "6", "typical_days_per_week": "5",
            "can_buy": true, "can_rent": rent, "indicative_price_minor": buy_minor, "currency_id": tzs });
        if rent {
            body["indicative_rent_per_day_minor"] = json!(rent_minor);
        }
        let e = api.post(&farm("/equipment"), body, ops).await?;
        equipment.push((text(&e, "id")?, rent));
    }
    for (code, en, sw, min, max) in [
        (
            "EQUIP",
            "Equipment loan",
            "Mkopo wa vifaa",
            200_000_000_i64,
            3_000_000_000_i64,
        ),
        (
            "SCHOOL",
            "School fees bridge",
            "Daraja la ada za shule",
            20_000_000,
            150_000_000,
        ),
    ] {
        api.post(
            &farm("/loan-products"),
            json!({ "code": code, "name_en": en, "name_sw": sw, "indicative_min_minor": min,
                    "indicative_max_minor": max, "currency_id": tzs,
                    "description_en": "DEMO listing: ask at the office." }),
            ops,
        )
        .await?;
    }
    // (person index, equipment, final status)
    let plan = [
        (3, 0, "draft"),
        (8, 1, "draft"),
        (15, 2, "submitted"),
        (20, 3, "submitted"),
        (26, 0, "under_review"),
        (31, 3, "under_review"),
        (36, 1, "approved"),
        (41, 2, "approved"),
        (46, 3, "rejected"),
        (50, 0, "withdrawn"),
    ];
    for (p, e, status) in plan {
        let (code, person) = &people[p % people.len()];
        let officer = &tokens[officer_of(code)?.as_str()];
        let (equipment_id, rent) = &equipment[e];
        let r = api
            .post(
                &farm("/pue-requests"),
                json!({ "person_id": person, "equipment_id": equipment_id, "quantity": 1,
                        "acquisition": if *rent && p % 2 == 0 { "rent" } else { "buy" },
                        "purpose": "DEMO: shared by the household group",
                        "status": if status == "draft" { "draft" } else { "submitted" } }),
                officer,
            )
            .await?;
        let tail = farm(&format!("/pue-requests/{}/transitions", text(&r, "id")?));
        let steps: &[(&str, Option<&str>)] = match status {
            "under_review" => &[("start_review", None)],
            "approved" => &[
                ("start_review", None),
                ("approve", Some("DEMO: fits the village capacity")),
            ],
            "rejected" => &[
                ("start_review", None),
                ("reject", Some("DEMO: the village has no headroom yet")),
            ],
            "withdrawn" => &[("withdraw", None)],
            _ => &[],
        };
        for (action, note) in steps {
            let token = if *action == "withdraw" {
                officer.as_str()
            } else {
                ops
            };
            api.post(&tail, json!({ "action": action, "note": note }), token)
                .await?;
        }
    }
    Ok(())
}

/// Buyers, demand for beans, coffee, sunflower and maize, and opportunities in every status.
async fn market(
    api: &Api,
    ws: &str,
    ops: &str,
    tzs: &str,
    crops: &HashMap<&str, String>,
    expected: &HashMap<(&str, &str), Vec<(String, i64)>>,
    village: &dyn Fn(&str) -> String,
) -> Result<(), AppError> {
    let farm = |tail: &str| format!("/workspace/{ws}/farm{tail}");
    let mut buyers = Vec::new();
    for (name, channel, note) in names::BUYERS {
        let b = api
            .post(
                &farm("/buyers"),
                json!({ "name": name, "channel": channel, "contact_note": note }),
                ops,
            )
            .await?;
        buyers.push(text(&b, "id")?);
    }
    let plan: &[Demand] = &[
        (0, "BEANS", "2500", 60, 120, 180_000, "KITOWO", "shared"),
        (1, "COFFEE", "4000", 200, 330, 450_000, "LUGODA", "accepted"),
        (2, "SUNFLOWER", "1500", 60, 120, 90_000, "MGAMA", "proposed"),
        (0, "MAIZE", "8000", 60, 120, 75_000, "MWANGEZA", "declined"),
        (2, "SUNFLOWER", "900", 70, 110, 95_000, "KITOWO", "lapsed"),
    ];
    for (buyer, crop, kg, from, to, price, code, status) in plan {
        let d = api
            .post(
                &farm("/buyer-demands"),
                json!({ "buyer_id": buyers[*buyer], "crop_id": crops[crop], "quantity_kg": kg,
                        "window_start": days_ago(-from), "window_end": days_ago(-to),
                        "indicative_price_per_kg_minor": price, "currency_id": tzs }),
                ops,
            )
            .await?;
        let o = api
            .post(
                &farm(&format!("/buyer-demands/{}/opportunities", text(&d, "id")?)),
                json!({ "village_id": village(code), "note": "DEMO" }),
                ops,
            )
            .await?;
        let o = text(&o, "id")?;
        if let Some((harvest, kg)) = expected.get(&(*code, *crop)).and_then(|h| h.first())
            && matches!(*status, "shared" | "accepted")
        {
            api.post(
                &farm(&format!("/opportunities/{o}/supply")),
                json!({ "harvest_report_id": harvest, "contributed_kg": (kg / 2).to_string() }),
                ops,
            )
            .await?;
        }
        let path: &[&str] = match *status {
            "shared" => &["shared"],
            "accepted" => &["shared", "accepted"],
            "declined" => &["declined"],
            "lapsed" => &["shared", "lapsed"],
            _ => &[],
        };
        for s in path {
            api.post(
                &farm(&format!("/opportunities/{o}/status")),
                json!({ "status": s }),
                ops,
            )
            .await?;
        }
    }
    Ok(())
}

/// Pay out farmers the sales credited (Hamisi neither registered nor verified them), void one
/// payout, and record his cash float and count.
pub async fn payouts(api: &Api, r: &Ruaha) -> Result<(), AppError> {
    let farm = |tail: &str| format!("/workspace/{}/farm{tail}", r.ws);
    let credited: Vec<Uuid> = sqlx::query_scalar!(
        "SELECT DISTINCT person_id FROM farm_passbook_entries WHERE workspace_id = $1 ORDER BY person_id",
        r.ws_id
    )
    .fetch_all(&r.db)
    .await?;
    api.post(
        &farm("/cash/movements"),
        json!({ "kind": "float_out", "staff_user_id": r.hamisi_id, "village_id": r.ilundo,
                "currency_id": r.tzs, "amount": 100_000_000 }),
        &r.ops,
    )
    .await?;
    let mut paid = Vec::new();
    for person in credited.iter().take(5) {
        let book = api
            .call(
                Method::GET,
                &farm(&format!("/persons/{person}/passbook")),
                None,
                Some(&r.ops),
            )
            .await?;
        let balance = book["data"][0]["balance_after"]["minor"]
            .as_i64()
            .unwrap_or(0);
        if balance <= 0 {
            continue;
        }
        let p = api
            .post(&farm("/payouts"),
                json!({ "client_ref": Uuid::now_v7(), "person_id": person,
                        "amount": (balance / 100 * 100).clamp(100, 10_000_000), "currency_id": r.tzs,
                        "id_type": "nida", "name_confirmed": true }), &r.hamisi)
            .await?;
        paid.push(text(&p, "id")?);
    }
    if let Some(p) = paid.last() {
        api.post(
            &farm(&format!("/payouts/{p}/void")),
            json!({ "reason": "DEMO: paid to the wrong household member, paid again next visit" }),
            &r.ops,
        )
        .await?;
    }
    api.post(
        &farm("/cash/movements"),
        json!({ "kind": "count", "staff_user_id": r.hamisi_id, "village_id": r.ilundo,
                "currency_id": r.tzs, "amount": 60_000_000 }),
        &r.hamisi,
    )
    .await?;
    Ok(())
}
