//! Bug report rules: anyone with a farm role files, as themselves; only farm admin reads; the
//! image type comes from the bytes; each image gets a fresh key in the reporter's own folder.

use std::{collections::BTreeMap, time::Duration};

use serde_json::Value;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::{
        farm_bug_reports::{
            dto::{BugReportResource, Form, MAX_ACTIONS},
            repo::{self, NewReport, ReportRow},
        },
        farm_documents::service::content_type,
        files::service::store,
    },
};

const IMAGES: &str = "must be a JPEG, PNG, WebP or HEIC image.";
const LINK_TTL: Duration = Duration::from_secs(10 * 60);
const ID_TAKEN: &str = "This id is already in use.";
/// Image types and the key extension each is stored under.
const EXTENSIONS: [(&str, &str); 4] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/heic", "heic"),
];

fn require_admin(access: &FarmAccess) -> Result<(), AppError> {
    if access.is_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden(NOT_ALLOWED))
    }
}

/// A checked text field, or its error.
fn text(
    errors: &mut BTreeMap<String, Vec<String>>,
    field: &str,
    value: Option<String>,
    max: usize,
) -> String {
    let v = value.unwrap_or_default();
    let f = field.replace('_', " ");
    if v.trim().is_empty() {
        errors.insert(field.into(), vec![format!("The {f} field is required.")]);
    } else if v.chars().count() > max {
        errors.insert(
            field.into(),
            vec![format!(
                "The {f} must not be greater than {max} characters."
            )],
        );
    }
    v
}

/// The image's content type, or its error.
fn image(
    errors: &mut BTreeMap<String, Vec<String>>,
    field: &str,
    bytes: &Option<Vec<u8>>,
) -> Option<&'static str> {
    let b = bytes.as_ref()?;
    let kind = content_type(b).filter(|ct| ct.starts_with("image/"));
    if kind.is_none() {
        errors.insert(field.into(), vec![format!("The {field} {IMAGES}")]);
    }
    kind
}

/// File a report. False = this id was filed before by the same reporter (a retry).
pub async fn create(
    state: &AppState,
    access: &FarmAccess,
    form: Form,
) -> Result<(Uuid, i64, bool), AppError> {
    let ws = access.workspace_id;
    let reporter = access.user_id;
    if let Some(id) = form.id
        && let Some((at, by, r#ref)) = repo::filed(&state.db, id).await?
    {
        return if (at, by) == (ws, reporter) {
            Ok((id, r#ref, false))
        } else {
            Err(AppError::Conflict(ID_TAKEN))
        };
    }
    let mut errors = BTreeMap::new();
    let route = text(&mut errors, "route", form.route, 500);
    let user_agent = text(&mut errors, "user_agent", form.user_agent, 500);
    let viewport = text(&mut errors, "viewport", form.viewport, 50);
    let description = text(&mut errors, "description", form.description, 5000);
    let actions: Value = match form.actions.as_deref().map(serde_json::from_str::<Value>) {
        Some(Ok(Value::Array(a))) if a.len() <= MAX_ACTIONS => Value::Array(a),
        Some(Ok(Value::Array(_))) => {
            errors.insert(
                "actions".into(),
                vec![format!(
                    "The actions must not have more than {MAX_ACTIONS} items."
                )],
            );
            Value::Null
        }
        _ => {
            errors.insert(
                "actions".into(),
                vec!["The actions must be a JSON array.".into()],
            );
            Value::Null
        }
    };
    let shot_type = image(&mut errors, "screenshot", &form.screenshot);
    let photo_type = image(&mut errors, "photo", &form.photo);
    if form.screenshot.is_none() && form.photo.is_none() {
        errors.insert(
            "screenshot".into(),
            vec!["A screenshot or a photo is required.".into()],
        );
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    let bucket = store(state)?;
    let id = form.id.unwrap_or_else(Uuid::now_v7);
    // The reporter's own folder; a fresh object per attempt, so nothing is ever replaced.
    let folder = format!("genesy/{ws}/farm/bug-reports/{reporter}/");
    let mut keys = Vec::new();
    for (bytes, kind) in [(form.screenshot, shot_type), (form.photo, photo_type)] {
        let key = match (bytes, kind) {
            (Some(bytes), Some(kind)) => {
                let ext = EXTENSIONS
                    .iter()
                    .find(|(ct, _)| *ct == kind)
                    .map_or("bin", |e| e.1);
                let key = format!("{folder}{id}-{}.{ext}", Uuid::now_v7());
                bucket.put(&key, bytes, kind, "bug-report", true).await?;
                Some(key)
            }
            _ => None,
        };
        keys.push(key);
    }
    let new = NewReport {
        id,
        reporter_id: reporter,
        route: &route,
        user_agent: &user_agent,
        viewport: &viewport,
        description: &description,
        actions: &actions,
        screenshot_key: keys[0].as_deref(),
        photo_key: keys[1].as_deref(),
    };
    let inserted = async {
        let mut tx = access.begin(&state.db).await?;
        let r#ref = repo::insert(&mut *tx, ws, &new).await?;
        tx.commit().await?;
        Ok::<_, AppError>(r#ref)
    }
    .await;
    match inserted {
        Ok(Some(r#ref)) => Ok((id, r#ref, true)),
        other => {
            // Lost a race or failed: this attempt's objects are nobody's files.
            for key in keys.iter().flatten() {
                if let Err(err) = bucket.delete(key).await {
                    tracing::warn!(report = %id, error = %err, "orphan bug report object: delete failed");
                }
            }
            match other {
                Ok(_) => Err(AppError::Conflict(ID_TAKEN)),
                Err(e) => Err(e),
            }
        }
    }
}

/// One page of reports (farm admin).
pub async fn list(
    state: &AppState,
    access: &FarmAccess,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ReportRow>, i64), AppError> {
    require_admin(access)?;
    Ok(repo::list(&state.db, access.workspace_id, None, limit, offset).await?)
}

/// One report with 10-minute image links (farm admin).
pub async fn get(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
) -> Result<BugReportResource, AppError> {
    require_admin(access)?;
    let id = Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let row = repo::list(&state.db, access.workspace_id, Some(id), 1, 0)
        .await?
        .0
        .pop()
        .ok_or(AppError::NotFound)?;
    let folder = format!(
        "genesy/{}/farm/bug-reports/{}/",
        access.workspace_id, row.reporter_id
    );
    let link = |key: &Option<String>| -> Result<Option<String>, AppError> {
        match key {
            // Belt and braces: never sign a key outside the reporter's folder.
            Some(k) if k.starts_with(&folder) => {
                let ct = EXTENSIONS
                    .iter()
                    .find(|(_, ext)| k.ends_with(&format!(".{ext}")))
                    .map_or("application/octet-stream", |e| e.0);
                Ok(Some(store(state)?.download_url(
                    k,
                    "bug-report",
                    ct,
                    true,
                    LINK_TTL,
                )?))
            }
            _ => Ok(None),
        }
    };
    let (shot, photo) = (link(&row.screenshot_key)?, link(&row.photo_key)?);
    Ok(BugReportResource::new(row, shot, photo))
}
