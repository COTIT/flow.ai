//! Production operator CLIs, run in-process because signup and mail are off there:
//! `create-owner` (first user + workspace, one-time password printed once) and
//! `set-user-password` (login for `afm` or `ruaha`: operator-typed password from stdin, never
//! printed, or `--generate`: one-time password printed once, changed at first field sign-in).

use std::io::{BufRead, Write};

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::Rules,
    modules::{
        auth::{
            dto::{NewPerson, NewSignup, required_email},
            password::{self, Password},
            repo as tokens, service, token,
        },
        farm_logins::repo as logins,
        teams::repo as teams,
        users::repo::{self as users, NewUser},
        workspaces::{
            model::{WorkspaceType, is_slug},
            repo as workspaces,
        },
    },
};

/// Only workspaces `set-user-password` writes to, on production: AFM's and RUAHA's.
const PASSWORD_WORKSPACES: &[&str] = &["afm", "ruaha"];
const SET_PASSWORD_USAGE: &str = "usage: set-user-password --workspace afm|ruaha --email <email> \
     [--admin] [--generate] [--first-name F --last-name L]   (password: one line on stdin)";

/// The owner just made: their workspace and the password to sign in with once.
pub struct Owner {
    pub workspace_id: Uuid,
    pub password: String,
}

/// Create the first owner. An existing email or workspace name is refused (422), never reused.
pub async fn create_owner(
    state: &AppState,
    email: &str,
    workspace: &str,
    first_name: &str,
    last_name: &str,
) -> Result<Owner, AppError> {
    if !is_slug(workspace) {
        return Err(AppError::field(
            "workspace_name",
            "The workspace name may only hold lowercase letters, digits and dashes.",
        ));
    }
    let password = one_time_password()?;
    let signup = NewSignup {
        person: NewPerson {
            first_name: first_name.to_owned(),
            middle_name: None,
            last_name: last_name.to_owned(),
            email: email.trim().to_lowercase(),
            password: Password::generated(password.clone()),
        },
        workspace_name: workspace.to_owned(),
        workspace_type: WorkspaceType::default(),
    };
    let session = service::create_workspace(state, signup).await?;
    let workspace_id = session
        .workspace_ids
        .first()
        .copied()
        .ok_or_else(|| AppError::Internal("create-owner: no workspace".into()))?;
    Ok(Owner {
        workspace_id,
        password,
    })
}

/// Random password that passes the staff rules (mixed case, digit, symbol).
fn one_time_password() -> Result<String, AppError> {
    let (plain, _) = token::generate()?;
    Ok(format!("Own-{}-Aa1!", &plain.expose()[..24]))
}

/// Whose login `set_user_password` sets. Names are used only when the user is new.
pub struct LoginFor<'a> {
    pub workspace: &'a str,
    pub email: &'a str,
    /// Also join the workspace's admin team (every permission, same as an admin invite).
    pub admin: bool,
    pub first_name: Option<&'a str>,
    pub last_name: Option<&'a str>,
    /// One-time password: field app asks for a new one at first sign-in.
    pub temporary: bool,
}

/// What `set_user_password` did. Nothing secret.
pub struct PasswordSet {
    pub created: bool,
    pub sessions_revoked: u64,
}

