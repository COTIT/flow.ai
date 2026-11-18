//! `wf01_execute` (C4b replaces the body): claim, pre-checks, one POST, verify.

use crate::{
    error::AppError,
    jobs::{kinds::Ctx, repo::Claimed, runner::Step},
};

/// Not built yet: a permanent error, so the rail does not retry it.
pub async fn run(_ctx: &Ctx, _job: &Claimed) -> Result<Step, AppError> {
    Err(AppError::Unprocessable(
        "wf01 execute is not built yet".into(),
    ))
}
