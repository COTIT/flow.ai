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
        permissions::catalog::WORKSPACE_ADMIN,
        teams::repo as teams,
        users::repo as users,
        workspaces::repo as workspaces,
    },
};

pub const EMAIL_TAKEN: &str = "The email has already been taken.";
/// Seeded at signup (Laravel WorkspaceSetup::teamAttributes, typo kept).
const ADMIN_TEAM: &str = "Admin Team";
const ADMIN_TEAM_DESCRIPTION: &str = "This team consist of admins of the workspace";
const NO_WORKSPACE: &str = "No workspace found associated with your account";
/// Failed logins per email per 15 minutes before further attempts get 429.
const MAX_LOGIN_FAILURES: i64 = 10;
const WORKSPACE_NAME_TAKEN: &str = "The workspace name has already been taken.";

/// Fresh login: plaintext token (shown once) + workspaces it can open.
pub struct Session {
    pub token: PlainSecret,
    pub workspace_ids: Vec<Uuid>,
}

/// New user + workspace + owner membership + Admin Team + token, one transaction.
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
    // Laravel WorkspaceSetup seeds this team so admins can be added from the UI.
    let admins = teams::insert(
        &mut *tx,
        workspace_id,
        ADMIN_TEAM,
        ADMIN_TEAM_DESCRIPTION,
        None,
    )
    .await?;
    teams::grant(
        &mut *tx,
        workspace_id,
        admins,
        &[WORKSPACE_ADMIN.to_owned()],
    )
    .await?;
    teams::add_members(&mut *tx, workspace_id, admins, &[user_id]).await?;
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
    // Per-account lock on top of the per-IP limit, checked before any argon2 work.
    // Same for unknown emails, so a 429 doesn't reveal which emails exist.
    // The attempt is recorded before counting: a parallel burst sees its own rows and can't all
    // slip under the limit, and a client that disconnects mid-verify still counts.
    // shortcut: anyone can lock an email for 15 min with 10 bad tries; add CAPTCHA/step-up
    // instead of a hard lock if that DoS shows up.
    tokens::record_login_attempt(&state.db, &input.email).await?;
    if tokens::recent_login_attempts(&state.db, &input.email).await? > MAX_LOGIN_FAILURES {
        return Err(AppError::TooManyRequests);
    }
    let found = users::find_credentials_by_email(&state.db, &input.email).await?;
    let matches = match &found {
        Some(user) => password::verify(input.password, user.password_hash.clone()).await?,
        None => password::verify_dummy(input.password).await?,
    };
    let Some(user) = found.filter(|_| matches) else {
        return Err(AppError::InvalidCredentials); // attempt row stays as the failure
    };
    tokens::clear_login_attempts(&state.db, &input.email).await?;

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
