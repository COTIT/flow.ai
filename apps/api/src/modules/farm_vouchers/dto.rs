//! Voucher request/response bodies. Money is integer minor units plus its currency.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, optional},
    modules::{
        farm_surveys::dto::Money,
        farm_vouchers::repo::{LogRow, MineRow, TotalRow, TrailRow},
    },
};

/// An account named on a voucher.
#[derive(Serialize, ToSchema)]
pub struct StaffRef {
    pub id: Uuid,
    pub name: String,
}

/// One of the household's vouchers, code included. The audit flag is never shown here.
#[derive(Serialize, ToSchema)]
pub struct MyVoucher {
    id: Uuid,
    survey_id: Uuid,
    survey_title_en: String,
    #[schema(required = true)]
    survey_title_sw: Option<String>,
    /// Show this at the office (typed or as a QR code).
    #[schema(example = "K7M2Q9XD4P")]
    code: String,
    amount: Money,
    /// `issued`, `redeemed` or `void`.
    status: String,
    expired: bool,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    #[schema(required = true)]
    redeemed_at: Option<DateTime<Utc>>,
}

impl From<MineRow> for MyVoucher {
    fn from(v: MineRow) -> Self {
        Self {
            id: v.id,
            survey_id: v.survey_id,
            survey_title_en: v.survey_title_en,
            survey_title_sw: v.survey_title_sw,
            code: v.code,
            amount: Money {
                minor: v.amount_minor,
                currency: v.currency,
                decimals: v.decimals,
                currency_id: v.currency_id,
            },
            status: v.status,
            expired: v.expired,
            issued_at: v.issued_at,
            expires_at: v.expires_at,
            redeemed_at: v.redeemed_at,
        }
    }
}

/// GET list query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// POST /farm/vouchers/lookup.
#[derive(Deserialize, ToSchema)]
pub struct LookupRequest {
    /// As typed or scanned; case, spaces and dashes don't matter.
    #[schema(value_type = String)]
    code: Option<String>,
}

impl Validate for LookupRequest {
    type Valid = String;

    fn validate(self, r: &mut Rules) -> Option<String> {
        r.required("code", self.code)
    }
}

/// What a scan shows the officer. Unknown codes, and codes of other villages, are `found: false`.
#[derive(Serialize, ToSchema, Default)]
pub struct LookupResult {
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voucher_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expired: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redeemed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub survey_title_en: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub survey_title_sw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub household_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respondent_name: Option<String>,
    /// Held for an audit: ops or admin must redeem it in person.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_ops: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_redeem: Option<bool>,
    /// Why this caller may not redeem it now, as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

/// POST /farm/vouchers/redeem.
#[derive(Deserialize, ToSchema)]
pub struct RedeemRequest {
    #[schema(value_type = String)]
    code: Option<String>,
    /// The ID document checked: `nida`, `voter`, `driving_licence` or `village_letter`. Never its number.
    #[schema(value_type = String)]
    id_type: Option<String>,
    /// The name on the ID matches the household.
    name_confirmed: Option<bool>,
}

/// Validated redeem; the ID fields are checked after the voucher's own rules.
pub struct Redeem {
    pub code: String,
    pub id_type: Option<String>,
    pub name_confirmed: bool,
}

impl Validate for RedeemRequest {
    type Valid = Redeem;

    fn validate(self, r: &mut Rules) -> Option<Redeem> {
        Some(Redeem {
            code: r.required("code", self.code)?,
            id_type: optional(self.id_type),
            name_confirmed: self.name_confirmed == Some(true),
        })
    }
}

/// A redemption.
#[derive(Serialize, ToSchema)]
pub struct Redeemed {
    pub voucher_id: Uuid,
    pub status: String,
    pub redeemed_at: DateTime<Utc>,
    pub redeemed_by: StaffRef,
    pub amount: Money,
    /// True when this officer had already redeemed it (a retry): no second payout.
    pub replayed: bool,
}

/// POST /farm/vouchers/{voucher}/void.
#[derive(Deserialize, ToSchema)]
pub struct VoidRequest {
    #[schema(value_type = String, max_length = 500)]
    reason: Option<String>,
}

impl Validate for VoidRequest {
    type Valid = String;

    fn validate(self, _: &mut Rules) -> Option<String> {
        // Blank is refused by the service in RUAHA's words, after the voucher checks.
        Some(
            self.reason
                .unwrap_or_default()
                .trim()
                .chars()
                .take(500)
                .collect(),
        )
    }
}

/// One step of a voucher's trail.
#[derive(Serialize, ToSchema)]
pub struct TrailEntry {
    occurred_at: DateTime<Utc>,
    /// household_registered, household_verified, login_initial, login_reset, survey_published,
    /// answered, issued, scanned, refused, redeemed or voided.
    kind: String,
    actor_name: String,
    actor_role: String,
    #[schema(value_type = Object)]
    detail: Value,
}

impl From<TrailRow> for TrailEntry {
    fn from(t: TrailRow) -> Self {
        Self {
            occurred_at: t.occurred_at,
            kind: t.kind,
            actor_name: t.actor_name,
            actor_role: t.actor_role,
            detail: t.detail,
        }
    }
}

/// `?from=YYYY-MM-DD&to=YYYY-MM-DD`, Tanzanian calendar days, both included.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DaysQuery {
    #[param(value_type = NaiveDate)]
    pub from: Option<String>,
    #[param(value_type = NaiveDate)]
    pub to: Option<String>,
    /// The log only: 1-based page; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// The log only: default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// One redemption, for cash reconciliation.
#[derive(Serialize, ToSchema)]
pub struct Redemption {
    voucher_id: Uuid,
    redeemed_at: DateTime<Utc>,
    redeemed_by: StaffRef,
    village_name: String,
    household_label: String,
    survey_title_en: String,
    #[schema(required = true)]
    survey_title_sw: Option<String>,
    amount: Money,
    id_type_seen: String,
    audit_required: bool,
}

impl From<LogRow> for Redemption {
    fn from(l: LogRow) -> Self {
        Self {
            voucher_id: l.voucher_id,
            redeemed_at: l.redeemed_at,
            redeemed_by: StaffRef {
                id: l.redeemed_by,
                name: l.redeemed_by_name,
            },
            village_name: l.village_name,
            household_label: l.household_label,
            survey_title_en: l.survey_title_en,
            survey_title_sw: l.survey_title_sw,
            amount: Money {
                minor: l.amount_minor,
                currency: l.currency,
                decimals: l.decimals,
                currency_id: l.currency_id,
            },
            id_type_seen: l.id_type_seen,
            audit_required: l.audit_required,
        }
    }
}

/// Redemptions per officer per day and currency.
#[derive(Serialize, ToSchema)]
pub struct RedemptionTotal {
    day: NaiveDate,
    redeemed_by: StaffRef,
    vouchers: i64,
    /// Sum in minor units of `currency`.
    minor: i64,
    currency: String,
    decimals: i16,
}

impl From<TotalRow> for RedemptionTotal {
    fn from(t: TotalRow) -> Self {
        Self {
            day: t.day,
            redeemed_by: StaffRef {
                id: t.redeemed_by,
                name: t.redeemed_by_name,
            },
            vouchers: t.vouchers,
            minor: t.minor,
            currency: t.currency,
            decimals: t.decimals,
        }
    }
}
