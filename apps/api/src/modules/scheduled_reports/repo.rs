//! scheduled_reports SQL. Owner rows filter by `workspace_id` and `user_id`; slot math runs in
//! Postgres (`AT TIME ZONE`), so DST follows the zone database.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

/// When a schedule is due: local `hour` on every day, one ISO weekday (1 = Monday) or one
/// day of the month (1..=28).
#[derive(Debug, Clone)]
pub struct Rule {
    /// `daily`, `weekly` or `monthly`.
    pub frequency: String,
    /// ISO weekday, weekly only.
    pub weekday: Option<i16>,
    /// Day of the month, monthly only.
    pub day_of_month: Option<i16>,
    /// Local hour, 0..24.
    pub hour: i16,
}

/// A schedule as its owner sees it.
pub struct ScheduleRow {
    pub id: Uuid,
    pub report: String,
    pub frequency: String,
    pub weekday: Option<i16>,
    pub day_of_month: Option<i16>,
    pub hour: i16,
    pub active: bool,
    pub paused_reason: Option<String>,
    pub next_run_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    /// Owner's login email: the only recipient.
    pub email: String,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_outcome: Option<String>,
}

/// A due schedule claimed by the runner.
pub struct Due {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub report: String,
    pub frequency: String,
    pub weekday: Option<i16>,
    pub day_of_month: Option<i16>,
    pub hour: i16,
}

impl Due {
    /// Its slot rule.
    pub fn rule(&self) -> Rule {
        Rule {
            frequency: self.frequency.clone(),
            weekday: self.weekday,
            day_of_month: self.day_of_month,
            hour: self.hour,
        }
    }
}

/// The workspace's `general.timezone`; UTC when unset or unknown to Postgres.
pub async fn zone(db: impl PgExecutor<'_>, workspace_id: Uuid) -> Result<String, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT coalesce((SELECT z.name FROM pg_timezone_names z
                            WHERE z.name = (SELECT s.data -> 'general' ->> 'timezone'
                                            FROM settings s WHERE s.workspace_id = $1)),
                           'UTC') AS "name!""#,
        workspace_id,
    )
    .fetch_one(db)
    .await
}

/// First slot of `rule` strictly after `after`, in the workspace zone.
pub async fn next_run(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    rule: &Rule,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>, sqlx::Error> {
    // 62 local days cover the longest gap (monthly: under 32 days to the next 28th).
    sqlx::query_scalar!(
        r#"WITH tz AS (
               SELECT coalesce((SELECT z.name FROM pg_timezone_names z
                                WHERE z.name = (SELECT s.data -> 'general' ->> 'timezone'
                                                FROM settings s WHERE s.workspace_id = $1)),
                               'UTC') AS name),
           days AS (
               SELECT (($6::timestamptz AT TIME ZONE tz.name)::date + d) AS day, tz.name
               FROM tz, generate_series(0, 62) d),
           slots AS (
               SELECT day, (day + make_interval(hours => $5::int)) AT TIME ZONE name AS at
               FROM days)
           SELECT min(at) AS "at!" FROM slots
           WHERE at > $6
             AND ($2 = 'daily'
                  OR ($2 = 'weekly' AND extract(isodow FROM day) = $3::int)
                  OR ($2 = 'monthly' AND extract(day FROM day) = $4::int))"#,
        workspace_id,
        rule.frequency,
        rule.weekday.map(i32::from),
        rule.day_of_month.map(i32::from),
        i32::from(rule.hour),
        after,
    )
    .fetch_one(db)
    .await
}

/// What a report covers: `[from, to)` plus its first and last local day.
pub struct Period {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub first_day: NaiveDate,
    pub last_day: NaiveDate,
}

/// The day or 7 days before local midnight of `at`, or the calendar month before its month.
pub async fn period(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    frequency: &str,
    at: DateTime<Utc>,
) -> Result<Period, sqlx::Error> {
    sqlx::query_as!(
        Period,
        r#"WITH tz AS (
               SELECT coalesce((SELECT z.name FROM pg_timezone_names z
                                WHERE z.name = (SELECT s.data -> 'general' ->> 'timezone'
                                                FROM settings s WHERE s.workspace_id = $1)),
                               'UTC') AS name),
           t AS (SELECT ($3::timestamptz AT TIME ZONE tz.name)::date::timestamp AS midnight, tz.name
                 FROM tz),
           e AS (SELECT CASE $2 WHEN 'monthly' THEN date_trunc('month', midnight)
                                ELSE midnight END AS stop, name
                 FROM t),
           p AS (SELECT CASE $2 WHEN 'monthly' THEN stop - interval '1 month'
                                WHEN 'weekly' THEN stop - interval '7 days'
                                ELSE stop - interval '1 day' END AS start, stop, name
                 FROM e)
           SELECT start AT TIME ZONE name AS "from!", stop AT TIME ZONE name AS "to!",
                  start::date AS "first_day!", (stop - interval '1 day')::date AS "last_day!"
           FROM p"#,
        workspace_id,
        frequency,
        at,
    )
    .fetch_one(db)
    .await
}

