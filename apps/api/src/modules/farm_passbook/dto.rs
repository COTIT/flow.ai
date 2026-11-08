//! Passbook and consent response bodies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::modules::{
    farm_passbook::repo::{ConsentRow, EntryRow, OwedRow},
    farm_surveys::dto::Money,
};

/// One passbook line.
#[derive(Serialize, ToSchema)]
#[schema(description = "A passbook line: a credit (sale) or debit (payout, reversal).")]
pub struct EntryResource {
    id: Uuid,
    /// `sale_credit`, `sale_reversal`, `payout` or `payout_void`.
    kind: String,
    /// Signed: credits positive, debits negative.
    amount: Money,
    /// Balance in this currency right after this line.
    balance_after: Money,
    /// The sale attribution behind a sale line.
    #[schema(required = true)]
    attribution_id: Option<Uuid>,
    /// The payout behind a payout line.
    #[schema(required = true)]
    payout_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

impl From<EntryRow> for EntryResource {
    fn from(e: EntryRow) -> Self {
        let money = |minor| Money {
            minor,
            currency: e.currency.clone(),
            decimals: e.decimals,
            currency_id: e.currency_id,
        };
        Self {
            id: e.id,
            kind: e.kind.clone(),
            amount: money(e.amount),
            balance_after: money(e.balance_after),
            attribution_id: e.attribution_id,
            payout_id: e.payout_id,
            created_at: e.created_at,
        }
    }
}

/// A person's passport consent.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "Whether the public lot page may ever name or picture this person. Nothing is shown while the data-protection gate is closed."
)]
pub struct ConsentResource {
    person_id: Uuid,
    consent: bool,
    #[schema(required = true)]
    consent_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    consent_by: Option<Uuid>,
}

impl From<ConsentRow> for ConsentResource {
    fn from(c: ConsentRow) -> Self {
        Self {
            person_id: c.id,
            consent: c.passport_profile_consent,
            consent_at: c.passport_profile_consent_at,
            consent_by: c.passport_profile_consent_by,
        }
    }
}

/// A page of passbook lines with the balance per currency.
#[derive(Serialize, ToSchema)]
pub struct PassbookPage {
    pub data: Vec<EntryResource>,
    pub meta: crate::http::list::Meta,
    /// Balance per currency over every line (not just this page), by currency code.
    pub balances: Vec<Money>,
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
    /// First Tanzania day kept (`YYYY-MM-DD`, inclusive); blank = no bound.
    pub from: Option<String>,
    /// Last Tanzania day kept (`YYYY-MM-DD`, inclusive); blank = no bound.
    pub to: Option<String>,
}

/// GET /farm/passbook/balances query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BalancesQuery {
    /// Only farmers living in this village.
    #[param(value_type = Option<Uuid>)]
    pub village_id: Option<String>,
    /// 1-based; junk falls back to 1.
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    /// Default 50, max 100.
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// One farmer the workspace owes, in one currency.
#[derive(Serialize, ToSchema)]
#[schema(description = "A farmer with a positive passbook balance: money the co-op owes them.")]
pub struct OwedResource {
    person_id: Uuid,
    /// Given and family name.
    name: String,
    village_id: Uuid,
    /// Village name.
    village: String,
    /// Positive balance over every line.
    balance: Money,
}

impl From<OwedRow> for OwedResource {
    fn from(r: OwedRow) -> Self {
        Self {
            person_id: r.person_id,
            name: r.name,
            village_id: r.village_id,
            village: r.village,
            balance: r.balance,
        }
    }
}

/// A page of farmers owed money with the total owed per currency.
#[derive(Serialize, ToSchema)]
pub struct BalancesPage {
    pub data: Vec<OwedResource>,
    pub meta: crate::http::list::Meta,
    /// Total owed per currency over every page, by currency code.
    pub totals: Vec<Money>,
}
