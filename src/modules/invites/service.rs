//! Invite rules: who may invite, code lifecycle, joining a workspace. No HTTP types here.

use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::workspace::NOT_ALLOWED,
    mail::Outgoing,
    modules::{
        auth::{
            password,
            service::{self as auth, EMAIL_TAKEN, Session},
            token,
        },
        invites::{
            dto::{Answer, BAD_TEAM, NewInvitee, SendInvite},
            repo::{self, InviteRow, NewInvite},
        },
        teams::repo as teams,
        users::repo as users,
        workspaces::repo as workspaces,
    },
};

const INVITE_TTL_DAYS: i64 = 7;
const EXPIRED: &str = "Workspace invite has expired";
const USED: &str = "This invitation has already been used.";
const ALREADY_MEMBER: &str = "user already exists in the workspace";
const VERIFY_FIRST: &str = "Verify your email address first.";

/// Invite plus derived fields for the status page.
pub struct StatusView {
    pub invite: InviteRow,
    pub status: &'static str,
    pub new_user: bool,
}

/// Member (already checked for `invite.create`) invites `email` into `workspace_id`.
/// Re-invite rotates the code, so only the newest email works. Returns invite id.
/// `inviter_is_admin`: owner or `workspace_admin`, needed to invite into a team holding
/// `workspace_admin` (else a second account of the inviter would join it).
// shortcut: mail sent inline, not queued; move to a job queue when send latency hurts.
pub async fn send(
    state: &AppState,
    inviter: Uuid,
    inviter_is_admin: bool,
    workspace_id: Uuid,
    SendInvite { email, team_id }: SendInvite,
) -> Result<Uuid, AppError> {
    let ws = workspaces::find(&state.db, workspace_id)
        .await?
        .ok_or(AppError::NotFound)?;
    // Each invite mails a stranger from our domain: only proven inbox owners may do that.
    let inviter_verified = users::find_contact(&state.db, inviter)
        .await?
        .is_some_and(|c| c.email_verified);
    if !inviter_verified {
        return Err(AppError::Forbidden(VERIFY_FIRST));
    }
    if workspaces::is_member_by_email(&state.db, workspace_id, &email).await? {
        return Err(AppError::field("email", ALREADY_MEMBER));
    }
    if let Some(team) = team_id {
        match teams::admin_flag(&state.db, workspace_id, team).await? {
            None => return Err(AppError::field("team_id", BAD_TEAM)),
            Some(true) if !inviter_is_admin => return Err(AppError::Forbidden(NOT_ALLOWED)),
            Some(_) => {}
        }
    }
    // Each send emails an arbitrary address from our domain: cap per inviter, so a throwaway
    // signup can't turn us into a spam relay. DB count, so the cap holds across instances.
    if repo::sends_in_last_hour(&state.db, inviter).await? >= state.invites_per_hour {
        return Err(AppError::TooManyRequests);
    }

    let (code, hash) = token::generate()?;
    let invite_id = repo::upsert_pending(
        &state.db,
        &NewInvite {
            workspace_id,
            invited_by_user_id: inviter,
            email: &email,
            token_hash: &hash,
            expires_at: Utc::now() + Duration::days(INVITE_TTL_DAYS),
            team_id,
        },
    )
    .await?;
    // Logged before sending: a failed SMTP attempt still counts, so retry loops stay bounded.
    repo::record_send(&state.db, inviter).await?;
    // Row first, mail second: if SMTP fails the owner retries, and the retry rotates the code.
    state
        .mailer
        .send(invite_mail(
            &state.frontend_url,
            &ws.name,
            email,
            code.expose(),
        ))
        .await?;
    Ok(invite_id)
}

/// Public status lookup by code.
pub async fn status(state: &AppState, code: &str) -> Result<StatusView, AppError> {
    let invite = find(state, code).await?;
    let new_user = !users::exists_by_email(&state.db, &invite.email).await?;
    Ok(StatusView {
        status: effective_status(&invite),
        new_user,
        invite,
    })
}

/// New account through invite: user + membership + invite accepted + token, one transaction.
pub async fn register(state: &AppState, input: NewInvitee) -> Result<Session, AppError> {
    let invite = find(state, &input.invite_code).await?;
    if !same_email(&invite.email, &input.person.email) {
        return Err(AppError::NotFound);
    }
    ensure_open(&invite)?;
    if users::exists_by_email(&state.db, &input.person.email).await? {
        return Err(AppError::field("email", EMAIL_TAKEN));
    }

    let password_hash = password::hash_ref(&input.person.password).await?;
    let mut tx = state.db.begin().await?;
    let user_id = users::insert(&mut *tx, &input.person.as_new_user(&password_hash))
        .await
        .map_err(auth::taken)?;
    // Row lock: a parallel register on this code waits here, then sees it answered.
    if !repo::mark_answered(&mut *tx, invite.id, &token::hash(&input.invite_code), true).await? {
        return Err(AppError::field("invite_code", USED)); // tx dropped = rollback
    }
    join(&mut tx, &invite, user_id).await?;
    // Signed up with the emailed code, so the inbox is proven.
    users::mark_email_verified(&mut *tx, user_id).await?;
    let token = auth::issue_token(&mut *tx, user_id, state.token_ttl).await?;
    tx.commit().await?;

    Ok(Session {
        token,
        workspace_ids: vec![invite.workspace_id],
    })
}

