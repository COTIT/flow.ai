//! Dialogue ownership, atomic reconciliation and revision-aware history pages.

use super::{dto::*, repo};
use crate::{
    AppError,
    modules::assistant::{self, repo::Owner},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Create independent member-owned history record.
pub async fn create(db: &PgPool, o: Owner, i: CreateInput) -> Result<Value, AppError> {
    let title = super::redaction::archive_text(i.title.as_deref().unwrap_or(""));
    if title.len() > 200 {
        return Err(AppError::field(
            "title",
            "Redacted title must not exceed 200 bytes.",
        ));
    }
    let id = repo::create(db, o, &title).await?;
    get(db, o, id).await
}
/// Read own metadata without refreshing inactivity.
pub async fn get(db: &PgPool, o: Owner, id: Uuid) -> Result<Value, AppError> {
    repo::get(db, o, id).await?.ok_or(AppError::NotFound)
}
/// List bounded history; source-session filter requires current execution login.
pub async fn list(db: &PgPool, o: Owner, q: HistoryQuery) -> Result<Value, AppError> {
    q.check()?;
    let before = q
        .cursor
        .as_deref()
        .map(|cursor| {
            URL_SAFE_NO_PAD
                .decode(cursor)
                .ok()
                .and_then(|bytes| {
                    serde_json::from_slice::<(chrono::DateTime<chrono::Utc>, Uuid)>(&bytes).ok()
                })
                .ok_or(AppError::field("cursor", "Invalid history cursor."))
        })
        .transpose()?;
    let mut rows = repo::list(db, o, &q, before).await?;
    let more = rows.len() > q.limit.unwrap_or(50) as usize;
    rows.truncate(q.limit.unwrap_or(50) as usize);
    let cursor = if more {
        rows.last()
            .map(|r| URL_SAFE_NO_PAD.encode(format!("[{},{}]", r["last_activity_at"], r["id"])))
    } else {
        None
    };
    Ok(json!({"conversations":rows,"cursor":cursor,"has_more":more}))
}
fn live(c: &repo::Conversation) -> Result<(), AppError> {
    if !c.live {
        return Err(AppError::Conflict("This conversation has expired."));
    }
    Ok(())
}
/// End execution controls or erase content while preserving no-resurrection identities.
pub async fn end(db: &PgPool, o: Owner, id: Uuid, delete: bool) -> Result<Value, AppError> {
    let mut tx = db.begin().await?;
    let c = repo::lock(&mut *tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    live(&c)?;
    repo::end(&mut *tx, o, id, delete).await?;
    repo::close_sessions(&mut *tx, o, id).await?;
    if delete {
        repo::erase(&mut *tx, o.workspace_id, id).await?;
        repo::erase_summaries(&mut *tx, o.workspace_id, id).await?;
    }
    tx.commit().await?;
    if delete {
        Ok(json!({"id":id,"state":"deleted"}))
    } else {
        get(db, o, id).await
    }
}
/// Bind fixed provider identity only after signed execution attachment matched.
pub async fn source(db: &PgPool, o: Owner, id: Uuid, i: SourceInput) -> Result<Value, AppError> {
    let mut tx = db.begin().await?;
    repo::lock_source_owner(&mut *tx, o).await?;
    let c = repo::lock(&mut *tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    live(&c)?;
    if c.state != "active" {
        return Err(AppError::Conflict("This conversation has ended."));
    }
    let session = assistant::repo::lock_session(&mut tx, o, i.session_id)
        .await?
        .ok_or(AppError::NotFound)?;
    assistant::service::require_active(&session)?;
    if session.conversation_id.as_deref() != Some(&i.provider_conversation_id) {
        return Err(AppError::Conflict(
            "The provider conversation does not match this session.",
        ));
    }
    if let Some(mut current) =
        repo::source_for_session(&mut *tx, o.workspace_id, id, i.session_id).await?
    {
        if current["provider_conversation_id"] != i.provider_conversation_id
            || current["agent_id"] != i.agent_id
            || current["text_only"] != i.text_only
        {
            return Err(AppError::Conflict(
                "This source already has another provider binding.",
            ));
        }
        if let Some(object) = current.as_object_mut() {
            object.remove("provider_conversation_id");
            object.remove("agent_id");
            object.remove("text_only");
        }
        tx.commit().await?;
        return Ok(current);
    }
    if repo::competing_source(&mut *tx, o, i.session_id).await? {
        return Err(AppError::Conflict(
            "Another Flow call is active. End it before starting a new one.",
        ));
    }
    let source = repo::insert_source(&mut *tx, o, id, &i)
        .await
        .map_err(conflict)?;
    repo::bump(&mut *tx, o, id, false).await?;
    tx.commit().await?;
    Ok(source)
}
fn conflict(e: sqlx::Error) -> AppError {
    if e.as_database_error()
        .is_some_and(|e| e.is_unique_violation())
    {
        AppError::Conflict("This source or event identity is already bound.")
    } else {
        e.into()
    }
}
/// Store original-login events; final snapshots cannot be overwritten by late observations.
pub async fn events(
    db: &PgPool,
    o: Owner,
    id: Uuid,
    mut i: EventsInput,
) -> Result<Value, AppError> {
    for event in &mut i.events {
        event.text = super::redaction::archive_text(&event.text);
        if event.text.len() > 16384
            || serde_json::to_string(&event.text).map_or(true, |s| s.len() > 48 * 1024)
        {
            return Err(AppError::field(
                "text",
                "Redacted text must not exceed 16384 bytes.",
            ));
        }
    }
    let mut tx = db.begin().await?;
    let c = repo::lock(&mut *tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    live(&c)?;
    let source = repo::source(&mut *tx, o.workspace_id, id, i.source_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if source.session_id != i.session_id || source.login_id != o.login_id {
        return Err(AppError::NotFound);
    }
    let session = assistant::repo::session(&mut *tx, o, i.session_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if session.state == "cleaned" {
        return Err(AppError::Conflict("This source has been cleaned."));
    }
    let finalized = source.state == "final";
    let mut changed = false;
    let mut new_user = false;
    for event in &i.events {
        if finalized && event.status == "complete" {
            continue;
        }
        let previous =
            repo::event_before(&mut *tx, o.workspace_id, id, source.id, &event.key).await?;
        if let Some(before) = &previous {
            let ordinal = if finalized {
                event.ordinal + 1000000
            } else {
                event.ordinal
            };
            if before["role"] != event.role
                || before["modality"] != event.modality
                || before["ordinal"] != ordinal
            {
                return Err(AppError::Conflict(
                    "This event already has another identity.",
                ));
            }
        }
        let updated = repo::event(&mut *tx, o.workspace_id, id, source.id, event, finalized)
            .await
            .map_err(conflict)?;
        new_user |= updated && previous.is_none() && event.role == "user";
        changed |= updated;
    }
    if changed {
        repo::bump_source(&mut *tx, o.workspace_id, id, source.id).await?;
        repo::bump(&mut *tx, o, id, new_user).await?;
    }
    tx.commit().await?;
    get(db, o, id).await
}
/// Replace complete provisional rows with canonical provider text in one transaction.
pub async fn ingest(db: &PgPool, mut i: IngestInput) -> Result<Value, AppError> {
    for turn in &mut i.turns {
        turn.text = super::redaction::archive_text(&turn.text);
        if turn.text.len() > 16384
            || serde_json::to_string(&turn.text).map_or(true, |s| s.len() > 48 * 1024)
        {
            return Err(AppError::field(
                "text",
                "Redacted text must not exceed 16384 bytes.",
            ));
        }
    }
    i.summary = i
        .summary
        .map(|text| super::redaction::archive_text(&text))
        .filter(|text| !text.is_empty());
    let (workspace_id, id, user_id) = repo::provider(db, &i.provider_conversation_id, &i.agent_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let o = Owner {
        workspace_id,
        user_id,
        login_id: Uuid::nil(),
    };
    let mut tx = db.begin().await?;
    let c = repo::lock(&mut *tx, o, id)
        .await?
        .ok_or(AppError::Conflict("This conversation was deleted."))?;
    live(&c)?;
    let source = repo::provider_source(&mut *tx, workspace_id, id, &i.provider_conversation_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let canonical = serde_json::to_vec(&i)
        .map_err(|_| AppError::Internal("dialogue snapshot serialization failed".into()))?;
    if canonical.len() > 2 * 1024 * 1024 {
        return Err(AppError::PayloadTooLarge);
    }
    if i.summary.as_ref().is_some_and(|s| {
        s.len() > 16384 || serde_json::to_string(s).map_or(true, |s| s.len() > 48 * 1024)
    }) {
        return Err(AppError::field(
            "summary",
            "Redacted summary must not exceed 16384 bytes.",
        ));
    }
    let digest = Sha256::digest(&canonical);
    if let Some(old) = source.final_sha256 {
        if old.as_slice() != digest.as_slice() {
            return Err(AppError::Conflict(
                "This source already has another final snapshot.",
            ));
        }
        tx.commit().await?;
        return Ok(
            json!({"id":source.id,"conversation_id":id,"revision":c.revision,"state":"final"}),
        );
    }
    let had_user = repo::has_user(&mut *tx, workspace_id, id, source.id).await?;
    repo::clear_provisional(&mut *tx, workspace_id, id, source.id).await?;
    repo::pending_ordinals(&mut *tx, workspace_id, id, source.id).await?;
    for (ordinal, turn) in i.turns.iter().enumerate() {
        repo::final_turn(&mut *tx, workspace_id, id, source.id, ordinal as i64, turn).await?;
    }
    repo::finalize(
        &mut *tx,
        workspace_id,
        id,
        source.id,
        &digest,
        i.summary.as_deref(),
    )
    .await?;
    repo::bump(
        &mut *tx,
        o,
        id,
        !had_user && i.turns.iter().any(|t| t.role == "user"),
    )
    .await?;
    tx.commit().await?;
    Ok(json!({"id":source.id,"conversation_id":id,"revision":c.revision+1,"state":"final"}))
}
/// Return bounded exact messages under content revision lock.
pub async fn messages(db: &PgPool, o: Owner, id: Uuid, q: HistoryQuery) -> Result<Value, AppError> {
    q.check()?;
    let mut tx = db.begin().await?;
    let c = repo::lock(&mut *tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !c.live {
        return Err(AppError::NotFound);
    }
    if q.revision.is_some_and(|r| r != c.revision) {
        return Err(AppError::Conflict(
            "Conversation content changed. Restart pagination.",
        ));
    }
    let after = if let Some(cursor) = &q.cursor {
        let decoded = URL_SAFE_NO_PAD
            .decode(cursor)
            .map_err(|_| AppError::field("cursor", "Invalid history cursor."))?;
        let (cid, revision, s, t, key): (Uuid, i64, i64, i64, Uuid) =
            serde_json::from_slice(&decoded)
                .map_err(|_| AppError::field("cursor", "Invalid history cursor."))?;
        if cid != id {
            return Err(AppError::field("cursor", "Invalid history cursor."));
        }
        if revision != c.revision {
            return Err(AppError::Conflict(
                "Conversation content changed. Restart pagination.",
            ));
        }
        (s, t, key)
    } else {
        (0, 0, Uuid::nil())
    };
    let mut rows = repo::messages(&mut *tx, o, id, &q, after).await?;
    let limit = q.limit.unwrap_or(50) as usize;
    let mut more = rows.len() > limit;
    rows.truncate(limit);
    let gaps = repo::gaps(&mut *tx, o.workspace_id, id).await?;
    let through = repo::through(&mut *tx, o.workspace_id, id).await?;
    let mut payload;
    loop {
        let cursor = if more {
            rows.last().map(|r| {
                URL_SAFE_NO_PAD.encode(format!(
                    "[\"{id}\",{},{},{},\"{}\"]",
                    c.revision, r.source_ordinal, r.ordinal, r.id
                ))
            })
        } else {
            None
        };
        let messages: Vec<_> = if q.tail {
            rows.iter().rev().map(|r| &r.data).collect()
        } else {
            rows.iter().map(|r| &r.data).collect()
        };
        payload = json!({"messages":messages,"revision":c.revision,"cursor":cursor,"has_more":more,"through_sequence":through,"gaps":gaps});
        if serde_json::to_vec(&json!({"data":payload}))
            .map_err(|_| AppError::Internal("dialogue page serialization failed".into()))?
            .len()
            <= 65536
        {
            break;
        }
        if rows.pop().is_none() {
            return Err(AppError::Internal(
                "dialogue page metadata exceeds limit".into(),
            ));
        }
        more = true;
    }
    tx.commit().await?;
    Ok(payload)
}

/// Tombstone inactive dialogue and erase body while preserving provider cleanup metadata.
pub async fn prune(db: &PgPool) -> Result<(), sqlx::Error> {
    // shortcut: 100 conversations/hour; raise batch before expiry exceeds 2400/day.
    for (workspace_id, user_id, id) in repo::expired(db).await? {
        let o = Owner {
            workspace_id,
            user_id,
            login_id: Uuid::nil(),
        };
        let mut tx = db.begin().await?;
        if repo::lock(&mut *tx, o, id).await?.is_some_and(|c| !c.live) {
            repo::end(&mut *tx, o, id, true).await?;
            repo::close_sessions(&mut *tx, o, id).await?;
            repo::erase(&mut *tx, workspace_id, id).await?;
            repo::erase_summaries(&mut *tx, workspace_id, id).await?;
        }
        tx.commit().await?;
    }
    Ok(())
}

/// Detail metadata and bounded source-summary directory; reads never refresh inactivity.
pub async fn detail(db: &PgPool, o: Owner, id: Uuid, q: DetailQuery) -> Result<Value, AppError> {
    let limit = q.source_limit.unwrap_or(50);
    if !(1..=100).contains(&limit) || q.source_before.is_some_and(|n| n <= 0) {
        return Err(AppError::field(
            "source_limit",
            "Choose source limit 1 to 100 and positive source cursor.",
        ));
    }
    let mut tx = db.begin().await?;
    let c = repo::lock(&mut *tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !c.live {
        return Err(AppError::NotFound);
    }
    let conversation = repo::get(&mut *tx, o, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut sources =
        repo::source_page(&mut *tx, o.workspace_id, id, q.source_before, limit).await?;
    let mut more = sources.len() > limit as usize;
    sources.truncate(limit as usize);
    let payload = loop {
        let cursor = if more {
            sources.last().and_then(|v| v["ordinal"].as_i64())
        } else {
            None
        };
        let ordered: Vec<_> = sources.iter().rev().collect();
        let payload = json!({"conversation":conversation,"sources":ordered,"sources_has_more":more,"next_source_cursor":cursor});
        if serde_json::to_vec(&json!({"data":payload}))
            .map_err(|_| AppError::Internal("dialogue source page serialization failed".into()))?
            .len()
            <= 65536
        {
            break payload;
        }
        if sources.pop().is_none() {
            return Err(AppError::Internal(
                "dialogue source metadata exceeds limit".into(),
            ));
        }
        more = true;
    };
    tx.commit().await?;
    Ok(payload)
}

/// Reconcile terminal native transcripts before provider cleanup; every failure remains retryable.
pub async fn reconcile(ctx: &crate::jobs::kinds::Ctx) -> Result<(), AppError> {
    use crate::http::json::{Rules, Validate};
    let Some(key) = ctx.assistant_api_key.as_ref() else {
        return Ok(());
    };
    for (provider, agent) in repo::native_pending(&ctx.db).await? {
        let url = crate::modules::assistant::cleanup::conversation_url(&provider)?;
        let request = ctx
            .http
            .request(reqwest::Method::GET, &url)
            .header("xi-api-key", key.expose())
            .timeout(std::time::Duration::from_secs(10));
        let Ok(mut response) = ctx.http.send(request).await else {
            continue;
        };
        if response.status() != reqwest::StatusCode::OK
            || response
                .content_length()
                .is_some_and(|n| n > 2 * 1024 * 1024)
        {
            continue;
        }
        let mut bytes = Vec::new();
        let mut valid = true;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) if bytes.len() + chunk.len() <= 2 * 1024 * 1024 => {
                    bytes.extend_from_slice(&chunk)
                }
                Ok(None) => break,
                _ => {
                    valid = false;
                    break;
                }
            }
        }
        if !valid {
            continue;
        }
        let Some(value) = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| super::native_parser::normalize(&v, &provider, &agent))
        else {
            continue;
        };
        let Ok(input) = serde_json::from_value::<IngestInput>(value) else {
            continue;
        };
        let mut rules = Rules::default();
        let Some(input) = input.validate(&mut rules) else {
            continue;
        };
        if rules.into_result().is_err() {
            continue;
        }
        if ingest(&ctx.db, input).await.is_err() {
            tracing::warn!("assistant native transcript reconciliation remains pending");
        }
    }
    Ok(())
}
