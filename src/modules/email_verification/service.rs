//! Email verification rules: issue + mail a link, verify by token, resend with quota. No HTTP types.

use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    mail::Outgoing,
    modules::{
        auth::{repo as access_tokens, token},
        email_verification::repo,
        users::repo as users,
    },
};

const TOKEN_TTL_HOURS: i64 = 24;
/// Resend limits per user, signup mail included.
const PER_HOUR: i64 = 5;
const PER_DAY: i64 = 10;
const MIN_GAP_SECS: i64 = 60;

/// Mail a fresh link right after signup. Failure is logged only: the account already exists
/// and the user can ask for a resend.
pub async fn send_after_signup(state: &AppState, user_id: Uuid, email: &str) {
    let result = async {
        let (plain, hash) = token::generate()?;
        repo::insert(&state.db, user_id, &hash, expiry()).await?;
        state
            .mailer
            .send(verification_mail(
                &state.api_public_url,
                email,
                plain.expose(),
            ))
            .await
    }
    .await;
    if let Err(err) = result {
        tracing::warn!(error = %err, %user_id, "verification mail after signup failed");
    }
}

/// Use a link. True when it is live and its user exists (repeat clicks stay true).
/// The first verification logs out every session: a squatter who signed up with this address
/// before the real owner clicked loses access.
// shortcut: squatter can log back in with their password; full fix = verify link also sets a
// new password, once password reset is ported.
pub async fn verify(state: &AppState, plain: &str) -> Result<bool, AppError> {
    if !token::looks_valid(plain) {
        return Ok(false);
    }
    let mut tx = state.db.begin().await?;
    let Some(user_id) = repo::find_live_user(&mut *tx, &token::hash(plain)).await? else {
        return Ok(false);
    };
    if users::mark_email_verified(&mut *tx, user_id).await? {
        access_tokens::delete_all_for_user(&mut *tx, user_id).await?;
    }
    tx.commit().await?;
    Ok(true)
}

/// Logged-in resend. Already verified = no mail, still success.
/// Runs under a row lock on the user, so parallel requests can't all pass the quota check,
/// and a failed send rolls its token back instead of eating the quota.
pub async fn resend(state: &AppState, user_id: Uuid) -> Result<(), AppError> {
    let mut tx = state.db.begin().await?;
    let contact = users::lock_contact(&mut *tx, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if contact.email_verified {
        return Ok(());
    }
    let stats = repo::issue_stats(&mut *tx, user_id).await?;
    let too_soon = stats
        .newest
        .is_some_and(|t| t > Utc::now() - Duration::seconds(MIN_GAP_SECS));
    if too_soon || stats.last_hour >= PER_HOUR || stats.last_day >= PER_DAY {
        return Err(AppError::TooManyRequests);
    }

    let (plain, hash) = token::generate()?;
    repo::insert(&mut *tx, user_id, &hash, expiry()).await?;
    state
        .mailer
        .send(verification_mail(
            &state.api_public_url,
            &contact.email,
            plain.expose(),
        ))
        .await?;
    tx.commit().await?;
    Ok(())
}

fn expiry() -> chrono::DateTime<Utc> {
    Utc::now() + Duration::hours(TOKEN_TTL_HOURS)
}

fn verification_mail(api_public_url: &str, to: &str, token: &str) -> Outgoing {
    // shortcut: English plain text only; add NL + HTML template when mail templates are ported.
    Outgoing {
        to: to.to_owned(),
        subject: "Verify your email for Genesy".into(),
        body: format!(
            "Someone created a Genesy account with this email address.\n\n\
             If that was you, confirm the address here:\n\
             {api_public_url}/v1/verify-email/{token}\n\n\
             If you didn't create this account, don't open the link. Ignore this email and the \
             account stays unconfirmed.\n\n\
             The link expires in {TOKEN_TTL_HOURS} hours.\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_points_at_api_and_warns_non_owners() {
        let mail = verification_mail("https://api.test", "a@b.co", "abc");

        assert_eq!(mail.to, "a@b.co");
        assert!(mail.body.contains("https://api.test/v1/verify-email/abc"));
        assert!(
            mail.body
                .contains("If you didn't create this account, don't open the link")
        );
        assert!(mail.body.contains("24 hours"));
    }
}
