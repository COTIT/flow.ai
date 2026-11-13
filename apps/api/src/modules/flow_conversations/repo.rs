//! Workspace-filtered dialogue storage and ordered transcript queries.

use super::dto::{Event, FinalTurn, HistoryQuery, SourceInput};
use crate::modules::assistant::repo::Owner;
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Locked logical conversation state.
pub struct Conversation {
    pub revision: i64,
    pub state: String,
    pub live: bool,
}
/// Trusted immutable source binding.
pub struct Source {
    pub id: Uuid,
    pub session_id: Uuid,
    pub login_id: Uuid,
    pub state: String,
    pub final_sha256: Option<Vec<u8>>,
}
/// One ordered transcript row and public payload.
pub struct Message {
    pub source_ordinal: i64,
    pub ordinal: i64,
    pub id: Uuid,
    pub data: Value,
}

/// Insert fresh dialogue owned independently from execution login.
pub async fn create(db: impl PgExecutor<'_>, o: Owner, title: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!("INSERT INTO assistant_conversations(workspace_id,user_id,title) VALUES($1,$2,$3) RETURNING id",o.workspace_id,o.user_id,title).fetch_one(db).await
}
/// Lock only caller-owned, nondeleted conversation, including expired state for refusal.
pub async fn lock(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as!(Conversation,r#"SELECT revision,state,expires_at>now() AS "live!" FROM assistant_conversations WHERE workspace_id=$1 AND user_id=$2 AND id=$3 AND state<>'deleted' FOR UPDATE"#,o.workspace_id,o.user_id,id).fetch_optional(db).await
}
/// Public bounded metadata; provider and execution identifiers stay private.
pub async fn get(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',c.id,'title',c.title,'state',c.state,'revision',c.revision,'coverage',CASE WHEN EXISTS(SELECT 1 FROM assistant_conversation_sources s WHERE s.workspace_id=c.workspace_id AND s.conversation_id=c.id AND s.state='partial') THEN 'partial' WHEN EXISTS(SELECT 1 FROM assistant_conversation_sources s WHERE s.workspace_id=c.workspace_id AND s.conversation_id=c.id AND s.state='provisional') THEN 'provisional' ELSE 'complete' END,'last_activity_at',c.last_activity_at,'expires_at',c.expires_at) AS "data!" FROM assistant_conversations c WHERE c.workspace_id=$1 AND c.user_id=$2 AND c.id=$3 AND c.state<>'deleted' AND c.expires_at>now()"#,o.workspace_id,o.user_id,id).fetch_optional(db).await
}
/// Bounded owner history and optional original-session lookup.
pub async fn list(
    db: impl PgExecutor<'_>,
    o: Owner,
    q: &HistoryQuery,
    before: Option<(chrono::DateTime<chrono::Utc>, Uuid)>,
) -> Result<Vec<Value>, sqlx::Error> {
    let pattern = q.q.as_ref().map(|s| {
        format!(
            "%{}%",
            s.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',c.id,'title',c.title,'state',c.state,'revision',c.revision,'last_activity_at',c.last_activity_at,'expires_at',c.expires_at,'coverage',CASE WHEN EXISTS(SELECT 1 FROM assistant_conversation_sources s WHERE s.workspace_id=c.workspace_id AND s.conversation_id=c.id AND s.state='partial') THEN 'partial' WHEN EXISTS(SELECT 1 FROM assistant_conversation_sources s WHERE s.workspace_id=c.workspace_id AND s.conversation_id=c.id AND s.state='provisional') THEN 'provisional' ELSE 'complete' END) AS "data!" FROM assistant_conversations c WHERE c.workspace_id=$1 AND c.user_id=$2 AND c.state<>'deleted' AND c.expires_at>now() AND ($3::timestamptz IS NULL OR (c.last_activity_at,c.id)<($3,$4)) AND ($5::text IS NULL OR c.title ILIKE $5 OR EXISTS(SELECT 1 FROM assistant_conversation_turns t WHERE t.workspace_id=c.workspace_id AND t.conversation_id=c.id AND t.text ILIKE $5)) AND ($6::uuid IS NULL OR EXISTS(SELECT 1 FROM assistant_conversation_sources s JOIN assistant_sessions a ON a.workspace_id=s.workspace_id AND a.id=s.session_id WHERE s.workspace_id=c.workspace_id AND s.conversation_id=c.id AND s.session_id=$6 AND s.login_id=$7 AND a.user_id=$2)) ORDER BY c.last_activity_at DESC,c.id DESC LIMIT $8"#,o.workspace_id,o.user_id,before.map(|v|v.0),before.map(|v|v.1),pattern.as_deref(),q.session_id,o.login_id,q.limit.unwrap_or(50)+1).fetch_all(db).await
}
/// Increment content revision; only new user dialogue extends inactivity window.
pub async fn bump(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    user_dialogue: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE assistant_conversations SET revision=revision+1,last_activity_at=CASE WHEN $4 THEN now() ELSE last_activity_at END,expires_at=CASE WHEN $4 THEN now()+interval '90 days' ELSE expires_at END WHERE workspace_id=$1 AND user_id=$2 AND id=$3",o.workspace_id,o.user_id,id,user_dialogue).execute(db).await?;
    Ok(())
}
/// End or tombstone while retaining immutable provider identities.
pub async fn end(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    delete: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE assistant_conversations SET state=CASE WHEN $4 THEN 'deleted' ELSE 'ended' END,deleted_at=CASE WHEN $4 THEN coalesce(deleted_at,now()) ELSE deleted_at END,revision=revision+1,title=CASE WHEN $4 THEN '' ELSE title END WHERE workspace_id=$1 AND user_id=$2 AND id=$3 AND state<>CASE WHEN $4 THEN 'deleted' ELSE 'ended' END",o.workspace_id,o.user_id,id,delete).execute(db).await?;
    Ok(())
}
/// Close linked execution controls without granting caller that login.
pub async fn close_sessions(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE assistant_sessions a SET state='closed',closed_at=coalesce(closed_at,now()) WHERE a.workspace_id=$1 AND a.user_id=$2 AND a.state='active' AND EXISTS(SELECT 1 FROM assistant_conversation_sources s WHERE s.workspace_id=a.workspace_id AND s.conversation_id=$3 AND s.session_id=a.id)",o.workspace_id,o.user_id,id).execute(db).await?;
    Ok(())
}
/// Erase transcript body immediately on tombstone.
pub async fn erase(db: impl PgExecutor<'_>, workspace: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM assistant_conversation_turns WHERE workspace_id=$1 AND conversation_id=$2",
        workspace,
        id
    )
    .execute(db)
    .await?;
    Ok(())
}
/// Strip retained source summaries when deleting content.
pub async fn erase_summaries(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE assistant_conversation_sources SET summary=NULL WHERE workspace_id=$1 AND conversation_id=$2",workspace,id).execute(db).await?;
    Ok(())
}
/// Look up source within locked owned conversation.
pub async fn source(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    source: Uuid,
) -> Result<Option<Source>, sqlx::Error> {
    sqlx::query_as!(Source,"SELECT id,session_id,login_id,state,final_sha256 FROM assistant_conversation_sources WHERE workspace_id=$1 AND conversation_id=$2 AND id=$3",workspace,id,source).fetch_optional(db).await
}
/// Resolve signature-authenticated provider identity before applying workspace filters.
pub async fn provider(
    db: impl PgExecutor<'_>,
    provider: &str,
    agent: &str,
) -> Result<Option<(Uuid, Uuid, Uuid)>, sqlx::Error> {
    let row=sqlx::query!("SELECT s.workspace_id,s.conversation_id,c.user_id FROM assistant_conversation_sources s JOIN assistant_conversations c ON c.workspace_id=s.workspace_id AND c.id=s.conversation_id WHERE s.provider_conversation_id=$1 AND s.agent_id=$2",provider,agent).fetch_optional(db).await?;
    Ok(row.map(|r| (r.workspace_id, r.conversation_id, r.user_id)))
}
/// Source provider identity scoped to resolved workspace.
pub async fn provider_source(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    provider: &str,
) -> Result<Option<Source>, sqlx::Error> {
    sqlx::query_as!(Source,"SELECT id,session_id,login_id,state,final_sha256 FROM assistant_conversation_sources WHERE workspace_id=$1 AND conversation_id=$2 AND provider_conversation_id=$3",workspace,id,provider).fetch_optional(db).await
}
/// Insert source only after original-login session was locked and provider attachment matched.
pub async fn insert_source(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    i: &SourceInput,
) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar!(r#"INSERT INTO assistant_conversation_sources(workspace_id,conversation_id,session_id,login_id,provider_conversation_id,agent_id,text_only,ordinal) SELECT $1,$2,$3,$4,$5,$6,$7,coalesce(max(ordinal),0)+1 FROM assistant_conversation_sources WHERE workspace_id=$1 AND conversation_id=$2 RETURNING jsonb_build_object('id',id,'conversation_id',conversation_id,'session_id',session_id,'ordinal',ordinal,'revision',revision,'state',state) AS "data!""#,o.workspace_id,id,i.session_id,o.login_id,i.provider_conversation_id,i.agent_id,i.text_only).fetch_one(db).await
}
/// Retrieve same source attachment for idempotent retry.
pub async fn source_for_session(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    session: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',id,'conversation_id',conversation_id,'session_id',session_id,'ordinal',ordinal,'revision',revision,'state',state,'provider_conversation_id',provider_conversation_id,'agent_id',agent_id,'text_only',text_only) AS "data!" FROM assistant_conversation_sources WHERE workspace_id=$1 AND conversation_id=$2 AND session_id=$3"#,workspace,id,session).fetch_optional(db).await
}
/// Upsert original provisional evidence; complete and delivery states never regress.
pub async fn event(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    source: Uuid,
    e: &Event,
    finalized: bool,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!("INSERT INTO assistant_conversation_turns(workspace_id,conversation_id,source_id,key,ordinal,role,text,modality,status,interrupted,provenance) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'provisional') ON CONFLICT(workspace_id,source_id,key,provenance) DO UPDATE SET text=EXCLUDED.text,status=EXCLUDED.status,interrupted=assistant_conversation_turns.interrupted OR EXCLUDED.interrupted WHERE assistant_conversation_turns.workspace_id=$1 AND assistant_conversation_turns.conversation_id=$2 AND assistant_conversation_turns.role=EXCLUDED.role AND assistant_conversation_turns.modality=EXCLUDED.modality AND assistant_conversation_turns.ordinal=EXCLUDED.ordinal AND (CASE assistant_conversation_turns.status WHEN 'queued' THEN 0 WHEN 'sent_unconfirmed' THEN 1 ELSE 2 END)<=(CASE EXCLUDED.status WHEN 'queued' THEN 0 WHEN 'sent_unconfirmed' THEN 1 ELSE 2 END) AND (assistant_conversation_turns.text,assistant_conversation_turns.status,assistant_conversation_turns.interrupted) IS DISTINCT FROM (EXCLUDED.text,EXCLUDED.status,assistant_conversation_turns.interrupted OR EXCLUDED.interrupted)",workspace,id,source,e.key,if finalized{e.ordinal+1000000}else{e.ordinal},e.role,e.text,e.modality,e.status,e.interrupted).execute(db).await?.rows_affected()>0)
}
/// Whether source already contains any user dialogue before canonical replacement.
pub async fn has_user(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    source: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM assistant_conversation_turns WHERE workspace_id=$1 AND conversation_id=$2 AND source_id=$3 AND role='user') AS "yes!""#,workspace,id,source).fetch_one(db).await
}
/// Remove provisional complete text while retaining separate delivery-uncertain evidence.
pub async fn clear_provisional(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    source: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM assistant_conversation_turns WHERE workspace_id=$1 AND conversation_id=$2 AND source_id=$3 AND status='complete'",workspace,id,source).execute(db).await?;
    Ok(())
}
/// Move retained pending evidence after canonical ordinals without guessing text matches.
pub async fn pending_ordinals(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    source: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE assistant_conversation_turns SET ordinal=ordinal+1000000 WHERE workspace_id=$1 AND conversation_id=$2 AND source_id=$3 AND provenance='provisional' AND ordinal<1000000",workspace,id,source).execute(db).await?;
    Ok(())
}
/// Store one canonical provider turn inside snapshot transaction.
pub async fn final_turn(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    source: Uuid,
    ordinal: i64,
    t: &FinalTurn,
) -> Result<(), sqlx::Error> {
    sqlx::query!("INSERT INTO assistant_conversation_turns(workspace_id,conversation_id,source_id,key,ordinal,role,text,modality,status,interrupted,provenance) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'complete',$9,'final')",workspace,id,source,t.key,ordinal,t.role,t.text,t.modality,t.interrupted).execute(db).await?;
    Ok(())
}
/// Record canonical source digest and summary exactly once.
pub async fn finalize(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    source: Uuid,
    digest: &[u8],
    summary: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE assistant_conversation_sources SET state='final',revision=revision+1,final_sha256=$4,summary=$5 WHERE workspace_id=$1 AND conversation_id=$2 AND id=$3",workspace,id,source,digest,summary).execute(db).await?;
    Ok(())
}
/// Bump source revision after changed provisional evidence.
pub async fn bump_source(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    source: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE assistant_conversation_sources SET revision=revision+1 WHERE workspace_id=$1 AND conversation_id=$2 AND id=$3",workspace,id,source).execute(db).await?;
    Ok(())
}
/// Indexed bounded transcript lookup; caller verifies revision under conversation lock.
pub async fn messages(
    db: impl PgExecutor<'_>,
    o: Owner,
    id: Uuid,
    q: &HistoryQuery,
    after: (i64, i64, Uuid),
) -> Result<Vec<Message>, sqlx::Error> {
    let pattern = q.q.as_ref().map(|s| {
        format!(
            "%{}%",
            s.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });
    sqlx::query_as!(Message,r#"SELECT s.ordinal AS source_ordinal,t.ordinal,t.id,jsonb_build_object('id',t.id,'source_id',t.source_id,'key',t.key,'ordinal',t.ordinal,'role',t.role,'text',t.text,'modality',t.modality,'status',t.status,'interrupted',t.interrupted,'provenance',t.provenance) AS "data!" FROM assistant_conversation_turns t JOIN assistant_conversation_sources s ON s.workspace_id=t.workspace_id AND s.conversation_id=t.conversation_id AND s.id=t.source_id WHERE t.workspace_id=$1 AND t.conversation_id=$2 AND ($3::uuid IS NULL OR t.source_id=$3) AND ($4::text IS NULL OR t.text ILIKE $4) AND (NOT $5 OR t.status='complete') AND (($6::bigint=0) OR (NOT $5 AND (s.ordinal,t.ordinal,t.id)>($6,$7,$8)) OR ($5 AND (s.ordinal,t.ordinal,t.id)<($6,$7,$8))) ORDER BY CASE WHEN $5 THEN -s.ordinal ELSE s.ordinal END,CASE WHEN $5 THEN -t.ordinal ELSE t.ordinal END,CASE WHEN $5 THEN NULL ELSE t.id END ASC, t.id DESC LIMIT $9"#,o.workspace_id,id,q.source_id,pattern.as_deref(),q.tail,after.0,after.1,after.2,q.limit.unwrap_or(50)+1).fetch_all(db).await
}
/// Full transcript high-water mark, independent of requested page/search.
pub async fn through(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT (s.ordinal::text||':'||t.ordinal::text) AS "sequence!" FROM assistant_conversation_turns t JOIN assistant_conversation_sources s ON s.workspace_id=t.workspace_id AND s.conversation_id=t.conversation_id AND s.id=t.source_id WHERE t.workspace_id=$1 AND t.conversation_id=$2 ORDER BY s.ordinal DESC,t.ordinal DESC,t.id DESC LIMIT 1"#,workspace,id).fetch_optional(db).await
}
/// Missing canonical sources remain explicit gaps.
pub async fn gaps(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('source_id',id,'state',state) AS "data!" FROM assistant_conversation_sources WHERE workspace_id=$1 AND conversation_id=$2 AND state<>'final' ORDER BY ordinal DESC LIMIT 100"#,workspace,id).fetch_all(db).await
}

/// Previous provisional event identity for exact retry and inactivity accounting.
pub async fn event_before(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    source: Uuid,
    key: &str,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('role',role,'modality',modality,'ordinal',ordinal) AS "data!" FROM assistant_conversation_turns WHERE workspace_id=$1 AND conversation_id=$2 AND source_id=$3 AND key=$4 AND provenance='provisional'"#,workspace,id,source,key).fetch_optional(db).await
}

/// Bounded expiry candidates; service locks each record before tombstoning.
pub async fn expired(db: impl PgExecutor<'_>) -> Result<Vec<(Uuid, Uuid, Uuid)>, sqlx::Error> {
    let rows=sqlx::query!("SELECT workspace_id,user_id,id FROM assistant_conversations WHERE state<>'deleted' AND expires_at<=now() ORDER BY expires_at,id LIMIT 100").fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.workspace_id, r.user_id, r.id))
        .collect())
}