/// Set an operator-typed password: update a live user (ending every session) or create one,
/// then ensure active `afm` membership, plus the admin team with `admin`. One transaction.
/// Same policy and argon2id hash as signup. `None` password = missing (422).
pub async fn set_user_password(
    state: &AppState,
    who: &LoginFor<'_>,
    password: Option<Password>,
) -> Result<PasswordSet, AppError> {
    if !PASSWORD_WORKSPACES.contains(&who.workspace) {
        return Err(AppError::field(
            "workspace",
            format!("only afm or ruaha, not {}", who.workspace),
        ));
    }
    let mut r = Rules::default();
    let email = required_email(&mut r, "email", Some(who.email.trim().to_lowercase()));
    let password = password::check(&mut r, "password", password);
    let existing = match &email {
        Some(e) => users::find_credentials_by_email(&state.db, e)
            .await?
            .map(|c| c.id),
        None => None,
    };
    let names = if existing.is_none() {
        let first = r.required("first_name", who.first_name.map(str::to_owned));
        let last = r.required("last_name", who.last_name.map(str::to_owned));
        first.zip(last)
    } else {
        None
    };
    r.into_result()?;
    let (Some(email), Some(password)) = (email, password) else {
        return Err(AppError::Internal("validated input missing".into()));
    };
    let workspace_id = sqlx::query_scalar!(
        "SELECT id FROM workspaces WHERE name = $1 AND deleted_at IS NULL",
        who.workspace
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::field("workspace", format!("no workspace {}", who.workspace)))?;
    let admin_team = match who.admin {
        true => Some(
            teams::admin_team(&state.db, workspace_id)
                .await?
                .ok_or_else(|| AppError::field("admin", "no admin team in this workspace"))?,
        ),
        false => None,
    };

    // Hash before opening tx: argon2 time shouldn't hold a DB connection + row locks.
    let hash = password::hash(password).await?;
    let mut tx = state.db.begin().await?;
    let (user_id, created, sessions_revoked) = match (existing, &names) {
        (Some(id), _) => {
            logins::set_password(&mut *tx, id, &hash).await?;
            (id, false, tokens::delete_all_for_user(&mut *tx, id).await?)
        }
        (None, Some((first, last))) => {
            let new = NewUser {
                email: &email,
                first_name: first,
                middle_name: None,
                last_name: last,
                password_hash: &hash,
            };
            let id = users::insert(&mut *tx, &new)
                .await
                .map_err(service::taken)?;
            (id, true, 0)
        }
        (None, None) => return Err(AppError::Internal("validated names missing".into())),
    };
    // Same rows an accepted invite gives (into the admin team with `admin`); reactivates a removed member.
    if who.temporary {
        sqlx::query("UPDATE users SET must_change_password = true WHERE id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    }
    workspaces::add_member(&mut *tx, workspace_id, user_id).await?;
    if let Some(team) = admin_team {
        teams::add_members(&mut *tx, workspace_id, team, &[user_id]).await?;
    }
    tokens::clear_login_attempts(&mut *tx, &email).await?;
    tx.commit().await?;
    Ok(PasswordSet {
        created,
        sessions_revoked,
    })
}

/// CLI `set-user-password`: flags from `args`, password = first line of `stdin` (line end
/// stripped, nothing trimmed). Writes only non-secret lines to `out`. Exit code: 0 done,
/// 1 refused, 2 usage.
pub async fn set_user_password_cli(
    state: &AppState,
    args: &[String],
    mut stdin: impl BufRead,
    out: &mut impl Write,
) -> i32 {
    let (mut workspace, mut email, mut first_name, mut last_name) = (None, None, None, None);
    let (mut admin, mut generate) = (false, false);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--admin" => {
                admin = true;
                continue;
            }
            "--generate" => {
                generate = true;
                continue;
            }
            _ => {}
        }
        // Unknown flags refused: a `--password` on the command line is never read as one.
        let slot = match arg.as_str() {
            "--workspace" => &mut workspace,
            "--email" => &mut email,
            "--first-name" => &mut first_name,
            "--last-name" => &mut last_name,
            _ => return usage(out),
        };
        match it.next().filter(|v| !v.starts_with("--")) {
            Some(value) => *slot = Some(value.as_str()),
            None => return usage(out),
        }
    }
    let (Some(workspace), Some(email)) = (workspace, email) else {
        return usage(out);
    };

    let one_time = match generate {
        true => match one_time_password() {
            Ok(p) => Some(p),
            Err(err) => {
                let _ = writeln!(out, "set-user-password failed: {err}");
                return 1;
            }
        },
        false => None,
    };
    let password = match &one_time {
        Some(p) => Some(Password::generated(p.clone())),
        None => {
            let mut line = String::new();
            if let Err(e) = stdin.read_line(&mut line) {
                let _ = writeln!(out, "set-user-password: stdin unreadable: {}", e.kind());
                return 1;
            }
            let stripped = line.strip_suffix('\n').unwrap_or(&line);
            let keep = stripped.strip_suffix('\r').unwrap_or(stripped).len();
            line.truncate(keep);
            (!line.is_empty()).then(|| Password::generated(line))
        }
    };

    let who = LoginFor {
        workspace,
        email,
        admin,
        first_name,
        last_name,
        temporary: generate,
    };
    match set_user_password(state, &who, password).await {
        Ok(done) => {
            let _ = writeln!(
                out,
                "set-user-password {workspace}: {} {}, active member, admin={}, {} sessions revoked",
                email.trim().to_lowercase(),
                if done.created { "created" } else { "updated" },
                if admin { "yes" } else { "unchanged" },
                done.sessions_revoked,
            );
            if let Some(p) = &one_time {
                // Printed once, to the operator's stdout only: it exists nowhere else.
                let _ = writeln!(out, "one-time password: {p}");
            }
            0
        }
        Err(AppError::Validation(errors)) => {
            for message in errors.values().flatten() {
                let _ = writeln!(out, "set-user-password refused: {message}");
            }
            1
        }
        Err(err) => {
            let _ = writeln!(out, "set-user-password failed: {err}");
            1
        }
    }
}

fn usage(out: &mut impl Write) -> i32 {
    let _ = writeln!(out, "{SET_PASSWORD_USAGE}");
    2
}
