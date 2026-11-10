//! Assistant review facts for design writes: field rows from same guarded code as dispatch.

use super::{dto::*, repo, service};
use crate::{
    AppError,
    modules::{
        assistant::{dto::ProposalInput, preconditions, repo::Owner},
        permissions,
    },
};
use serde_json::{Value, json};
use sqlx::PgConnection;

/// Design writes whose review shows field changes.
pub fn requires_facts(operation: &str) -> bool {
    matches!(
        operation,
        "storefront_design_edit" | "storefront_design_restore" | "storefront_design_publish"
    )
}

/// Admin check precedes any document read; stale or invalid drafts refuse before a card exists.
#[tracing::instrument(name = "storefront_design_capture", level = "debug", skip_all)]
pub async fn capture(
    conn: &mut PgConnection,
    owner: Owner,
    input: &ProposalInput,
) -> Result<Option<Value>, AppError> {
    let operation = input.operation_id.as_str();
    if !requires_facts(operation) {
        return Ok(None);
    }
    let access = permissions::repo::access(&mut *conn, owner.workspace_id, owner.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let grants = permissions::model::Granted::new(access.owner, access.permissions);
    if !access.active || !grants.allows("workspace_admin") {
        return Err(AppError::Forbidden(
            "You do not have permission for this action.",
        ));
    }
    let ws = owner.workspace_id;
    let body = input.body.clone().unwrap_or_else(|| json!({}));
    let page = repo::page(&mut *conn, ws).await?;
    let changes = if operation == "storefront_design_edit" {
        let edit = preconditions::validated::<StorefrontDesignEdit>(body)?;
        service::plan_edit(conn, ws, page.as_ref(), &edit).await?.1
    } else {
        let select = preconditions::validated::<StorefrontDesignSelect>(body)?;
        let publish = operation == "storefront_design_publish";
        service::selection_rows(conn, ws, page.as_ref(), &select, publish).await?
    };
    Ok(Some(json!({"version":1,"operation_id":operation,"facts":{
        "kind":"storefront_design",
        // `locked` is what the dispatch claim requires before a reviewed write runs.
        "locked":{
            "version":page.as_ref().map_or(0, |p| p.version),
            "draft_revision_id":page.as_ref().and_then(|p| p.draft_revision_id),
            "published_revision_id":page.as_ref().and_then(|p| p.published_revision_id),
        },
        "changes":changes,
    }})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::storefront_design::repo::NewRevision;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn person(pool: &PgPool, email: &str) -> Uuid {
        sqlx::query_scalar("INSERT INTO users (email, first_name, last_name, password_hash) VALUES ($1, 'A', 'B', 'h') RETURNING id")
            .bind(email)
            .fetch_one(pool)
            .await
            .unwrap()
    }
    async fn join(pool: &PgPool, ws: Uuid, user: Uuid, grant: Option<&str>) {
        sqlx::query("INSERT INTO workspace_members (workspace_id, user_id) VALUES ($1, $2)")
            .bind(ws)
            .bind(user)
            .execute(pool)
            .await
            .unwrap();
        if let Some(grant) = grant {
            sqlx::query("WITH t AS (INSERT INTO teams (workspace_id, name, description) VALUES ($1, $3, '') RETURNING id), m AS (INSERT INTO team_members (team_id, user_id) SELECT id, $2 FROM t) INSERT INTO team_permissions (team_id, permission) SELECT id, $3 FROM t")
                .bind(ws)
                .bind(user)
                .bind(grant)
                .execute(pool)
                .await
                .unwrap();
        }
    }
    async fn world(pool: &PgPool) -> (Uuid, Uuid) {
        let owner = person(pool, "owner@example.com").await;
        let ws: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, created_by_user_id) VALUES ('acme', $1) RETURNING id",
        )
        .bind(owner)
        .fetch_one(pool)
        .await
        .unwrap();
        join(pool, ws, owner, None).await;
        (ws, owner)
    }
    async fn facts(
        pool: &PgPool,
        ws: Uuid,
        user: Uuid,
        operation: &str,
        body: Value,
    ) -> Result<Option<Value>, AppError> {
        let input: ProposalInput = serde_json::from_value(json!({
            "session_id": Uuid::now_v7(),
            "operation_id": operation,
            "method": "POST",
            "path": format!("/v1/workspace/{ws}/storefront/pages/home/x"),
            "body": body,
        }))
        .unwrap();
        let owner = Owner {
            workspace_id: ws,
            user_id: user,
            login_id: Uuid::now_v7(),
        };
        capture(&mut pool.acquire().await.unwrap(), owner, &input).await
    }
    async fn revision(pool: &PgPool, ws: Uuid, actor: Uuid, heading: &str) -> Uuid {
        let mut doc = starter();
        if let StorefrontDesignSection::Hero { props, .. } = &mut doc.sections[0] {
            props.heading = heading.into();
        }
        let mut tx = pool.begin().await.unwrap();
        let page = repo::lock_page(&mut tx, ws).await.unwrap();
        let new = NewRevision {
            parent: page.draft_revision_id,
            restored: None,
            document: &serde_json::to_value(&doc).unwrap(),
            actor,
            login: Uuid::now_v7(),
        };
        let id = repo::insert_revision(&mut tx, ws, page.id, new)
            .await
            .unwrap();
        repo::draft(&mut tx, ws, page.id, id).await.unwrap();
        tx.commit().await.unwrap();
        id
    }
    fn theme() -> Value {
        json!({"expected_version":0,"changes":[{"op":"set_theme","theme":"neutral"}]})
    }

    #[sqlx::test]
    async fn edit_facts_are_field_rows_from_the_guarded_plan(pool: PgPool) {
        let (ws, owner) = world(&pool).await;
        assert_eq!(
            facts(&pool, ws, owner, "storefront_design_edit", theme())
                .await
                .unwrap()
                .unwrap(),
            json!({"version":1,"operation_id":"storefront_design_edit","facts":{
                "kind":"storefront_design",
                "locked":{"version":0,"draft_revision_id":null,"published_revision_id":null},
                "changes":[{"field":"theme","label":null,"before":"forest","after":"neutral"}]
            }})
        );
        let stale = json!({"expected_version":3,"changes":[{"op":"set_theme","theme":"neutral"}]});
        assert!(matches!(
            facts(&pool, ws, owner, "storefront_design_edit", stale).await,
            Err(AppError::Conflict(_))
        ));
        let noop = json!({"expected_version":0,"changes":[{"op":"set_theme","theme":"forest"}]});
        assert!(matches!(
            facts(&pool, ws, owner, "storefront_design_edit", noop).await,
            Err(AppError::Validation(_))
        ));
        let mut document = theme();
        document["document"] = json!({});
        assert!(matches!(
            facts(&pool, ws, owner, "storefront_design_edit", document).await,
            Err(AppError::Validation(_))
        ));
        assert!(
            facts(&pool, ws, owner, "storefront_design_save", json!({}))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn only_admins_get_facts(pool: PgPool) {
        let (ws, _) = world(&pool).await;
        let stranger = person(&pool, "stranger@example.com").await;
        assert!(matches!(
            facts(&pool, ws, stranger, "storefront_design_edit", theme()).await,
            Err(AppError::NotFound)
        ));
        let member = person(&pool, "member@example.com").await;
        join(&pool, ws, member, Some("setting.update")).await;
        assert!(matches!(
            facts(&pool, ws, member, "storefront_design_edit", theme()).await,
            Err(AppError::Forbidden(_))
        ));
        let admin = person(&pool, "admin@example.com").await;
        join(&pool, ws, admin, Some("workspace_admin")).await;
        assert!(
            facts(&pool, ws, admin, "storefront_design_edit", theme())
                .await
                .unwrap()
                .is_some()
        );
    }

    #[sqlx::test]
    async fn restore_and_publish_facts_compare_with_draft_and_published(pool: PgPool) {
        let (ws, owner) = world(&pool).await;
        let first = revision(&pool, ws, owner, "One").await;
        let review = facts(
            &pool,
            ws,
            owner,
            "storefront_design_publish",
            json!({"expected_version":1,"revision_id":first}),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            review["facts"]["changes"][0],
            json!({"field":"theme","label":null,"before":null,"after":"forest"}),
            "first publication lists whole design"
        );
        let page = repo::page(&pool, ws).await.unwrap().unwrap();
        let mut tx = pool.begin().await.unwrap();
        repo::publish(&mut tx, ws, page.id, first, owner)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let second = revision(&pool, ws, owner, "Two").await;
        let v = 3;
        let review = facts(
            &pool,
            ws,
            owner,
            "storefront_design_publish",
            json!({"expected_version":v,"revision_id":second}),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            review["facts"]["changes"],
            json!([{"field":"heading","label":"One","before":"One","after":"Two"}])
        );
        assert_eq!(
            review["facts"]["locked"]["published_revision_id"],
            json!(first)
        );
        let review = facts(
            &pool,
            ws,
            owner,
            "storefront_design_restore",
            json!({"expected_version":v,"revision_id":first}),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            review["facts"]["changes"],
            json!([{"field":"heading","label":"Two","before":"Two","after":"One"}])
        );
        for (operation, body) in [
            (
                "storefront_design_publish",
                json!({"expected_version":v,"revision_id":first}),
            ),
            (
                "storefront_design_publish",
                json!({"expected_version":v - 1,"revision_id":second}),
            ),
            (
                "storefront_design_restore",
                json!({"expected_version":v - 1,"revision_id":first}),
            ),
        ] {
            assert!(
                matches!(
                    facts(&pool, ws, owner, operation, body.clone()).await,
                    Err(AppError::Conflict(_))
                ),
                "{body}"
            );
        }
        let unknown = json!({"expected_version":v,"revision_id":Uuid::now_v7()});
        assert!(matches!(
            facts(&pool, ws, owner, "storefront_design_restore", unknown).await,
            Err(AppError::NotFound)
        ));
    }
}
