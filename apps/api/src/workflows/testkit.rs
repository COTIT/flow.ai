//! Test fixtures: workspace with requester, approver, connections, WF-01 pair, requests.

use sqlx::PgPool;
use uuid::Uuid;

/// One workspace's WF-01 setup.
#[derive(Debug, Clone, Copy)]
pub struct Kit {
    pub ws: Uuid,
    pub requester: Uuid,
    pub approver: Uuid,
    pub pair: Uuid,
}

pub const FP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

async fn user(pool: &PgPool, email: String) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, first_name, last_name, password_hash)
         VALUES ($1, 'U', 'X', 'h') RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap()
}

pub async fn kit(pool: &PgPool, name: &str) -> Kit {
    let requester = user(pool, format!("req-{name}@x.io")).await;
    let approver = user(pool, format!("apr-{name}@x.io")).await;
    let ws: Uuid = sqlx::query_scalar(
        "INSERT INTO workspaces (name, created_by_user_id) VALUES ($2, $1) RETURNING id",
    )
    .bind(requester)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap();
    let pair: Uuid = sqlx::query_scalar(
        "WITH s AS (INSERT INTO integration_connections (workspace_id, provider, label, config, created_by)
                    VALUES ($1, 'odoo', 'o', '{}', $2) RETURNING id),
              t AS (INSERT INTO integration_connections (workspace_id, provider, label, config, created_by)
                    VALUES ($1, 'netsuite', 'n', '{}', $2) RETURNING id)
         INSERT INTO integration_pairs (workspace_id, workflow_key, source_connection_id,
             target_connection_id, config, created_by)
         SELECT $1, 'wf01_purchase', s.id, t.id, '{}', $2 FROM s, t RETURNING id",
    )
    .bind(ws)
    .bind(requester)
    .fetch_one(pool)
    .await
    .unwrap();
    Kit {
        ws,
        requester,
        approver,
        pair,
    }
}

/// Approved request + its `queued` execution. Returns the execution id.
pub async fn execution(pool: &PgPool, k: &Kit) -> Uuid {
    sqlx::query_scalar(
        "WITH r AS (INSERT INTO approval_requests (workspace_id, workflow_key, pair_id, subject_ref,
                        revision, fingerprint, snapshot, amount_minor, currency_ref, currency_decimals,
                        required_permission, requester_user_id, status, decided_by, decided_at)
                    VALUES ($1, 'wf01_purchase', $2, 'odoo:x:purchase.order:' || gen_random_uuid(), 1,
                        $3, '{}', 100, 'EUR', 2, 'integration.approve', $4, 'approved', $5, now())
                    RETURNING id)
         INSERT INTO workflow_executions (workspace_id, approval_request_id, workflow_key,
             correlation_id, status)
         SELECT $1, r.id, 'wf01_purchase', 'vz-' || replace(gen_random_uuid()::text, '-', ''), 'queued'
         FROM r RETURNING id",
    )
    .bind(k.ws)
    .bind(k.pair)
    .bind(FP)
    .bind(k.requester)
    .bind(k.approver)
    .fetch_one(pool)
    .await
    .unwrap()
}