/// Bounded source-summary directory, latest selection returned in source order by service.
pub async fn source_page(
    db: impl PgExecutor<'_>,
    workspace: Uuid,
    id: Uuid,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',s.id,'conversation_id',s.conversation_id,'ordinal',s.ordinal,'revision',s.revision,'state',s.state,'text_only',s.text_only,'summary',s.summary,'provenance',CASE WHEN s.state='final' THEN 'final' ELSE 'provisional' END) AS "data!" FROM assistant_conversation_sources s WHERE s.workspace_id=$1 AND s.conversation_id=$2 AND ($3::bigint IS NULL OR s.ordinal<$3) ORDER BY s.ordinal DESC LIMIT $4"#,workspace,id,before,limit+1).fetch_all(db).await
}

/// Claim bounded unfinalized sources; attempted rows rotate behind untouched work.
pub async fn native_pending(db: impl PgExecutor<'_>) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows=sqlx::query!("WITH picked AS (SELECT s.workspace_id,s.id FROM assistant_conversation_sources s JOIN assistant_conversations c ON c.workspace_id=s.workspace_id AND c.id=s.conversation_id JOIN assistant_sessions a ON a.workspace_id=s.workspace_id AND a.id=s.session_id WHERE c.state<>'deleted' AND c.expires_at>now() AND s.state<>'final' AND s.remote_deleted_at IS NULL AND a.state<>'cleaned' AND (a.state='closed' OR a.expires_at<=now() OR a.last_seen_at<=now()-interval '15 minutes' OR NOT EXISTS(SELECT 1 FROM access_tokens t WHERE t.id=a.login_id AND t.expires_at>now())) ORDER BY s.last_reconcile_attempt_at NULLS FIRST,s.created_at,s.id LIMIT 20 FOR UPDATE OF s SKIP LOCKED) UPDATE assistant_conversation_sources s SET last_reconcile_attempt_at=now() FROM picked p WHERE s.workspace_id=p.workspace_id AND s.id=p.id RETURNING s.provider_conversation_id,s.agent_id").fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.provider_conversation_id, r.agent_id))
        .collect())
}

