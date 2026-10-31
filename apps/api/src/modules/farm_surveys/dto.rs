//! Survey request/response bodies. Money is integer minor units plus its currency.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    http::json::{Rules, Validate, filled, optional, present},
    modules::farm_surveys::repo::{SurveyRow, SurveyVoucherRow},
};

const KINDS: &[&str] = &["single_choice", "multi_choice", "number", "text", "yes_no"];

/// An amount in minor units of a workspace currency (TZS 10,000.00 = 1000000 at 2 decimals).
#[derive(Serialize, ToSchema)]
pub struct Money {
    pub minor: i64,
    /// ISO 4217 code.
    #[schema(example = "TZS")]
    pub currency: String,
    pub decimals: i16,
    pub currency_id: Uuid,
}

/// Who published a survey.
#[derive(Serialize, ToSchema)]
pub struct PublisherRef {
    id: Uuid,
    name: String,
}

/// One option of a choice question.
#[derive(Serialize, ToSchema)]
pub struct QuestionOption {
    value: String,
    label_en: String,
    #[schema(required = true)]
    label_sw: Option<String>,
}

/// One question.
#[derive(Serialize, ToSchema)]
pub struct QuestionResource {
    id: Uuid,
    position: i32,
    /// `single_choice`, `multi_choice`, `number`, `text` or `yes_no`.
    kind: String,
    prompt_en: String,
    #[schema(required = true)]
    prompt_sw: Option<String>,
    required: bool,
    #[schema(value_type = Vec<QuestionOption>)]
    options: Value,
}

/// A survey with its questions.
#[derive(Serialize, ToSchema)]
#[schema(
    description = "A survey. A household answers it once and receives a voucher for `reward`."
)]
pub struct SurveyResource {
    id: Uuid,
    /// Null = every village.
    #[schema(required = true)]
    village_id: Option<Uuid>,
    title_en: String,
    #[schema(required = true)]
    title_sw: Option<String>,
    #[schema(required = true)]
    description_en: Option<String>,
    #[schema(required = true)]
    description_sw: Option<String>,
    reward: Money,
    #[schema(required = true)]
    max_households: Option<i32>,
    audit_percent: i16,
    #[schema(required = true)]
    closes_at: Option<DateTime<Utc>>,
    /// `draft`, `live` or `closed`.
    status: String,
    #[schema(required = true)]
    published_at: Option<DateTime<Utc>>,
    #[schema(required = true, value_type = Option<PublisherRef>)]
    published_by: Option<Value>,
    #[schema(required = true)]
    closed_at: Option<DateTime<Utc>>,
    /// Households that answered.
    responses: i64,
    #[schema(value_type = Vec<QuestionResource>)]
    questions: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<SurveyRow> for SurveyResource {
    fn from(s: SurveyRow) -> Self {
        Self {
            id: s.id,
            village_id: s.village_id,
            title_en: s.title_en,
            title_sw: s.title_sw,
            description_en: s.description_en,
            description_sw: s.description_sw,
            reward: Money {
                minor: s.reward_minor,
                currency: s.currency,
                decimals: s.decimals,
                currency_id: s.currency_id,
            },
            max_households: s.max_households,
            audit_percent: s.audit_percent,
            closes_at: s.closes_at,
            status: s.status,
            published_at: s.published_at,
            published_by: s.published_by,
            closed_at: s.closed_at,
            responses: s.answered,
            questions: s.questions,
            created_at: s.created_at,
            updated_at: s.updated_at,
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

/// POST /farm/surveys, and PATCH (only sent fields change; null clears an optional one).
#[derive(Deserialize, ToSchema)]
#[schema(description = "Survey fields. Editable only while the survey is a draft.")]
pub struct SurveyRequest {
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = String, max_length = 200)]
    title_en: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 200)]
    title_sw: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 2000)]
    description_en: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>, max_length = 2000)]
    description_sw: Option<Option<String>>,
    /// The incentive in minor units of `currency_id`.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = i64, example = 1000000)]
    reward_minor: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Uuid)]
    currency_id: Option<Option<String>>,
    /// Null = every village.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    village_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>)]
    max_households: Option<Option<Value>>,
    /// 0–100, default 15: share of vouchers held for an ops audit at redemption.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i16>)]
    audit_percent: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<DateTime<Utc>>)]
    closes_at: Option<Option<String>>,
}

/// Validated survey fields; None = not sent.
pub struct SurveyInput {
    pub title_en: Option<String>,
    pub title_sw: Option<Option<String>>,
    pub description_en: Option<Option<String>>,
    pub description_sw: Option<Option<String>>,
    pub reward_minor: Option<i64>,
    pub currency_id: Option<Uuid>,
    pub village_id: Option<Option<Uuid>>,
    pub max_households: Option<Option<i32>>,
    pub audit_percent: Option<i16>,
    pub closes_at: Option<Option<DateTime<Utc>>>,
}