/// The workspace's display name.
pub async fn workspace_name(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
) -> Result<String, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT coalesce(nullif(btrim(profile->>'company_name'), ''), name) AS "name!"
           FROM workspaces WHERE id = $1"#,
        workspace_id
    )
    .fetch_one(db)
    .await
}

/// The owner's schedules in this workspace, oldest first.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<ScheduleRow>, sqlx::Error> {
    sqlx::query_as!(
        ScheduleRow,
        r#"SELECT s.id, s.report, s.frequency, s.weekday, s.day_of_month, s.hour, s.active,
                  s.paused_reason, s.next_run_at, s.created_at, coalesce(u.email, '') AS "email!",
                  r.ran_at AS "last_run_at?", r.outcome AS "last_outcome?"
           FROM scheduled_reports s
           JOIN users u ON u.id = s.user_id
           LEFT JOIN LATERAL (SELECT ran_at, outcome FROM scheduled_report_runs
                              WHERE schedule_id = s.id ORDER BY ran_at DESC, id DESC LIMIT 1) r
                  ON true
           WHERE s.workspace_id = $1 AND s.user_id = $2
           ORDER BY s.created_at, s.id"#,
        workspace_id,
        user_id,
    )
    .fetch_all(db)
    .await
}

/// How many schedules the owner has in this workspace.
pub async fn count(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM scheduled_reports WHERE workspace_id = $1 AND user_id = $2"#,
        workspace_id,
        user_id,
    )
    .fetch_one(db)
    .await
}

/// The owner's schedule `id`, its report and rule, when it is theirs in this workspace.
pub async fn owned(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<Due>, sqlx::Error> {
    sqlx::query_as!(
        Due,
        "SELECT id, workspace_id, user_id, report, frequency, weekday, day_of_month, hour
         FROM scheduled_reports WHERE id = $1 AND workspace_id = $2 AND user_id = $3",
        id,
        workspace_id,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// Store a new active schedule; its id.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    report: &str,
    rule: &Rule,
    next_run_at: DateTime<Utc>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO scheduled_reports
             (workspace_id, user_id, report, frequency, weekday, day_of_month, hour, next_run_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        workspace_id,
        user_id,
        report,
        rule.frequency,
        rule.weekday,
        rule.day_of_month,
        rule.hour,
        next_run_at,
    )
    .fetch_one(db)
    .await
}

/// Pause with `reason`, or resume (no reason) from `next_run_at`. Scoped to the owner.
pub async fn set_active(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    id: Uuid,
    reason: Option<&str>,
    next_run_at: Option<DateTime<Utc>>,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE scheduled_reports
         SET active = $4::text IS NULL, paused_reason = $4,
             next_run_at = coalesce($5, next_run_at), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND user_id = $3",
        id,
        workspace_id,
        user_id,
        reason,
        next_run_at,
    )
    .execute(db)
    .await?
    .rows_affected()
        == 1)
}

/// Delete the owner's schedule (its run log goes with it).
pub async fn delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!(
        "DELETE FROM scheduled_reports WHERE id = $1 AND workspace_id = $2 AND user_id = $3",
        id,
        workspace_id,
        user_id,
    )
    .execute(db)
    .await?
    .rows_affected()
        == 1)
}

/// Lock up to `limit` due schedules; a parallel runner skips them.
pub async fn claim_due(db: impl PgExecutor<'_>, limit: i64) -> Result<Vec<Due>, sqlx::Error> {
    sqlx::query_as!(
        Due,
        "SELECT id, workspace_id, user_id, report, frequency, weekday, day_of_month, hour
         FROM scheduled_reports WHERE active AND next_run_at <= now()
         ORDER BY next_run_at LIMIT $1 FOR UPDATE SKIP LOCKED",
        limit,
    )
    .fetch_all(db)
    .await
}

