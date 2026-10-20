//! /v1/workspace/{ws}/users/teams: CRUD, members, permission delta (vimdesk-web team.service.js).

mod common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{app_with_mail, member_with, send, signup};
use serde_json::{Value, json};
use sqlx::PgPool;

const NOT_FOUND: &str = "The requested resource could not be found.";
const UNAUTHORIZED: &str = "This action is unauthorized.";

struct World {
    app: Router,
    pool: PgPool,
    owner: String,
    ws: String,
}

/// Ada owns `acme`.
async fn world(pool: PgPool) -> World {
    let (app, _) = app_with_mail(pool.clone());
    let (owner, ws) = signup(&app, "ada@example.com", "acme").await;
    World {
        app,
        pool,
        owner,
        ws,
    }
}

impl World {
    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: &str,
    ) -> (StatusCode, Value) {
        let uri = format!("/v1/workspace/{}/users/teams{path}", self.ws);
        send(&self.app, method, &uri, body, Some(token)).await
    }

    async fn create(&self, name: &str) -> String {
        let (status, res) = self
            .call(
                Method::POST,
                "",
                Some(json!({ "name": name, "description": "d" })),
                &self.owner,
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{res}");
        res["data"]["team_id"].as_str().unwrap().to_owned()
    }

    async fn get(&self, team: &str) -> Value {
        let (status, res) = self
            .call(Method::GET, &format!("/{team}"), None, &self.owner)
            .await;
        assert_eq!(status, StatusCode::OK, "{res}");
        res["data"].clone()
    }

    async fn user_id(&self, email: &str) -> String {
        sqlx::query_scalar("SELECT id::text FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn grant(&self, team: &str, perms: Value, token: &str) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            &format!("/{team}/permissions"),
            Some(json!({ "permissions": perms })),
            token,
        )
        .await
    }
}

// ================= create / get / list

#[sqlx::test]
async fn create_then_get_in_team_resource_shape(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w
        .call(
            Method::POST,
            "",
            Some(json!({
                "name": " Marketing ", "description": "Promo people",
                "permissions": ["relation.get", "Relation.Create", "nope.list", "workspace_admin", 7],
                "meta": { "colour": "red" },
            })),
            &w.owner,
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    let id = res["data"]["team_id"].as_str().unwrap();
    let team = w.get(id).await;
    assert_eq!(team["id"], id);
    assert_eq!(team["name"], "Marketing");
    assert_eq!(team["description"], "Promo people");
    // Catalog names lowercased; unknown dropped; owner may grant workspace_admin.
    assert_eq!(
        team["permissions"],
        json!(["relation.create", "relation.get", "workspace_admin"])
    );
    assert_eq!(team["meta"], json!({ "colour": "red" }));
    assert!(team["created_at"].is_string() && team["last_updated_at"].is_string());
    assert_eq!(team["relationships"], json!({ "users": [] }));
}

#[sqlx::test]
async fn create_validates_like_laravel(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w.call(Method::POST, "", Some(json!({})), &w.owner).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["name"],
        json!(["The name field is required."])
    );
    assert_eq!(
        res["errors"]["description"],
        json!(["The description field is required."])
    );

    let (_, res) = w
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "n".repeat(56), "description": "d".repeat(226), "permissions": "x" })),
            &w.owner,
        )
        .await;
    assert_eq!(
        res["errors"]["name"],
        json!(["The name field must not be greater than 55 characters."])
    );
    assert_eq!(
        res["errors"]["description"],
        json!(["The description field must not be greater than 225 characters."])
    );
    assert_eq!(
        res["errors"]["permissions"],
        json!(["The permissions field must be an array."])
    );
}

#[sqlx::test]
async fn list_has_admin_team_and_members_with_user_shape(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w
        .call(Method::GET, "?tab=teams&team_id=x", None, &w.owner)
        .await;

    assert_eq!(status, StatusCode::OK, "{res}");
    let list = res["data"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "Admin Team");
    let user = &list[0]["relationships"]["users"][0];
    assert_eq!(user["id"], w.user_id("ada@example.com").await);
    assert_eq!(user["name"], "Ada Lovelace");
    assert_eq!(user["initials"], "AL");
    assert_eq!(user["email"], "ada@example.com");
    assert!(
        user["avatar_path"]
            .as_str()
            .unwrap()
            .ends_with("?name=Ada+Lovelace")
    );
    assert_eq!(user["color"], Value::Null);
    assert_eq!(res["meta"]["total"], 1);
    assert_eq!(res["meta"]["current_page"], 1);
}

