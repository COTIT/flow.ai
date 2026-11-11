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

/// A scene's settings with its config read.
struct Settings {
    position: i16,
    hidden: bool,
    config: FlowSceneConfig,
}

/// A saved row's settings; None (logged, ids only) when its config no longer fits the
/// vocabulary, so one stale row never fails the read.
fn stored(access: &WorkspaceAccess, row: SceneRow) -> Option<(String, Settings)> {
    match serde_json::from_value::<FlowSceneConfig>(row.config) {
        Ok(config) if dto::readable(&config) => Some((
            row.scene,
            Settings {
                position: row.position,
                hidden: row.hidden,
                config,
            },
        )),
        _ => {
            tracing::warn!(
                workspace_id = %access.workspace_id,
                user_id = %access.user_id,
                scene = %row.scene,
                "flow scene config unreadable; using default"
            );
            None
        }
    }
}

fn default_settings(scene: &str) -> Option<Settings> {
    dto::default(scene).map(|(position, config)| Settings {
        position,
        hidden: false,
        config,
    })
}

fn item(access: &WorkspaceAccess, scene: &str, s: Settings, saved: bool) -> FlowScene {
    let restricted = s
        .config
        .components
        .iter()
        .filter(|c| c.permission().is_some_and(|p| !access.allows(p)))
        .map(|c| c.id.clone())
        .collect();
    FlowScene {
        scene: scene.to_owned(),
        position: s.position,
        hidden: s.hidden,
        saved,
        schema_version: 1,
        config: s.config,
        restricted,
    }
}

/// The caller's scenes here: defaults for built-ins not saved, plus every readable saved
/// row, sorted by position then key.
pub async fn list(state: &AppState, access: &WorkspaceAccess) -> Result<Vec<FlowScene>, AppError> {
    let saved: Vec<_> = repo::list(&state.db, access.workspace_id, access.user_id)
        .await?
        .into_iter()
        .filter_map(|r| stored(access, r))
        .collect();
    let mut out: Vec<_> = dto::SCENES
        .iter()
        .filter(|s| !saved.iter().any(|(k, _)| k == *s))
        .filter_map(|s| default_settings(s).map(|d| item(access, s, d, false)))
        .collect();
    out.extend(saved.into_iter().map(|(k, s)| item(access, &k, s, true)));
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
    let row = repo::one(&mut *tx, ws, user, scene).await?;
    let exists = row.is_some();
    if !exists && repo::count(&mut *tx, ws, user).await? >= MAX_PER_MEMBER {
        return Err(AppError::field(
            "scene",
            format!("You can keep at most {MAX_PER_MEMBER} saved scenes in this workspace."),
        ));
    }
    let current = match row.and_then(|r| stored(access, r)) {
        Some((_, s)) => s,
        None => match default_settings(scene) {
            Some(s) => s,
            None if input.config.is_none() => {
                return Err(AppError::field(
                    "config",
                    "The config field is required for a new scene.",
                ));
            }
            None => Settings {
                position: CUSTOM_POSITION,
                hidden: false,
                config: FlowSceneConfig::default(),
            },
        },
    };
    let config = match input.config {
        Some(new) => {
            let adds_restricted = new.components.iter().any(|c| {
                c.permission().is_some_and(|p| !access.allows(p))
                    && !current.config.components.iter().any(|o| o.same(c))
            });
            if adds_restricted {
                return Err(AppError::Forbidden(NOT_ALLOWED));
            }
            new
        }
        None => current.config,
    };
    let next = Settings {
        position: input.position.unwrap_or(current.position),
        hidden: input.hidden.unwrap_or(current.hidden),
        config,
    };
    let value = serde_json::to_value(&next.config)
        .map_err(|e| AppError::Internal(format!("flow scene config: {e}")))?;
    let saved = SceneRow {
        scene: scene.to_owned(),
        position: next.position,
        hidden: next.hidden,
        config: value,
    };
    repo::upsert(&mut *tx, ws, user, &saved).await?;
    tx.commit().await?;
    Ok(item(access, scene, next, true))
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
