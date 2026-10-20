//! GET /healthz: 200 when DB answers, 503 otherwise. Compose + traefik health checks hit it.

use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use crate::AppState;

/// Ping DB with `SELECT 1`. DB error goes to log only, body stays generic.
pub async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Err(err) => {
            tracing::warn!(error = %err, "healthz db ping failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            )
        }
    }
}
