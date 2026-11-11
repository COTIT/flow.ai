//! Your Flow rules: own rows only, Rust defaults merged with saved rows, components the
//! caller can't read listed as restricted, at most `MAX_PER_MEMBER` rows per member here.

use crate::{
    AppState,
    error::AppError,
    http::workspace::{NOT_ALLOWED, WorkspaceAccess},
    modules::flow_scenes::{
        dto::{self, CUSTOM_POSITION, FlowScene, FlowSceneConfig, SceneUpdate},
        repo::{self, SceneRow},
    },
};

/// Most saved scenes one member keeps in one workspace.
pub const MAX_PER_MEMBER: i64 = 20;

fn item(access: &WorkspaceAccess, row: SceneRow, saved: bool) -> Result<FlowScene, AppError> {
    let config: FlowSceneConfig = serde_json::from_value(row.config)
        .map_err(|e| AppError::Internal(format!("stored flow scene config: {e}")))?;
    let restricted = config
        .components
        .iter()
        .filter(|c| c.permission().is_some_and(|p| !access.allows(p)))
        .map(|c| c.id.clone())
        .collect();
    Ok(FlowScene {
        scene: row.scene,
        position: row.position,
        hidden: row.hidden,
        saved,
        schema_version: row.schema_version,
        config,
        restricted,
    })
}

fn default_row(scene: &str) -> Option<SceneRow> {
    dto::default(scene).map(|(position, config)| SceneRow {
        scene: scene.to_owned(),
        position,
        hidden: false,
        schema_version: 1,
        config: serde_json::to_value(config).unwrap_or_default(),
    })
}

/// The caller's scenes here: defaults for built-ins not saved, plus every saved row, sorted
/// by position then key.
pub async fn list(state: &AppState, access: &WorkspaceAccess) -> Result<Vec<FlowScene>, AppError> {
    let saved = repo::list(&state.db, access.workspace_id, access.user_id).await?;
    let mut out = dto::SCENES
        .iter()
        .filter(|s| !saved.iter().any(|r| r.scene == **s))
        .filter_map(|s| default_row(s))
        .map(|r| item(access, r, false))
        .collect::<Result<Vec<_>, _>>()?;
    for r in saved {
        out.push(item(access, r, true)?);
    }
    out.sort_by(|a, b| (a.position, &a.scene).cmp(&(b.position, &b.scene)));
    Ok(out)
}

/// Save the caller's settings for `scene`; omitted fields keep the saved value, else the
/// default. 403 when the config adds a component the caller can't read.
pub async fn save(
    state: &AppState,
    access: &WorkspaceAccess,
    scene: &str,
    input: SceneUpdate,
) -> Result<FlowScene, AppError> {
    dto::check_scene(scene)?;
    let (ws, user) = (access.workspace_id, access.user_id);
    let mut tx = state.db.begin().await?;
    // Serialize one member's saves so two new scenes at once can't pass the cap together.
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended('flow_scenes' || $1::uuid::text || $2::uuid::text, 0))",
        ws,
        user,
    )
    .execute(&mut *tx)
    .await?;
    let current = match repo::one(&mut *tx, ws, user, scene).await? {
        Some(row) => row,
        None => {
            if repo::count(&mut *tx, ws, user).await? >= MAX_PER_MEMBER {
                return Err(AppError::field(
                    "scene",
                    format!(
                        "You can keep at most {MAX_PER_MEMBER} saved scenes in this workspace."
                    ),
                ));
            }
            match default_row(scene) {
                Some(row) => row,
                None if input.config.is_none() => {
                    return Err(AppError::field(
                        "config",
                        "The config field is required for a new scene.",
                    ));
                }
                None => SceneRow {
                    scene: scene.to_owned(),
                    position: CUSTOM_POSITION,
                    hidden: false,
                    schema_version: 1,
                    config: serde_json::to_value(FlowSceneConfig::default()).unwrap_or_default(),
                },
            }
        }
    };
    let config = match input.config {
        Some(new) => {
            let had: FlowSceneConfig =
                serde_json::from_value(current.config.clone()).unwrap_or_default();
            let adds_restricted = new.components.iter().any(|c| {
                c.permission().is_some_and(|p| !access.allows(p))
                    && !had.components.iter().any(|o| o.same(c))
            });
            if adds_restricted {
                return Err(AppError::Forbidden(NOT_ALLOWED));
            }
            serde_json::to_value(new)
                .map_err(|e| AppError::Internal(format!("flow scene config: {e}")))?
        }
        None => current.config,
    };
    let row = SceneRow {
        scene: scene.to_owned(),
        position: input.position.unwrap_or(current.position),
        hidden: input.hidden.unwrap_or(current.hidden),
        schema_version: 1,
        config,
    };
    repo::upsert(&mut *tx, ws, user, &row).await?;
    tx.commit().await?;
    item(access, row, true)
}

/// Drop the caller's row for `scene`: built-ins fall back to default, custom ones go.
pub async fn delete(
    state: &AppState,
    access: &WorkspaceAccess,
    scene: &str,
) -> Result<(), AppError> {
    dto::check_scene(scene)?;
    Ok(repo::delete(&state.db, access.workspace_id, access.user_id, scene).await?)
}

/// Drop all the caller's rows here.
pub async fn reset(state: &AppState, access: &WorkspaceAccess) -> Result<(), AppError> {
    Ok(repo::reset(&state.db, access.workspace_id, access.user_id).await?)
}