/// Move a claimed schedule to its next slot.
pub async fn advance(
    db: impl PgExecutor<'_>,
    id: Uuid,
    next_run_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE scheduled_reports SET next_run_at = $2, updated_at = now() WHERE id = $1",
        id,
        next_run_at,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Pause a claimed schedule whose owner lost access.
pub async fn pause_lost(db: impl PgExecutor<'_>, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE scheduled_reports SET active = false, paused_reason = 'access_lost', updated_at = now()
         WHERE id = $1",
        id,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Log one run; `unsubscribe_hash` only for a sent mail.
pub async fn log_run(
    db: impl PgExecutor<'_>,
    due: &Due,
    outcome: &str,
    unsubscribe_hash: Option<&[u8]>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO scheduled_report_runs (schedule_id, workspace_id, outcome, unsubscribe_hash)
         VALUES ($1, $2, $3, $4)",
        due.id,
        due.workspace_id,
        outcome,
        unsubscribe_hash,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// The owner's login email, when the account is still there.
pub async fn email(db: impl PgExecutor<'_>, user_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    Ok(
        sqlx::query_scalar!("SELECT email FROM users WHERE id = $1", user_id)
            .fetch_optional(db)
            .await?
            .flatten(),
    )
}

/// Pause the schedule a mailed link belongs to (`unsubscribed`, unless already paused);
/// its report, or None for an unknown link.
pub async fn unsubscribe(
    db: impl PgExecutor<'_>,
    hash: &[u8],
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "UPDATE scheduled_reports s
         SET active = false, paused_reason = coalesce(s.paused_reason, 'unsubscribed'),
             updated_at = now()
         FROM scheduled_report_runs r
         WHERE r.unsubscribe_hash = $1 AND s.id = r.schedule_id
         RETURNING s.report",
        hash,
    )
    .fetch_optional(db)
    .await
}

/// Drop run log rows older than 90 days (their unsubscribe links stop working).
pub async fn prune_runs(db: impl PgExecutor<'_>) -> Result<u64, sqlx::Error> {
    Ok(
        sqlx::query!("DELETE FROM scheduled_report_runs WHERE ran_at < now() - interval '90 days'")
            .execute(db)
            .await?
            .rows_affected(),
    )
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn rule(frequency: &str, weekday: Option<i16>, day_of_month: Option<i16>, hour: i16) -> Rule {
        Rule {
            frequency: frequency.into(),
            weekday,
            day_of_month,
            hour,
        }
    }

    async fn workspace(db: &PgPool) -> Uuid {
        sqlx::query_scalar(
            "WITH u AS (INSERT INTO users (email, first_name, last_name, password_hash)
                        VALUES ('o@x.io', 'O', 'W', 'h') RETURNING id)
             INSERT INTO workspaces (name, created_by_user_id) SELECT 'acme', id FROM u RETURNING id",
        )
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn set_zone(db: &PgPool, ws: Uuid, zone: &str) {
        sqlx::query(
            "INSERT INTO settings (workspace_id, data)
             VALUES ($1, jsonb_build_object('general', jsonb_build_object('timezone', $2::text)))
             ON CONFLICT (workspace_id) DO UPDATE SET data = EXCLUDED.data",
        )
        .bind(ws)
        .bind(zone)
        .execute(db)
        .await
        .unwrap();
    }

    /// Slots in the workspace zone, DST included; strictly after `after`; unknown zone = UTC.
    #[sqlx::test]
    async fn next_run_follows_the_workspace_time_zone(db: PgPool) {
        let ws = workspace(&db).await;
        let weekly = rule("weekly", Some(1), None, 7);
        let daily = rule("daily", None, None, 7);
        let monthly = rule("monthly", None, Some(28), 23);
        let next = |r: &Rule, after: &str| {
            let (db, r, after) = (db.clone(), r.clone(), t(after));
            async move { next_run(&db, ws, &r, after).await.unwrap() }
        };

        assert_eq!(
            next(&daily, "2026-11-09T06:59:00Z").await,
            t("2026-11-09T07:00:00Z")
        );
        set_zone(&db, ws, "Europe/Amsterdam").await;
        // Summer time (UTC+2) until 25 Oct 2026, winter (UTC+1) after.
        assert_eq!(
            next(&weekly, "2026-10-17T12:00:00Z").await,
            t("2026-10-19T05:00:00Z")
        );
        assert_eq!(
            next(&weekly, "2026-10-24T12:00:00Z").await,
            t("2026-10-26T06:00:00Z")
        );
        set_zone(&db, ws, "Africa/Nairobi").await;
        assert_eq!(
            next(&daily, "2026-11-09T03:59:00Z").await,
            t("2026-11-09T04:00:00Z")
        );
        assert_eq!(
            next(&daily, "2026-11-09T04:00:00Z").await,
            t("2026-11-10T04:00:00Z"),
            "a slot equal to `after` is used"
        );
        set_zone(&db, ws, "Mars/Olympus").await;
        assert_eq!(
            next(&monthly, "2026-02-28T23:00:00Z").await,
            t("2026-03-28T23:00:00Z")
        );
        assert_eq!(
            next(&monthly, "2026-12-29T00:00:00Z").await,
            t("2027-01-28T23:00:00Z")
        );
    }

    /// Daily covers yesterday, weekly the 7 days before, monthly the calendar month before,
    /// all ending at local midnight.
    #[sqlx::test]
    async fn period_ends_at_local_midnight(db: PgPool) {
        let ws = workspace(&db).await;
        set_zone(&db, ws, "Africa/Nairobi").await;
        let at = t("2026-11-09T04:00:00Z");

        let d = period(&db, ws, "daily", at).await.unwrap();
        assert_eq!(
            (d.from, d.to),
            (t("2026-11-07T21:00:00Z"), t("2026-11-08T21:00:00Z"))
        );
        assert_eq!(d.first_day, d.last_day);
        let w = period(&db, ws, "weekly", at).await.unwrap();
        assert_eq!(w.from, t("2026-11-01T21:00:00Z"));
        assert_eq!(
            (w.first_day.to_string(), w.last_day.to_string()),
            ("2026-11-02".into(), "2026-11-08".into())
        );
        let m = period(&db, ws, "monthly", at).await.unwrap();
        assert_eq!(
            (m.first_day.to_string(), m.last_day.to_string()),
            ("2026-10-01".into(), "2026-10-31".into())
        );
    }
}
