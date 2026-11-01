//! Farm fixture: the RUAHA demo cast in one workspace. Owner (workspace admin, farm admin),
//! ops, one field officer per village (Ilundo, Mgama). Built through the API, so fixtures
//! exercise the same rules the tests check.

use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;

use super::{app, join_workspace, send, signup};

pub struct Farm {
    pub app: Router,
    pub pool: PgPool,
    pub ws: String,
    /// Workspace owner, also farm admin.
    pub owner: String,
    pub ops: String,
    pub officer_ilu: String,
    pub officer_mga: String,
    pub ilundo: String,
    pub mgama: String,
}

impl Farm {
    pub async fn new(pool: PgPool) -> Self {
        Self::new_named(pool, "ruaha.test", "ruaha").await
    }

    /// The same cast in a workspace named `name`, staff emails at `domain` (two farms in one DB).
    pub async fn new_named(pool: PgPool, domain: &str, name: &str) -> Self {
        let app = app(pool.clone());
        let (owner, ws) = signup(&app, &format!("owner@{domain}"), name).await;
        let mut farm = Self {
            app,
            pool,
            ws,
            owner,
            ops: String::new(),
            officer_ilu: String::new(),
            officer_mga: String::new(),
            ilundo: String::new(),
            mgama: String::new(),
        };
        let owner_id = farm.user_id(&format!("owner@{domain}")).await;
        farm.grant(&farm.owner, &owner_id, "admin", None).await;
        farm.ilundo = farm.village("ILUNDO", "Ilundo").await;
        farm.mgama = farm.village("MGAMA", "Mgama").await;
        farm.ops = farm.staff(&format!("ops@{domain}"), "ops", None).await;
        let ilu = farm.ilundo.clone();
        let mga = farm.mgama.clone();
        farm.officer_ilu = farm
            .staff(&format!("salima@{domain}"), "field_officer", Some(&ilu))
            .await;
        farm.officer_mga = farm
            .staff(&format!("peter@{domain}"), "field_officer", Some(&mga))
            .await;
        farm
    }

    /// `/v1/workspace/{ws}/farm{tail}`.
    pub fn url(&self, tail: &str) -> String {
        format!("/v1/workspace/{}/farm{tail}", self.ws)
    }

    pub async fn call(
        &self,
        method: Method,
        tail: &str,
        body: Option<Value>,
        token: &str,
    ) -> (StatusCode, Value) {
        send(&self.app, method, &self.url(tail), body, Some(token)).await
    }