fn uuid_field(r: &mut Rules, field: &str, v: Option<String>) -> Option<Uuid> {
    let v = optional(v)?;
    let id = Uuid::parse_str(&v).ok();
    r.check(
        field,
        id.is_some(),
        format!("The {} must be a valid UUID.", field.replace('_', " ")),
    );
    id
}

impl Validate for SurveyRequest {
    type Valid = SurveyInput;

    fn validate(self, r: &mut Rules) -> Option<SurveyInput> {
        let reward_minor = self.reward_minor.map(|v| match filled(v) {
            Some(v) => r.integer("reward_minor", &v, 1, i64::MAX),
            None => {
                r.missing("reward_minor");
                None
            }
        });
        let currency_id = self.currency_id.map(|v| match optional(v.clone()) {
            Some(_) => uuid_field(r, "currency_id", v),
            None => {
                r.missing("currency_id");
                None
            }
        });
        let max_households = self.max_households.map(|v| {
            filled(v)
                .and_then(|v| r.integer("max_households", &v, 1, i64::from(i32::MAX)))
                .and_then(|n| i32::try_from(n).ok())
        });
        let audit_percent = self.audit_percent.map(|v| {
            filled(v)
                .and_then(|v| r.integer("audit_percent", &v, 0, 100))
                .and_then(|n| i16::try_from(n).ok())
        });
        let closes_at = self.closes_at.map(|v| {
            optional(v).and_then(|s| {
                let t = DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|t| t.with_timezone(&Utc));
                r.check(
                    "closes_at",
                    t.is_some(),
                    "The closes at must be a date and time (RFC 3339).",
                );
                t
            })
        });
        Some(SurveyInput {
            title_en: r.patch_text("title_en", self.title_en, 200),
            title_sw: r.patch_opt_text("title_sw", self.title_sw, 200),
            description_en: r.patch_opt_text("description_en", self.description_en, 2000),
            description_sw: r.patch_opt_text("description_sw", self.description_sw, 2000),
            reward_minor: reward_minor.flatten(),
            currency_id: currency_id.flatten(),
            village_id: self.village_id.map(|v| uuid_field(r, "village_id", v)),
            max_households,
            audit_percent: audit_percent.flatten(),
            closes_at,
        })
    }
}

/// POST /farm/surveys/{survey}/questions and PUT .../{question}.
#[derive(Deserialize, ToSchema)]
#[schema(description = "A question. Editable only while the survey is a draft.")]
pub struct QuestionRequest {
    #[schema(value_type = i32, minimum = 1)]
    position: Option<Value>,
    /// `single_choice`, `multi_choice`, `number`, `text` or `yes_no`.
    #[schema(value_type = String)]
    kind: Option<String>,
    #[schema(value_type = String, max_length = 500)]
    prompt_en: Option<String>,
    #[schema(value_type = Option<String>, max_length = 500)]
    prompt_sw: Option<String>,
    /// Default true.
    required: Option<bool>,
    /// Choice kinds: `[{value, label_en, label_sw}]`, at least two before publishing.
    #[schema(value_type = Option<Vec<Object>>)]
    options: Option<Value>,
}

/// Validated question.
pub struct QuestionInput {
    pub position: i32,
    pub kind: String,
    pub prompt_en: String,
    pub prompt_sw: Option<String>,
    pub required: bool,
    pub options: Value,
}

impl Validate for QuestionRequest {
    type Valid = QuestionInput;

    fn validate(self, r: &mut Rules) -> Option<QuestionInput> {
        let position = match filled(self.position) {
            Some(v) => r
                .integer("position", &v, 1, 1000)
                .and_then(|n| i32::try_from(n).ok()),
            None => {
                r.missing("position");
                None
            }
        };
        let kind = r
            .required("kind", self.kind.clone())
            .and_then(|_| r.one_of("kind", self.kind, KINDS, None));
        let given = self.options.unwrap_or(Value::Array(Vec::new()));
        let ok = given.as_array().is_some_and(|a| {
            a.len() <= 50
                && a.iter()
                    .all(|o| o.is_object() && o["value"].is_string() && o["label_en"].is_string())
        });
        r.check(
            "options",
            ok,
            "The options must be a list of {value, label_en, label_sw}.",
        );
        // Stored in one shape: exactly these three keys, trimmed.
        let text = |v: &Value| {
            v.as_str()
                .map(|s| Value::String(s.trim().to_owned()))
                .unwrap_or(Value::Null)
        };
        let options = Value::Array(
            given
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|o| json!({ "value": text(&o["value"]), "label_en": text(&o["label_en"]), "label_sw": text(&o["label_sw"]) }))
                        .collect()
                })
                .unwrap_or_default(),
        );
        Some(QuestionInput {
            position: position?,
            kind: kind?,
            prompt_en: r.text("prompt_en", self.prompt_en, 500),
            prompt_sw: r.opt_text("prompt_sw", self.prompt_sw, 500),
            required: self.required.unwrap_or(true),
            options,
        })
    }
}

