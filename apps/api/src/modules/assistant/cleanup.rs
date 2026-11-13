//! Scheduled provider cleanup; never executes business actions or records provider bodies.

use super::repo;
use crate::{
    AppError,
    jobs::{kinds::Ctx, runner::Step},
};
use std::time::Duration;
use uuid::Uuid;

/// Delete due whole conversations; provider failure retains references for later sweeps.
pub async fn run(ctx: &Ctx) -> Result<Step, AppError> {
    repo::expire(&ctx.db).await?;
    let Some(key) = ctx.assistant_api_key.as_ref() else {
        return Ok(Step::Done);
    };
    crate::modules::flow_conversations::service::reconcile(ctx).await?;
    // One chunk bounds network time and leaves later rows for next scheduled minute.
    for due in repo::cleanup_due(&ctx.db).await?.iter().take(20) {
        let id = due["session_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| AppError::Internal("invalid assistant cleanup record".into()))?;
        let workspace = due["workspace_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| AppError::Internal("invalid assistant cleanup owner".into()))?;
        if !repo::cleanup_attempted(&ctx.db, workspace, id).await? {
            continue;
        }
        if let Some(conversation) = due["conversation_id"].as_str() {
            let url = conversation_url(conversation)?;
            let request = ctx
                .http
                .request(reqwest::Method::DELETE, &url)
                .header("xi-api-key", key.expose())
                .timeout(Duration::from_secs(10));
            let result = ctx.http.send(request).await;
            match result {
                Ok(res) if confirmed_delete(res.status().as_u16()) => {}
                _ => {
                    tracing::warn!(session_id=%id,"assistant provider cleanup remains pending");
                    continue;
                }
            }
        }
        repo::cleanup_ack(&ctx.db, id).await?;
    }
    Ok(Step::Done)
}

/// Fixed provider URL for validated native conversation reference.
pub(crate) fn conversation_url(id: &str) -> Result<String, AppError> {
    if !(1..=200).contains(&id.len())
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(AppError::Internal(
            "invalid assistant conversation reference".into(),
        ));
    }
    Ok(format!(
        "https://api.elevenlabs.io/v1/convai/conversations/{id}"
    ))
}
fn confirmed_delete(status: u16) -> bool {
    matches!(status, 200 | 204 | 404)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_targets_only_fixed_provider_path() {
        assert_eq!(
            conversation_url("conv_123").unwrap(),
            "https://api.elevenlabs.io/v1/convai/conversations/conv_123"
        );
        for bad in ["../other", "x/y", "https://evil.test", "x?key=secret", ""] {
            assert!(conversation_url(bad).is_err());
        }
    }
    #[test]
    fn failures_never_ack_remote_deletion() {
        for code in [200, 204, 404] {
            assert!(confirmed_delete(code));
        }
        for code in [202, 301, 401, 422, 429, 500, 503] {
            assert!(!confirmed_delete(code));
        }
    }
}
