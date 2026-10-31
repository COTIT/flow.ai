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
        let app = app(pool.clone());
        let (owner, ws) = signup(&app, "owner@ruaha.test", "ruaha").await;
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
        let owner_id = farm.user_id("owner@ruaha.test").await;
        farm.grant(&farm.owner, &owner_id, "admin", None).await;
        farm.ilundo = farm.village("ILUNDO", "Ilundo").await;
        farm.mgama = farm.village("MGAMA", "Mgama").await;
        farm.ops = farm.staff("ops@ruaha.test", "ops", None).await;
        let ilu = farm.ilundo.clone();
        let mga = farm.mgama.clone();
        farm.officer_ilu = farm
            .staff("salima@ruaha.test", "field_officer", Some(&ilu))
            .await;
        farm.officer_mga = farm
            .staff("peter@ruaha.test", "field_officer", Some(&mga))
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

    /// Owner creates a village; returns its id.
    pub async fn village(&self, code: &str, name: &str) -> String {
        let (status, res) = self
            .call(
                Method::POST,
                "/villages",
                Some(
                    json!({ "code": code, "name": name, "latitude": "-7.7", "longitude": "35.7" }),
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
        let own = format!("ws-{}", &email[..email.find('@').unwrap()]);
        let (token, _) = signup(&self.app, email, &own).await;
        join_workspace(&self.pool, &self.ws, email).await;
        token
    }

    /// New user with only a farmer membership in `village` (not a workspace member).
    /// Direct SQL: farmer roles come from login issue, not the grant route.
    pub async fn farmer(&self, email: &str, village: &str) -> String {
        let own = format!("ws-{}", &email[..email.find('@').unwrap()]);
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