/// Logged-in user accepts or declines. `invite_code` is the emailed code, or the invite id from
/// /v1/me (verified email required). Repeating an accept is a no-op success while still a member.
pub async fn answer(state: &AppState, user_id: Uuid, input: Answer) -> Result<(), AppError> {
    let by_code = token::looks_valid(&input.invite_code);
    let invite = if by_code {
        find(state, &input.invite_code).await?
    } else {
        let id = Uuid::parse_str(&input.invite_code).map_err(|_| AppError::NotFound)?;
        repo::find_by_id(&state.db, id)
            .await?
            .ok_or(AppError::NotFound)?
    };
    let contact = users::find_contact(&state.db, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !same_email(&invite.email, &contact.email) {
        return Err(AppError::NotFound);
    }
    // The id is no secret: only an account proven to own the inbox may use it, so a squatter
    // who registered someone else's address can't take their invites.
    if !by_code && !contact.email_verified {
        return Err(AppError::Forbidden(VERIFY_FIRST));
    }

    match invite.status.as_str() {
        "pending" => {
            ensure_open(&invite)?;
            let mut tx = state.db.begin().await?;
            if !repo::mark_answered(&mut *tx, invite.id, &invite.token_hash, input.accept).await? {
                return Err(AppError::field("invite_code", USED));
            }
            if input.accept {
                join(&mut tx, &invite, user_id).await?;
            }
            if by_code {
                // Holding the emailed code proves the inbox (Laravel verified here too).
                users::mark_email_verified(&mut *tx, user_id).await?;
            }
            tx.commit().await?;
        }
        // Double click / retried request (Laravel COT-241). Never re-adds: a removed member
        // replaying an old code must not get back in.
        "accepted"
            if input.accept
                && workspaces::is_member(&state.db, invite.workspace_id, user_id).await? => {}
        _ => return Err(AppError::field("invite_code", USED)),
    }
    Ok(())
}

/// Accepted invite: workspace membership, plus the invite's team if still live.
async fn join(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invite: &InviteRow,
    user_id: Uuid,
) -> Result<(), AppError> {
    workspaces::add_member(&mut **tx, invite.workspace_id, user_id).await?;
    if let Some(team) = invite.team_id {
        teams::add_members(&mut **tx, invite.workspace_id, team, &[user_id]).await?;
    }
    Ok(())
}

/// Invite for a plaintext code. Malformed and unknown codes are the same 404.
async fn find(state: &AppState, code: &str) -> Result<InviteRow, AppError> {
    if !token::looks_valid(code) {
        return Err(AppError::NotFound);
    }
    repo::find_by_token_hash(&state.db, &token::hash(code))
        .await?
        .ok_or(AppError::NotFound)
}

fn effective_status(invite: &InviteRow) -> &'static str {
    match invite.status.as_str() {
        "pending" if invite.expires_at <= Utc::now() => "expired",
        "pending" => "pending",
        "accepted" => "accepted",
        _ => "denied",
    }
}

/// 422 unless pending and not expired.
fn ensure_open(invite: &InviteRow) -> Result<(), AppError> {
    match effective_status(invite) {
        "pending" => Ok(()),
        "expired" => Err(AppError::field("invite_code", EXPIRED)),
        _ => Err(AppError::field("invite_code", USED)),
    }
}

// Unicode lowercase, same as Postgres lower() used by the unique indexes.
fn same_email(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn invite_mail(frontend_url: &str, workspace: &str, to: String, code: &str) -> Outgoing {
    // shortcut: English plain text only; add NL + HTML template when mail templates are ported.
    Outgoing {
        subject: format!("You're invited to join {workspace} on Genesy"),
        body: format!(
            "You've been invited to join the {workspace} workspace on Genesy.\n\n\
             Accept the invitation here:\n\
             {frontend_url}/accept-invitation?code={code}\n\n\
             The link expires in {INVITE_TTL_DAYS} days. If you weren't expecting this, ignore this email.\n"
        ),
        to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(status: &str, expires_in: Duration) -> InviteRow {
        InviteRow {
            id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            workspace_name: "acme".into(),
            email: "a@b.co".into(),
            status: status.into(),
            expires_at: Utc::now() + expires_in,
            created_at: Utc::now(),
            token_hash: vec![0; 32],
            team_id: None,
        }
    }

    #[test]
    fn effective_status_derives_expired_from_time() {
        assert_eq!(
            effective_status(&row("pending", Duration::hours(1))),
            "pending"
        );
        assert_eq!(
            effective_status(&row("pending", -Duration::seconds(1))),
            "expired"
        );
        // Answered invites keep their answer after expiry.
        assert_eq!(
            effective_status(&row("accepted", -Duration::days(1))),
            "accepted"
        );
        assert_eq!(
            effective_status(&row("denied", Duration::days(1))),
            "denied"
        );
    }

    #[test]
    fn ensure_open_messages() {
        assert!(ensure_open(&row("pending", Duration::hours(1))).is_ok());
        assert_eq!(
            ensure_open(&row("pending", -Duration::seconds(1)))
                .unwrap_err()
                .to_string(),
            EXPIRED
        );
        assert_eq!(
            ensure_open(&row("accepted", Duration::hours(1)))
                .unwrap_err()
                .to_string(),
            USED
        );
    }

    #[test]
    fn same_email_ignores_unicode_case() {
        assert!(same_email("Ünï@Example.com", "ünï@example.COM"));
        assert!(!same_email("a@b.co", "a@b.com"));
    }

    #[test]
    fn mail_has_link_and_expiry() {
        let mail = invite_mail("https://app.test", "acme", "a@b.co".into(), "abc");

        assert_eq!(mail.to, "a@b.co");
        assert!(
            mail.body
                .contains("https://app.test/accept-invitation?code=abc")
        );
        assert!(mail.body.contains("7 days"));
        assert!(mail.subject.contains("acme"));
    }
}