/// Serialize history-source attachment across this member's logins and logical conversations.
pub async fn lock_source_owner(db: impl PgExecutor<'_>, o: Owner) -> Result<(), sqlx::Error> {
    sqlx::query!("SELECT pg_advisory_xact_lock(hashtextextended('flow_conversation_source|' || $1::uuid::text || $2::uuid::text,0))",o.workspace_id,o.user_id).execute(db).await?;
    Ok(())
}
/// Any other live history execution owned by this member; legacy unlinked sessions stay outside.
pub async fn competing_source(
    db: impl PgExecutor<'_>,
    o: Owner,
    session: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM assistant_conversation_sources s JOIN assistant_sessions a ON a.workspace_id=s.workspace_id AND a.id=s.session_id JOIN access_tokens t ON t.id=a.login_id AND t.user_id=a.user_id WHERE s.workspace_id=$1 AND a.user_id=$2 AND a.id<>$3 AND a.state='active' AND a.expires_at>now() AND a.last_seen_at>now()-interval '15 minutes' AND t.expires_at>now()) AS "active!""#,o.workspace_id,o.user_id,session).fetch_one(db).await
}

#[cfg(test)]
mod fairness_tests {
    use super::*;
    use sqlx::PgPool;
    #[sqlx::test]
    async fn native_reconciliation_turns_past_failed_first_chunk(db: PgPool) {
        let user:Uuid=sqlx::query_scalar("INSERT INTO users(email,first_name,last_name,password_hash) VALUES('fair@example.test','A','B','h') RETURNING id").fetch_one(&db).await.unwrap();
        let workspace: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces(name,created_by_user_id) VALUES('fair',$1) RETURNING id",
        )
        .bind(user)
        .fetch_one(&db)
        .await
        .unwrap();
        let conversation: Uuid = sqlx::query_scalar(
            "INSERT INTO assistant_conversations(workspace_id,user_id) VALUES($1,$2) RETURNING id",
        )
        .bind(workspace)
        .bind(user)
        .fetch_one(&db)
        .await
        .unwrap();
        for index in 0..25 {
            let session:Uuid=sqlx::query_scalar("INSERT INTO assistant_sessions(workspace_id,user_id,login_id,language,state,closed_at,last_seen_at) VALUES($1,$2,uuidv7(),'en','closed',now()-interval '2 hours',now()-interval '2 hours') RETURNING id").bind(workspace).bind(user).fetch_one(&db).await.unwrap();
            sqlx::query("INSERT INTO assistant_conversation_sources(workspace_id,conversation_id,session_id,login_id,provider_conversation_id,agent_id,text_only,ordinal) VALUES($1,$2,$3,uuidv7(),$4,'agent_fair',false,$5)").bind(workspace).bind(conversation).bind(session).bind(format!("conv_fair_{index}")).bind(index+1_i64).execute(&db).await.unwrap();
        }
        let first = native_pending(&db).await.unwrap();
        assert_eq!(first.len(), 20);
        let second = native_pending(&db).await.unwrap();
        assert_eq!(
            second.iter().filter(|row| !first.contains(row)).count(),
            5,
            "Failed oldest chunk must not starve remaining sources"
        );
        let first = crate::modules::assistant::repo::cleanup_due(&db)
            .await
            .unwrap();
        let ids: Vec<_> = first
            .iter()
            .take(20)
            .map(|row| Uuid::parse_str(row["session_id"].as_str().unwrap()).unwrap())
            .collect();
        for id in &ids {
            assert!(
                crate::modules::assistant::repo::cleanup_attempted(&db, workspace, *id)
                    .await
                    .unwrap()
            );
        }
        let next = crate::modules::assistant::repo::cleanup_due(&db)
            .await
            .unwrap();
        assert_eq!(
            next.iter()
                .take(20)
                .filter(|row| !ids
                    .contains(&Uuid::parse_str(row["session_id"].as_str().unwrap()).unwrap()))
                .count(),
            5,
            "Failed provider deletes must not starve remaining sessions"
        );
    }
}
