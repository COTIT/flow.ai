//! Scheduled report request/response bodies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{PartialSchema, ToSchema, openapi::RefOr, openapi::schema::Schema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, human},
    modules::scheduled_reports::repo::{Rule, ScheduleRow},
    openapi::string_enum,
};

/// Reports a schedule can mail. Fixed list: a schedule never names an operation.
pub const REPORTS: &[&str] = &["sales_summary", "attention", "overdue_invoices", "stock"];
/// How often; daily is the shortest.
pub const FREQUENCIES: &[&str] = &["daily", "weekly", "monthly"];
/// Why a schedule is paused.
pub const PAUSE_REASONS: &[&str] = &["user", "access_lost", "unsubscribed"];
/// What became of a run.
pub const OUTCOMES: &[&str] = &["sent", "skipped_empty", "paused_access", "failed"];

macro_rules! enum_schema {
    ($ty:ident, $list:expr, $doc:literal) => {
        #[doc = $doc]
        pub struct $ty;
        impl PartialSchema for $ty {
            fn schema() -> RefOr<Schema> {
                string_enum($list.iter().copied(), $doc)
            }
        }
        impl ToSchema for $ty {}
    };
}

enum_schema!(
    ScheduledReportKind,
    REPORTS,
    "Report mailed: `sales_summary` (sales of the past day, 7 days or month; needs `order.list`), `attention` (today's to-do, sections by your permissions), `overdue_invoices` (needs `invoice.list`; only sent when something is overdue), `stock` (sold out or at/below the low-stock threshold; needs `stock.list`; only sent when something is low)."
);
enum_schema!(
    ScheduledReportFrequency,
    FREQUENCIES,
    "How often: every day, one weekday a week, or one day (1-28) a month."
);
enum_schema!(
    ScheduledReportPause,
    PAUSE_REASONS,
    "Why it is paused: `user` (you paused it), `access_lost` (you lost the report's permission or left the workspace), `unsubscribed` (the link in a mail)."
);
enum_schema!(ScheduledReportOutcome, OUTCOMES, "What the last run did.");

/// A schedule as its owner sees it.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A scheduled report: mailed only to `email` (your own login), around `hour` local time in `time_zone` (runs every 5 minutes)."
)]
pub struct ScheduledReport {
    id: Uuid,
    #[schema(value_type = ScheduledReportKind)]
    report: String,
    #[schema(value_type = ScheduledReportFrequency)]
    frequency: String,
    /// ISO weekday 1 (Monday) to 7 (Sunday); weekly only.
    #[schema(required = true)]
    weekday: Option<i16>,
    /// 1 to 28; monthly only.
    #[schema(required = true)]
    day_of_month: Option<i16>,
    /// Local hour, 0 to 23.
    hour: i16,
    /// Workspace time zone the hour is read in (`general.timezone`, UTC when unset).
    #[schema(example = "Africa/Nairobi")]
    time_zone: String,
    active: bool,
    #[schema(required = true, value_type = Option<ScheduledReportPause>)]
    paused_reason: Option<String>,
    next_run_at: DateTime<Utc>,
    /// The only recipient: the owner's login email.
    email: String,
    #[schema(required = true)]
    last_run_at: Option<DateTime<Utc>>,
    #[schema(required = true, value_type = Option<ScheduledReportOutcome>)]
    last_outcome: Option<String>,
    created_at: DateTime<Utc>,
}

impl ScheduledReport {
    /// From a row and the workspace zone.
    pub fn new(r: ScheduleRow, time_zone: &str) -> Self {
        Self {
            id: r.id,
            report: r.report,
            frequency: r.frequency,
            weekday: r.weekday,
            day_of_month: r.day_of_month,
            hour: r.hour,
            time_zone: time_zone.to_owned(),
            active: r.active,
            paused_reason: r.paused_reason,
            next_run_at: r.next_run_at,
            email: r.email,
            last_run_at: r.last_run_at,
            last_outcome: r.last_outcome,
            created_at: r.created_at,
        }
    }
}

