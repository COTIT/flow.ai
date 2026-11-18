//! `wf01_reconcile` (C4b replaces the body): find by external id, compare, settle.

use crate::{
    error::AppError,
    jobs::{kinds::Ctx, repo::Claimed, runner::Step},
};

/// Not built yet: a permanent error, so the rail does not retry it.
pub async fn run(_ctx: &Ctx, _job: &Claimed) -> Result<Step, AppError> {
    Err(AppError::Unprocessable(
        "wf01 reconcile is not built yet".into(),
    ))
}