/// POST /farm/surveys/{survey}/responses.
#[derive(Deserialize, ToSchema)]
#[schema(
    description = "The household's answers, keyed by question id. Make `client_ref` once \
    per answer form and send it again on a retry."
)]
pub struct ResponseRequest {
    #[schema(value_type = Object)]
    answers: Option<Value>,
    #[schema(value_type = Uuid)]
    client_ref: Option<String>,
}

/// Validated submit.
pub struct ResponseInput {
    pub answers: serde_json::Map<String, Value>,
    pub client_ref: Uuid,
}

impl Validate for ResponseRequest {
    type Valid = ResponseInput;

    fn validate(self, r: &mut Rules) -> Option<ResponseInput> {
        let client_ref = match optional(self.client_ref) {
            None => {
                r.add(
                    "client_ref",
                    "client_ref is required so a retry cannot answer twice",
                );
                None
            }
            v => uuid_field(r, "client_ref", v),
        };
        let answers = match self.answers {
            Some(Value::Object(m)) => Some(m),
            _ => {
                r.add("answers", "answers must be an object keyed by question");
                None
            }
        };
        Some(ResponseInput {
            answers: answers?,
            client_ref: client_ref?,
        })
    }
}

/// The voucher a submit issued.
#[derive(Serialize, ToSchema)]
#[schema(description = "The household's voucher. Show `code` at the office to be paid.")]
pub struct Submitted {
    pub response_id: Uuid,
    pub voucher_id: Uuid,
    pub code: String,
    pub amount: Money,
    pub expires_at: DateTime<Utc>,
    /// True when this repeats an earlier submit with the same `client_ref`.
    pub replayed: bool,
}

/// One live or closed survey and whether the caller's household may answer it.
#[derive(Serialize, ToSchema)]
pub struct Eligibility {
    pub survey_id: Uuid,
    pub eligible: bool,
    /// Why not, shown as written; null when eligible or already answered.
    #[schema(required = true)]
    pub reason: Option<String>,
    #[schema(required = true)]
    pub response_id: Option<Uuid>,
    #[schema(required = true)]
    pub voucher_id: Option<Uuid>,
}

/// A count and its money.
#[derive(Serialize, ToSchema)]
pub struct CountMinor {
    pub count: i64,
    pub minor: i64,
}

/// One option's answer count.
#[derive(Serialize, ToSchema)]
pub struct TallyRow {
    question_id: Uuid,
    position: i32,
    option_value: String,
    answers: i64,
}

/// Number answers of one question.
#[derive(Serialize, ToSchema)]
pub struct NumberSummary {
    question_id: Uuid,
    position: i32,
    answers: i64,
    #[schema(example = "6.00")]
    average: String,
    minimum: String,
    maximum: String,
}

/// A survey's answers and vouchers, staff only. Money in minor units of the survey's currency.
#[derive(Serialize, ToSchema)]
pub struct SurveyResults {
    pub responses: i64,
    pub currency: String,
    pub issued: CountMinor,
    pub redeemed: CountMinor,
    /// Issued, not expired, not redeemed.
    pub outstanding: CountMinor,
    pub expired_count: i64,
    pub void_count: i64,
    #[schema(value_type = Vec<TallyRow>)]
    pub tally: Value,
    #[schema(value_type = Vec<NumberSummary>)]
    pub numbers: Value,
}

/// A survey's voucher for ops. Never the code.
#[derive(Serialize, ToSchema)]
pub struct SurveyVoucher {
    voucher_id: Uuid,
    household_label: String,
    respondent_name: String,
    submitted_at: DateTime<Utc>,
    /// `issued`, `redeemed` or `void`.
    status: String,
    expired: bool,
    amount: Money,
    audit_required: bool,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    #[schema(required = true)]
    redeemed_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    redeemed_by_name: Option<String>,
    #[schema(required = true)]
    id_type_seen: Option<String>,
    #[schema(required = true)]
    void_reason: Option<String>,
}

impl From<SurveyVoucherRow> for SurveyVoucher {
    fn from(v: SurveyVoucherRow) -> Self {
        Self {
            voucher_id: v.id,
            household_label: v.household_label,
            respondent_name: v.respondent_name,
            submitted_at: v.submitted_at,
            status: v.status,
            expired: v.expired,
            amount: Money {
                minor: v.amount_minor,
                currency: v.currency,
                decimals: v.decimals,
                currency_id: v.currency_id,
            },
            audit_required: v.audit_required,
            issued_at: v.issued_at,
            expires_at: v.expires_at,
            redeemed_at: v.redeemed_at,
            redeemed_by_name: v.redeemed_by_name,
            id_type_seen: v.id_type_seen,
            void_reason: v.void_reason,
        }
    }
}