/// POST body: which report and when. No recipient field: it is always you.
#[derive(Deserialize, ToSchema)]
pub struct CreateScheduledReportRequest {
    #[schema(value_type = ScheduledReportKind, example = "sales_summary")]
    report: Option<String>,
    #[schema(value_type = ScheduledReportFrequency, example = "weekly")]
    frequency: Option<String>,
    /// ISO weekday 1 (Monday) to 7 (Sunday); required for weekly, refused otherwise.
    #[schema(value_type = Option<i64>, example = 1)]
    weekday: Option<Value>,
    /// 1 to 28; required for monthly, refused otherwise.
    #[schema(value_type = Option<i64>)]
    day_of_month: Option<Value>,
    /// Local hour 0 to 23 in the workspace time zone ("morning" = 7).
    #[schema(value_type = i64, example = 7)]
    hour: Option<Value>,
}

/// Validated create input.
pub struct NewSchedule {
    pub report: String,
    pub rule: Rule,
}

/// Required for `when`, refused otherwise; `min..=max` whole number.
fn only_for(
    r: &mut Rules,
    field: &str,
    value: Option<Value>,
    wanted: bool,
    max: i64,
) -> Option<i16> {
    let value = value.filter(|v| !v.is_null());
    match (wanted, value) {
        (true, None) => {
            r.missing(field);
            None
        }
        (false, Some(_)) => {
            r.add(
                field,
                format!(
                    "The {} field does not apply to this frequency.",
                    human(field)
                ),
            );
            None
        }
        (true, Some(v)) => r
            .integer(field, &v, 1, max)
            .and_then(|n| i16::try_from(n).ok()),
        (false, None) => None,
    }
}

impl Validate for CreateScheduledReportRequest {
    type Valid = NewSchedule;

    fn validate(self, r: &mut Rules) -> Option<NewSchedule> {
        let report = r
            .required("report", self.report.clone())
            .and_then(|_| r.one_of("report", self.report, REPORTS, None));
        let frequency = r
            .required("frequency", self.frequency.clone())
            .and_then(|_| r.one_of("frequency", self.frequency, FREQUENCIES, None));
        let hour = match r.required_value("hour", self.hour) {
            Some(v) => r.integer("hour", &v, 0, 23),
            None => None,
        };
        let f = frequency.as_deref().unwrap_or_default();
        let weekday = only_for(r, "weekday", self.weekday, f == "weekly", 7);
        let day_of_month = only_for(r, "day_of_month", self.day_of_month, f == "monthly", 28);
        Some(NewSchedule {
            report: report?,
            rule: Rule {
                frequency: frequency?,
                weekday,
                day_of_month,
                hour: i16::try_from(hour?).ok()?,
            },
        })
    }
}

/// POST /{schedule} body: pause or resume.
#[derive(Deserialize, ToSchema)]
pub struct UpdateScheduledReportRequest {
    /// false pauses (`paused_reason` `user`), true resumes from the next slot.
    #[schema(value_type = bool)]
    active: Option<Value>,
}

impl Validate for UpdateScheduledReportRequest {
    type Valid = bool;

    fn validate(self, r: &mut Rules) -> Option<bool> {
        let v = r.required_value("active", self.active)?;
        r.boolean("active", &v)
    }
}

/// Public unsubscribe body: the token from a report mail.
#[derive(Deserialize, ToSchema)]
pub struct UnsubscribeRequest {
    /// `token` from the mail's unsubscribe link.
    token: Option<String>,
}

impl Validate for UnsubscribeRequest {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        r.required("token", self.token)
    }
}

/// Unsubscribe reply: which report stopped.
#[derive(Serialize, ToSchema)]
pub struct Unsubscribed {
    #[schema(value_type = ScheduledReportKind)]
    pub report: String,
}

impl ScheduledReport {
    /// Its id.
    pub fn id(&self) -> Uuid {
        self.id
    }
}
