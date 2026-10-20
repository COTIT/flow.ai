//! Auth rules: signup, token issue. No HTTP types here.

use chrono::{Duration, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::Rules,
    modules::{
        auth::{
            dto::{LoginInput, NewSignup},
            password, repo as tokens,
            token::{self, PlainSecret},
        },
        email_verification::service as email_verification,
        users::repo as users,
        workspaces::repo as workspaces,
    },
};

pub const EMAIL_TAKEN: &str = "The email has already been taken.";
const NO_WORKSPACE: &str = "No workspace found associated with your account";
const WORKSPACE_NAME_TAKEN: &str = "The workspace name has already been taken.";

/// Fresh login: plaintext token (shown once) + workspaces it can open.
pub struct Session {
    pub token: PlainSecret,
    pub workspace_ids: Vec<Uuid>,
}

/// New user + workspace + owner membership + token, one transaction.
pub async fn create_workspace(state: &AppState, input: NewSignup) -> Result<Session, AppError> {
    // Pre-check reports both "taken" errors at once, like Laravel. Unique indexes still
    // catch the race where two signups pass this check together (see `taken`).
    let mut r = Rules::default();
    let email_taken = users::exists_by_email(&state.db, &input.person.email).await?;
    let name_taken = workspaces::name_taken(&state.db, &input.workspace_name).await?;
    r.check("email", !email_taken, EMAIL_TAKEN);
    r.check("workspace_name", !name_taken, WORKSPACE_NAME_TAKEN);
    r.into_result()?;

    // Hash before opening tx: argon2 time shouldn't hold a DB connection + row locks.
    let NewSignup {
        person,
        workspace_name,
        workspace_type,
    } = input;
    let password_hash = password::hash_ref(&person.password).await?;

    let mut tx = state.db.begin().await?;
    let user_id = users::insert(&mut *tx, &person.as_new_user(&password_hash))
        .await
        .map_err(taken)?;
    let workspace_id =
        workspaces::insert(&mut *tx, &workspace_name, workspace_type.as_str(), user_id)
            .await
            .map_err(taken)?;
    workspaces::add_member(&mut *tx, workspace_id, user_id).await?;
    let token = issue_token(&mut *tx, user_id, state.token_ttl).await?;
    tx.commit().await?;
    // After commit: a mail failure must not undo the account.
    email_verification::send_after_signup(state, user_id, &person.email).await;

    Ok(Session {
        token,
        workspace_ids: vec![workspace_id],
    })
}

/// Email + password -> fresh token. Unknown email and wrong password are indistinguishable,
/// in both body and timing. Password checked before workspaces, so 403 never leaks a valid email
/// to someone without its password.
pub async fn login(state: &AppState, input: LoginInput) -> Result<Session, AppError> {
    let Some(user) = users::find_credentials_by_email(&state.db, &input.email).await? else {
        password::verify_dummy(input.password).await?;
        return Err(AppError::InvalidCredentials);
    };
    if !password::verify(input.password, user.password_hash).await? {
        return Err(AppError::InvalidCredentials);
    }

    let workspace_ids = workspaces::active_workspace_ids(&state.db, user.id).await?;
    if workspace_ids.is_empty() {
        return Err(AppError::Forbidden(NO_WORKSPACE));
    }
    let token = issue_token(&state.db, user.id, state.token_ttl).await?;
    Ok(Session {
        token,
        workspace_ids,
    })
}

/// Revoke token used for this request. Other sessions stay live.
pub async fn logout(state: &AppState, token_id: Uuid) -> Result<(), AppError> {
    tokens::delete(&state.db, token_id).await?;
    Ok(())
}

/// New bearer token for user, valid for `ttl`.
pub async fn issue_token(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    ttl: Duration,
) -> Result<PlainSecret, AppError> {
    let (plain, hash) = token::generate()?;
    tokens::insert(db, user_id, &hash, Utc::now() + ttl).await?;
    Ok(plain)
}

/// Race loser passes a pre-check then hits a unique index; give it the same 422 as the pre-check.
pub fn taken(err: sqlx::Error) -> AppError {
    match err.as_database_error().and_then(|e| e.constraint()) {
        Some("users_email_unique") => AppError::field("email", EMAIL_TAKEN),
        Some("workspaces_name_unique") => AppError::field("workspace_name", WORKSPACE_NAME_TAKEN),
        _ => err.into(),
    }
}