#[sqlx::test]
async fn list_search_sort_and_paging(pool: PgPool) {
    let w = world(pool).await;
    for name in ["Beta", "alpha", "Gamma_1", "Gamma%2"] {
        w.create(name).await;
    }
    let names = |res: &Value| -> Vec<String> {
        res["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    };

    let (_, res) = w.call(Method::GET, "?q=ALP", None, &w.owner).await;
    assert_eq!(names(&res), ["alpha"]);
    // Wildcards in q match literally.
    let (_, res) = w.call(Method::GET, "?q=a_1", None, &w.owner).await;
    assert_eq!(names(&res), ["Gamma_1"]);
    let (_, res) = w.call(Method::GET, "?q=%25", None, &w.owner).await;
    assert_eq!(names(&res), ["Gamma%2"]);

    // Case-insensitive; the two Gammas' relative order depends on DB collation.
    let (_, res) = w.call(Method::GET, "?sort=-name", None, &w.owner).await;
    let sorted = names(&res);
    assert!(
        sorted[..2].iter().all(|n| n.starts_with("Gamma")),
        "{sorted:?}"
    );
    assert_eq!(sorted[2..], ["Beta", "alpha", "Admin Team"]);
    // Unknown sort column ignored, default oldest first.
    let (_, res) = w
        .call(Method::GET, "?sort=password_hash", None, &w.owner)
        .await;
    assert_eq!(names(&res)[0], "Admin Team");

    let (_, res) = w
        .call(Method::GET, "?page=2&page_size=2", None, &w.owner)
        .await;
    assert_eq!(names(&res), ["alpha", "Gamma_1"]);
    assert_eq!(res["meta"]["last_page"], 3);
    assert_eq!(res["meta"]["total"], 5);

    let (status, res) = w
        .call(
            Method::GET,
            &format!("?q={}", "x".repeat(101)),
            None,
            &w.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["q"],
        json!(["The q field must not be greater than 100 characters."])
    );
}

#[sqlx::test]
async fn deleted_and_foreign_teams_are_hidden(pool: PgPool) {
    let w = world(pool).await;
    let gone = w.create("Gone").await;
    w.call(Method::DELETE, &format!("/{gone}"), None, &w.owner)
        .await;
    let (_, eves) = signup(&w.app, "eve@example.com", "eves").await;
    let foreign = common::add_team(&w.pool, &eves, "Theirs", &[], false).await;
    let ada = w.user_id("ada@example.com").await;

    let (_, res) = w.call(Method::GET, "", None, &w.owner).await;
    assert_eq!(res["data"].as_array().unwrap().len(), 1, "only Admin Team");

    for team in [gone.as_str(), foreign.as_str(), "not-a-uuid"] {
        for (method, path, body) in [
            (Method::GET, format!("/{team}"), None),
            (
                Method::POST,
                format!("/{team}"),
                Some(json!({ "name": "x" })),
            ),
            (Method::DELETE, format!("/{team}"), None),
            (
                Method::POST,
                format!("/{team}/add-users"),
                Some(json!({ "user_ids": [ada] })),
            ),
            (
                Method::POST,
                format!("/{team}/permissions"),
                Some(json!({ "permissions": [] })),
            ),
        ] {
            let (status, res) = w.call(method.clone(), &path, body, &w.owner).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}: {res}");
            assert_eq!(res["message"], NOT_FOUND);
        }
    }
}

// ================= update / delete

#[sqlx::test]
async fn update_changes_only_sent_fields(pool: PgPool) {
    let w = world(pool).await;
    let team = w.create("Sales").await;

    let (status, res) = w
        .call(
            Method::POST,
            &format!("/{team}"),
            Some(json!({ "description": "New desc" })),
            &w.owner,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res, json!({ "message": "ok" }));
    let t = w.get(&team).await;
    assert_eq!(
        (t["name"].as_str(), t["description"].as_str()),
        (Some("Sales"), Some("New desc"))
    );

    let (status, res) = w
        .call(
            Method::POST,
            &format!("/{team}"),
            Some(json!({ "name": null, "description": "" })),
            &w.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["name"],
        json!(["The name field is required."])
    );
    assert_eq!(
        res["errors"]["description"],
        json!(["The description field is required."])
    );
}

#[sqlx::test]
async fn delete_soft_deletes_and_drops_its_grants_from_members(pool: PgPool) {
    let w = world(pool).await;
    let bob = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &["team.list"]).await;
    let crew: String = sqlx::query_scalar("SELECT id::text FROM teams WHERE name = 'Crew'")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(w.call(Method::GET, "", None, &bob).await.0, StatusCode::OK);

    let (status, res) = w
        .call(Method::DELETE, &format!("/{crew}"), None, &w.owner)
        .await;

    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    let deleted: bool =
        sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM teams WHERE id = $1::uuid")
            .bind(&crew)
            .fetch_one(&w.pool)
            .await
            .unwrap();
    assert!(deleted, "row kept, soft deleted");
    assert_eq!(
        w.call(Method::GET, "", None, &bob).await.0,
        StatusCode::FORBIDDEN
    );
}

