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
