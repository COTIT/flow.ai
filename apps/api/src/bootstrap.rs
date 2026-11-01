//! Production bootstrap (CLI `create-owner`): the first user and workspace, made in-process
//! because signup is off in production. Prints a one-time password once; never resets anyone.

use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        auth::{
            dto::{NewPerson, NewSignup},
            password::Password,
            service, token,
        },
        workspaces::model::{WorkspaceType, is_slug},
    },
};

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
    let (plain, _) = token::generate()?;
    // Mixed case, digit and symbol, as the password rules want.
    let password = format!("Own-{}-Aa1!", &plain.expose()[..24]);
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