// ================= members

#[sqlx::test]
async fn add_and_remove_users(pool: PgPool) {
    let w = world(pool).await;
    let team = w.create("Sales").await;
    member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;
    let bob = w.user_id("bob@example.com").await;

    let path = format!("/{team}/add-users");
    let (status, res) = w
        .call(
            Method::POST,
            &path,
            Some(json!({ "user_ids": [bob, bob] })),
            &w.owner,
        )
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    // Adding again is a no-op.
    assert_eq!(
        w.call(
            Method::POST,
            &path,
            Some(json!({ "user_ids": [bob] })),
            &w.owner
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(w.get(&team).await["relationships"]["users"][0]["id"], bob);

    let (status, _) = w
        .call(
            Method::POST,
            &format!("/{team}/remove-users"),
            Some(json!({ "user_ids": [bob] })),
            &w.owner,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(w.get(&team).await["relationships"]["users"], json!([]));
}

#[sqlx::test]
async fn add_users_must_be_workspace_members(pool: PgPool) {
    let w = world(pool).await;
    let team = w.create("Sales").await;
    signup(&w.app, "eve@example.com", "eves").await;
    let eve = w.user_id("eve@example.com").await;

    for ids in [json!([eve]), json!(["not-a-uuid"]), json!([1]), json!("x")] {
        let (status, res) = w
            .call(
                Method::POST,
                &format!("/{team}/add-users"),
                Some(json!({ "user_ids": ids })),
                &w.owner,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{ids}");
        assert_eq!(
            res["errors"]["user_ids"],
            json!(["user ids must be an array of valid user_ids"]),
            "{ids}"
        );
    }
    for body in [json!({}), json!({ "user_ids": [] })] {
        let (_, res) = w
            .call(
                Method::POST,
                &format!("/{team}/add-users"),
                Some(body),
                &w.owner,
            )
            .await;
        assert_eq!(
            res["errors"]["user_ids"],
            json!(["The user ids field is required."])
        );
    }
    assert_eq!(w.get(&team).await["relationships"]["users"], json!([]));
}

#[sqlx::test]
async fn remove_users_must_already_be_in_team(pool: PgPool) {
    let w = world(pool).await;
    let team = w.create("Sales").await;
    let ada = w.user_id("ada@example.com").await;

    let (status, res) = w
        .call(
            Method::POST,
            &format!("/{team}/remove-users"),
            Some(json!({ "user_ids": [ada] })),
            &w.owner,
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["user_ids"],
        json!(["This user ids does not exist in the team"])
    );
}

// ================= permissions delta

#[sqlx::test]
async fn permissions_save_is_a_delta(pool: PgPool) {
    let w = world(pool).await;
    let team = w.create("Sales").await;

    let (status, res) = w
        .grant(&team, json!(["relation.get", "relation.create"]), &w.owner)
        .await;
    assert_eq!((status, res), (StatusCode::OK, json!({ "message": "ok" })));
    assert_eq!(
        w.get(&team).await["permissions"],
        json!(["relation.create", "relation.get"])
    );

    // vimdesk-web sends kept grants positive and unticked ones with `-`; junk ignored.
    w.grant(
        &team,
        json!([
            "relation.get",
            "-relation.create",
            "relation",
            "-relation",
            null,
            "",
            "-",
            5
        ]),
        &w.owner,
    )
    .await;
    assert_eq!(w.get(&team).await["permissions"], json!(["relation.get"]));

    // Add then remove in one call: removal wins (Laravel order).
    w.grant(&team, json!(["order.list", "-order.list"]), &w.owner)
        .await;
    assert_eq!(w.get(&team).await["permissions"], json!(["relation.get"]));

    let (status, res) = w
        .call(
            Method::POST,
            &format!("/{team}/permissions"),
            Some(json!({})),
            &w.owner,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res["errors"]["permissions"],
        json!(["The permissions field is required."])
    );
}

#[sqlx::test]
async fn only_owner_grants_workspace_admin(pool: PgPool) {
    let w = world(pool).await;
    let team = w.create("Sales").await;
    let bob = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "bob@example.com",
        &["team.permissions", "team.get"],
    )
    .await;

    let (status, _) = w
        .grant(&team, json!(["workspace_admin", "relation.list"]), &bob)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        w.get(&team).await["permissions"],
        json!(["relation.list"]),
        "silently dropped"
    );

    w.grant(&team, json!(["workspace_admin"]), &w.owner).await;
    assert_eq!(
        w.get(&team).await["permissions"],
        json!(["relation.list", "workspace_admin"])
    );
}

// ================= permission per route

#[sqlx::test]
async fn each_route_needs_its_permission(pool: PgPool) {
    let w = world(pool).await;
    let team = w.create("Sales").await;
    let none = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;
    let ada = w.user_id("ada@example.com").await;

    // Order matters: add-users puts Ada in the team that remove-users then takes her out of.
    let cases: [(Method, String, Option<Value>, &str); 8] = [
        (
            Method::POST,
            String::new(),
            Some(json!({ "name": "x", "description": "d" })),
            "team.create",
        ),
        (Method::GET, String::new(), None, "team.list"),
        (Method::GET, format!("/{team}"), None, "team.get"),
        (
            Method::POST,
            format!("/{team}"),
            Some(json!({ "name": "y" })),
            "team.update",
        ),
        (
            Method::POST,
            format!("/{team}/add-users"),
            Some(json!({ "user_ids": [ada] })),
            "team.invite",
        ),
        (
            Method::POST,
            format!("/{team}/remove-users"),
            Some(json!({ "user_ids": [ada] })),
            "team.invite",
        ),
        (
            Method::POST,
            format!("/{team}/permissions"),
            Some(json!({ "permissions": [] })),
            "team.permissions",
        ),
        (Method::DELETE, format!("/{team}"), None, "team.delete"),
    ];
    for (i, (method, path, body, perm)) in cases.into_iter().enumerate() {
        let (status, res) = w.call(method.clone(), &path, body.clone(), &none).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{method} {path} without {perm}"
        );
        assert_eq!(res["message"], UNAUTHORIZED);

        let email = format!("m{i}@example.com");
        let token = member_with(&w.app, &w.pool, &w.ws, &email, &[perm]).await;
        let (status, res) = w.call(method.clone(), &path, body, &token).await;
        assert!(
            status.is_success(),
            "{method} {path} with {perm}: {status} {res}"
        );
    }
}

#[sqlx::test]
async fn outsider_gets_404(pool: PgPool) {
    let w = world(pool).await;
    let (eve, _) = signup(&w.app, "eve@example.com", "eves").await;

    let (status, _) = w.call(Method::GET, "", None, &eve).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn missing_permission_is_403_before_body_validation(pool: PgPool) {
    let w = world(pool).await;
    let team = w.create("Sales").await;
    let none = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &[]).await;

    // Laravel FormRequest authorize() runs before rules().
    for (method, path) in [
        (Method::POST, String::new()),
        (Method::POST, format!("/{team}")),
        (Method::POST, format!("/{team}/add-users")),
        (Method::POST, format!("/{team}/permissions")),
    ] {
        let (status, _) = w
            .call(method, &path, Some(json!({ "name": 1 })), &none)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
}

// ================= Admin Team gate: only owner / workspace_admin touch a team holding workspace_admin

async fn admin_team(w: &World) -> String {
    sqlx::query_scalar("SELECT id::text FROM teams WHERE name = 'Admin Team'")
        .fetch_one(&w.pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn team_invite_cannot_join_or_empty_admin_team(pool: PgPool) {
    let w = world(pool).await;
    let admins = admin_team(&w).await;
    let bob = member_with(&w.app, &w.pool, &w.ws, "bob@example.com", &["team.invite"]).await;
    let bob_id = w.user_id("bob@example.com").await;
    let ada_id = w.user_id("ada@example.com").await;

    let (status, res) = w
        .call(
            Method::POST,
            &format!("/{admins}/add-users"),
            Some(json!({ "user_ids": [bob_id] })),
            &bob,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "self-escalation: {res}");
    assert_eq!(res["message"], UNAUTHORIZED);
    let (status, _) = w
        .call(
            Method::POST,
            &format!("/{admins}/remove-users"),
            Some(json!({ "user_ids": [ada_id] })),
            &bob,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Ordinary teams still work with team.invite alone.
    let sales = w.create("Sales").await;
    let (status, _) = w
        .call(
            Method::POST,
            &format!("/{sales}/add-users"),
            Some(json!({ "user_ids": [bob_id] })),
            &bob,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn workspace_admin_member_may_manage_admin_team(pool: PgPool) {
    let w = world(pool).await;
    let admins = admin_team(&w).await;
    let bob = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "bob@example.com",
        &["workspace_admin"],
    )
    .await;
    member_with(&w.app, &w.pool, &w.ws, "carol@example.com", &[]).await;
    let carol = w.user_id("carol@example.com").await;

    let (status, res) = w
        .call(
            Method::POST,
            &format!("/{admins}/add-users"),
            Some(json!({ "user_ids": [carol] })),
            &bob,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{res}");
}

#[sqlx::test]
async fn admin_team_cannot_be_deleted_or_stripped_by_non_admin(pool: PgPool) {
    let w = world(pool).await;
    let admins = admin_team(&w).await;
    let bob = member_with(
        &w.app,
        &w.pool,
        &w.ws,
        "bob@example.com",
        &["team.delete", "team.permissions", "team.get"],
    )
    .await;

    assert_eq!(
        w.call(Method::DELETE, &format!("/{admins}"), None, &bob)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.grant(&admins, json!(["-workspace_admin"]), &bob).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.get(&admins).await["permissions"],
        json!(["workspace_admin"])
    );

    // Owner may.
    assert_eq!(
        w.grant(&admins, json!(["-workspace_admin"]), &w.owner)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(w.get(&admins).await["permissions"], json!([]));
}

// ================= edges

#[sqlx::test]
async fn name_and_description_exact_caps_count_chars(pool: PgPool) {
    let w = world(pool).await;
    let (name, description) = ("é".repeat(55), "😀".repeat(225));

    let (status, res) = w
        .call(
            Method::POST,
            "",
            Some(json!({ "name": name, "description": description })),
            &w.owner,
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{res}");
    let t = w.get(res["data"]["team_id"].as_str().unwrap()).await;
    assert_eq!(t["name"], name);
}

#[sqlx::test]
async fn empty_update_changes_nothing(pool: PgPool) {
    let w = world(pool).await;
    let team = w.create("Sales").await;

    let (status, _) = w
        .call(Method::POST, &format!("/{team}"), Some(json!({})), &w.owner)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(w.get(&team).await["name"], "Sales");
}

#[sqlx::test]
async fn meta_must_be_an_object(pool: PgPool) {
    let w = world(pool).await;

    for meta in [json!("x"), json!(1), json!([1])] {
        let (status, res) = w
            .call(
                Method::POST,
                "",
                Some(json!({ "name": "n", "description": "d", "meta": meta })),
                &w.owner,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{meta}");
        assert_eq!(
            res["errors"]["meta"],
            json!(["The meta field must be an array."])
        );
    }
}

#[sqlx::test]
async fn nul_character_is_422_not_500(pool: PgPool) {
    let w = world(pool).await;

    let (status, res) = w
        .call(
            Method::POST,
            "",
            Some(json!({ "name": "a\u{0}b", "description": "d" })),
            &w.owner,
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{res}");
}

#[sqlx::test]
async fn dash_with_space_still_removes(pool: PgPool) {
    let w = world(pool).await;
    let team = w.create("Sales").await;
    w.grant(&team, json!(["relation.get"]), &w.owner).await;

    w.grant(&team, json!(["- Relation.Get "]), &w.owner).await;

    assert_eq!(w.get(&team).await["permissions"], json!([]));
}

#[sqlx::test]
async fn paging_past_end_clamps_and_huge_numbers(pool: PgPool) {
    let w = world(pool).await;
    for name in ["A", "B"] {
        w.create(name).await;
    }

    let (_, res) = w
        .call(Method::GET, "?page=9&page_size=2", None, &w.owner)
        .await;
    assert_eq!(res["data"], json!([]));
    assert_eq!(res["meta"]["total"], 3, "real total on an empty page");
    assert_eq!(res["meta"]["last_page"], 2);

    let (status, res) = w
        .call(Method::GET, "?page=9223372036854775807", None, &w.owner)
        .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"], json!([]));

    let (_, res) = w
        .call(Method::GET, "?page_size=500&page=0", None, &w.owner)
        .await;
    assert_eq!(res["meta"]["per_page"], 100);
    assert_eq!(res["meta"]["current_page"], 1);
}