    pub async fn user_id(&self, email: &str) -> String {
        sqlx::query_scalar("SELECT id::text FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    /// Owner creates a village with RUAHA's demo capacity (Ilundo 500 kW × 0.600, others
    /// 150 kW × 0.650); returns its id.
    pub async fn village(&self, code: &str, name: &str) -> String {
        let (kw, factor) = if code == "ILUNDO" {
            ("500", "0.600")
        } else {
            ("150", "0.650")
        };
        let (status, res) = self
            .call(
                Method::POST,
                "/villages",
                Some(
                    json!({ "code": code, "name": name, "latitude": "-7.7", "longitude": "35.7",
                    "capacity": { "capacity_kw": kw, "simultaneity_factor": factor } }),
                ),
                &self.owner,
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["id"].as_str().unwrap().to_owned()
    }

    /// `as_token` grants `role`; asserts 200/201 and returns the membership.
    pub async fn grant(
        &self,
        as_token: &str,
        user_id: &str,
        role: &str,
        village: Option<&str>,
    ) -> Value {
        let (status, res) = self
            .call(
                Method::POST,
                "/memberships",
                Some(json!({ "user_id": user_id, "role": role, "village_id": village })),
                as_token,
            )
            .await;
        assert!(status.is_success(), "{status} {res}");
        res["data"].clone()
    }

    /// New user (own signup workspace) joins this workspace and gets a farm staff role.
    pub async fn staff(&self, email: &str, role: &str, village: Option<&str>) -> String {
        let token = self.member(email).await;
        let id = self.user_id(email).await;
        self.grant(&self.owner, &id, role, village).await;
        token
    }

    /// New user who is an active member of this workspace, no farm role.
    pub async fn member(&self, email: &str) -> String {
        let own = format!("ws-{}", email.replace(['@', '.'], "-"));
        let (token, _) = signup(&self.app, email, &own).await;
        join_workspace(&self.pool, &self.ws, email).await;
        token
    }

    /// New user with only a farmer membership in `village` (not a workspace member).
    /// Direct SQL: farmer roles come from login issue, not the grant route.
    pub async fn farmer(&self, email: &str, village: &str) -> String {
        let own = format!("ws-{}", email.replace(['@', '.'], "-"));
        let (token, _) = signup(&self.app, email, &own).await;
        sqlx::query(
            "INSERT INTO farm_memberships (workspace_id, user_id, role, village_id)
             SELECT $1::uuid, id, 'farmer', $2::uuid FROM users WHERE email = $3",
        )
        .bind(&self.ws)
        .bind(village)
        .bind(email)
        .execute(&self.pool)
        .await
        .unwrap();
        token
    }
}

/// Ids from one registration.
pub struct Reg {
    pub person: String,
    pub household: String,
    pub farm: String,
    pub plot: String,
    pub cycles: Vec<String>,
    pub harvests: Vec<String>,
}

impl Reg {
    fn from(res: &Value) -> Self {
        let s = |k: &str| res[k].as_str().unwrap().to_owned();
        let list = |k: &str| {
            res[k]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect()
        };
        Self {
            person: s("person_id"),
            household: s("household_id"),
            farm: s("farm_id"),
            plot: s("plot_id"),
            cycles: list("crop_cycle_ids"),
            harvests: list("harvest_report_ids"),
        }
    }
}

/// The registered cast: Ilundo (Neema + Rehema share a household and farm, Asha on her
/// own), Mgama (Joseph). Crops: maize by area, banana by tree count, poultry by unit count.
pub struct Cast {
    pub maize: String,
    pub banana: String,
    pub poultry: String,
    pub neema: Reg,
    pub rehema: Reg,
    pub asha: Reg,
    pub joseph: Reg,
}

/// A fresh client_ref.
pub fn client_ref() -> String {
    uuid::Uuid::now_v7().to_string()
}

impl Farm {
    /// ops adds a crop; returns its id.
    pub async fn crop(&self, code: &str, measured_by: &str) -> String {
        let body =
            json!({ "code": code, "name_en": code, "name_sw": code, "measured_by": measured_by });
        let (status, res) = self
            .call(Method::POST, "/crops", Some(body), &self.ops)
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["id"].as_str().unwrap().to_owned()
    }

    /// POST /registrations as `token`.
    pub async fn register(&self, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, "/registrations", Some(body), token)
            .await
    }

    async fn registered(&self, token: &str, body: Value) -> Reg {
        let (status, res) = self.register(token, body).await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        Reg::from(&res["data"])
    }

    /// Register the cast through the API.
    pub async fn cast(&self) -> Cast {
        let maize = self.crop("MAIZE", "area").await;
        let banana = self.crop("BANANA", "tree_count").await;
        let poultry = self.crop("POULTRY", "unit_count").await;
        let neema = self
            .registered(
                &self.officer_ilu,
                json!({
                    "client_ref": client_ref(), "village_id": self.ilundo,
                    "person": { "given_name": "Neema", "family_name": "Mwakalinga", "phone": "0712 000 001" },
                    "household": { "label": "Mwakalinga household" },
                    "farm": { "label": "Mwakalinga farm", "latitude": "-7.71", "longitude": "35.71" },
                    "plot": { "label": "Riverside", "area_ha": "1.8" },
                    "cycles": [
                        { "crop_id": maize, "area_ha": "1.6", "harvest_start": "2026-09-01", "harvest_end": "2026-09-30",
                          "harvest": { "quantity_kg": "3200", "confidence": "medium", "reported_for": "2026-09-15" } },
                        { "crop_id": banana, "tree_count": 40 }
                    ]
                }),
            )
            .await;
        let rehema = self
            .registered(
                &self.officer_ilu,
                json!({
                    "client_ref": client_ref(), "village_id": self.ilundo,
                    "person": { "given_name": "Rehema", "family_name": "Mwakalinga", "phone": "+255712000002" },
                    "household": { "existing_household_id": neema.household },
                    "farm": { "existing_farm_id": neema.farm },
                    "plot": { "label": "Hill", "area_ha": "0.5" }
                }),
            )
            .await;
        let asha = self
            .registered(
                &self.officer_ilu,
                json!({
                    "client_ref": client_ref(), "village_id": self.ilundo,
                    "person": { "given_name": "Asha", "family_name": "Kimaro", "phone": "255712000003" },
                    "household": { "label": "Kimaro household" },
                    "farm": { "label": "Kimaro farm", "latitude": "-7.72", "longitude": "35.72" },
                    "plot": { "label": "Kimaro plot", "area_ha": "2.0" },
                    "cycles": [{ "crop_id": poultry, "unit_count": 50 }]
                }),
            )
            .await;
        let joseph = self
            .registered(
                &self.officer_mga,
                json!({
                    "client_ref": client_ref(), "village_id": self.mgama,
                    "person": { "given_name": "Joseph", "family_name": "Mlowe", "phone": "0713000001" },
                    "household": { "label": "Mlowe household" },
                    "farm": { "label": "Mlowe farm", "latitude": "-7.90", "longitude": "35.90" },
                    "plot": { "label": "Valley", "area_ha": "1.0" },
                    "cycles": [{ "crop_id": maize, "area_ha": "1.0",
                                 "harvest": { "quantity_kg": "1500" } }]
                }),
            )
            .await;
        Cast {
            maize,
            banana,
            poultry,
            neema,
            rehema,
            asha,
            joseph,
        }
    }

    /// Link an account to a registered person (login issue does this in step 3).
    pub async fn link(&self, email: &str, person: &str) {
        sqlx::query(
            "UPDATE farm_persons SET user_id = (SELECT id FROM users WHERE email = $1)
             WHERE id = $2::uuid",
        )
        .bind(email)
        .bind(person)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// A farmer account linked to `person` in `village`.
    pub async fn farmer_for(&self, email: &str, village: &str, person: &str) -> String {
        let token = self.farmer(email, village).await;
        self.link(email, person).await;
        token
    }

    /// GET a list under /farm and return it.
    pub async fn list(&self, tail: &str, token: &str) -> Value {
        let (status, res) = self.call(Method::GET, tail, None, token).await;
        assert_eq!(status, StatusCode::OK, "{tail}: {res}");
        res
    }
}

/// A cast ready for surveys: households verified by ops (a second pair of eyes), Neema and
/// Asha signed in with their own passwords, a TZS currency, a third Ilundo officer (Juma).
pub struct Surveyed {
    pub c: Cast,
    pub tzs: String,
    pub neema: String,
    pub asha: String,
    pub juma: String,
}

impl Farm {
    /// TZS (2 decimals) in this workspace; its id.
    pub async fn tzs(&self) -> String {
        sqlx::query_scalar(
            "INSERT INTO currencies (workspace_id, name, asset_type, decimals, active)
             VALUES ($1::uuid, 'Tanzanian shilling', 'TZS', 2, true) RETURNING id::text",
        )
        .bind(&self.ws)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    /// Issue a login for `person`, sign in, choose a password: the farmer's token.
    pub async fn signed_in_farmer(&self, person: &str, phone: &str) -> String {
        let (status, issued) = self
            .call(
                Method::POST,
                &format!("/persons/{person}/login"),
                None,
                &self.officer_ilu,
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{issued}");
        let temp = issued["data"]["temp_password"].as_str().unwrap().to_owned();
        let token = self.farm_sign_in(phone, &temp).await;
        let (status, res) = super::send(
            &self.app,
            Method::POST,
            "/v1/farm/password",
            Some(json!({ "current_password": temp, "password": "maji safi kila siku" })),
            Some(&token),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{res}");
        token
    }

    /// POST /v1/farm/login; the token.
    pub async fn farm_sign_in(&self, phone: &str, password: &str) -> String {
        let (status, res) = super::send(
            &self.app,
            Method::POST,
            "/v1/farm/login",
            Some(json!({ "phone": phone, "password": password })),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        res["data"]["access_token"].as_str().unwrap().to_owned()
    }

    /// `token` verifies a record; asserts 200.
    pub async fn verify(&self, kind: &str, id: &str, token: &str) {
        let (status, res) = self
            .call(
                Method::POST,
                &format!("/{kind}/{id}/verification"),
                None,
                token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{res}");
    }

    pub async fn surveyed(&self) -> Surveyed {
        let c = self.cast().await;
        for h in [&c.neema.household, &c.asha.household] {
            self.verify("households", h, &self.ops).await;
        }
        let neema = self.signed_in_farmer(&c.neema.person, "0712000001").await;
        let asha = self.signed_in_farmer(&c.asha.person, "0712000003").await;
        let juma = self
            .staff("juma@ruaha.test", "field_officer", Some(&self.ilundo))
            .await;
        let tzs = self.tzs().await;
        Surveyed {
            c,
            tzs,
            neema,
            asha,
            juma,
        }
    }

    /// Admin authors a survey with one required single-choice and one optional number
    /// question, `reward` minor units, `audit_percent` held for audit; published unless
    /// `draft`. Returns (survey id, [question ids]).
    pub async fn survey(
        &self,
        tzs: &str,
        reward: i64,
        audit_percent: i64,
        draft: bool,
    ) -> (String, Vec<String>) {
        let (status, s) = self
            .call(
                Method::POST,
                "/surveys",
                Some(json!({ "title_en": "Maize storage", "title_sw": "Hifadhi ya mahindi",
                             "reward_minor": reward, "currency_id": tzs, "audit_percent": audit_percent })),
                &self.owner,
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{s}");
        let id = s["data"]["id"].as_str().unwrap().to_owned();
        let mut questions = Vec::new();
        for q in [
            json!({ "position": 1, "kind": "single_choice", "prompt_en": "Where do you store maize?", "required": true,
                    "options": [{ "value": "home", "label_en": "At home" }, { "value": "store", "label_en": "In a store" }] }),
            json!({ "position": 2, "kind": "number", "prompt_en": "How many bags?", "required": false }),
        ] {
            let (status, res) = self
                .call(
                    Method::POST,
                    &format!("/surveys/{id}/questions"),
                    Some(q),
                    &self.owner,
                )
                .await;
            assert_eq!(status, StatusCode::CREATED, "{res}");
            questions.push(res["data"]["id"].as_str().unwrap().to_owned());
        }
        if !draft {
            let (status, res) = self
                .call(
                    Method::POST,
                    &format!("/surveys/{id}/publish"),
                    None,
                    &self.owner,
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{res}");
        }
        (id, questions)
    }

    /// `token` answers survey `id`: (status, body).
    pub async fn answer(
        &self,
        id: &str,
        answers: Value,
        client_ref: &str,
        token: &str,
    ) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            &format!("/surveys/{id}/responses"),
            Some(json!({ "answers": answers, "client_ref": client_ref })),
            token,
        )
        .await
    }
}

impl Farm {
    /// Id of the account signing in with this phone.
    pub async fn user_id_by_phone(&self, phone: &str) -> String {
        sqlx::query_scalar("SELECT id::text FROM users WHERE phone = $1")
            .bind(phone)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

/// Supply-chain helpers (Phase 4b): products, warehouses, harvests, batches, lot stock.
impl Farm {
    /// `/v1/workspace/{ws}{tail}` of this farm's workspace as `token`.
    pub async fn ws_call(
        &self,
        method: Method,
        tail: &str,
        body: Option<Value>,
        token: &str,
    ) -> (StatusCode, Value) {
        let url = format!("/v1/workspace/{}{tail}", self.ws);
        send(&self.app, method, &url, body, Some(token)).await
    }

    /// Owner adds a stock-tracked product; its id.
    pub async fn product(&self, name: &str) -> String {
        let body = json!({ "name": name, "price": 10 });
        let (status, res) = self
            .ws_call(Method::POST, "/products", Some(body), &self.owner)
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["product_id"].as_str().unwrap().to_owned()
    }

    /// Owner adds a warehouse; its id.
    pub async fn warehouse(&self, name: &str) -> String {
        let (status, res) = self
            .ws_call(
                Method::POST,
                "/warehouses",
                Some(json!({ "name": name })),
                &self.owner,
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["warehouse_id"].as_str().unwrap().to_owned()
    }

    /// An Ilundo farmer with one maize-like cycle of `crop`: (person, cycle).
    pub async fn grower(&self, name: &str, phone: &str, crop: &str) -> (String, String) {
        let (status, res) = self
            .register(
                &self.officer_ilu,
                json!({
                    "client_ref": client_ref(), "village_id": self.ilundo,
                    "person": { "given_name": name, "family_name": "Grower", "phone": phone },
                    "household": { "label": format!("{name} household") },
                    "farm": { "label": format!("{name} farm"), "latitude": "-7.75", "longitude": "35.75" },
                    "plot": { "label": "plot", "area_ha": "1.0" },
                    "cycles": [{ "crop_id": crop, "area_ha": "1.0",
                                 "harvest_start": "2026-09-01", "harvest_end": "2026-09-30" }]
                }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        let d = &res["data"];
        (
            d["person_id"].as_str().unwrap().to_owned(),
            d["crop_cycle_ids"][0].as_str().unwrap().to_owned(),
        )
    }

    /// The Ilundo officer reports an actual figure on `cycle`; the owner verifies it. Its id.
    pub async fn actual(&self, cycle: &str, kg: &str) -> String {
        let (status, res) = self
            .call(
                Method::POST,
                &format!("/crop-cycles/{cycle}/harvest-reports"),
                Some(json!({ "kind": "actual", "quantity_kg": kg, "source": "field_verified" })),
                &self.officer_ilu,
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        let id = res["data"]["id"].as_str().unwrap().to_owned();
        self.verify("harvest-reports", &id, &self.owner).await;
        id
    }

    /// ops buys `lines` (harvest, kg) of `crop` in Ilundo at `price_per_kg` into a raw lot of
    /// `product`; the batch.
    pub async fn batch(
        &self,
        code: &str,
        crop: &str,
        product: &str,
        currency: &str,
        price_per_kg: i64,
        lines: &[(&str, &str)],
    ) -> Value {
        let lines: Vec<Value> = lines
            .iter()
            .map(|(h, kg)| json!({ "harvest_report_id": h, "kg": kg }))
            .collect();
        let body = json!({
            "client_ref": client_ref(), "village_id": self.ilundo, "crop_id": crop, "code": code,
            "raw_product_id": product, "price_per_kg": price_per_kg, "currency_id": currency,
            "lines": lines
        });
        let (status, res) = self
            .call(Method::POST, "/supply-batches", Some(body), &self.ops)
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"].clone()
    }

    /// Owner puts `qty` of `lot` into `warehouse` (Phase 3 lot stock-in).
    pub async fn lot_stock(&self, lot: &str, warehouse: &str, qty: &str) {
        let (status, res) = self
            .ws_call(
                Method::POST,
                &format!("/lots/{lot}/stock-mutations"),
                Some(json!({ "warehouse_id": warehouse, "quantity": qty, "reason": "received" })),
                &self.owner,
            )
            .await;
        assert!(status.is_success(), "{status} {res}");
    }
}
