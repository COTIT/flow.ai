//! Tax profile bodies (masterplan §15 TAX): draft payload, patch, verify, resource, review.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    error::AppError,
    http::json::{Rules, Validate, optional, present},
};

const REF_MAX: usize = 255;
const REASON_MAX: usize = 1000;
/// Components per profile.
pub const MAX_COMPONENTS: usize = 10;
/// Exemptions per profile.
pub const MAX_EXEMPTIONS: usize = 100;
const TYPES: [&str; 2] = ["sale", "purchase"];
const TAX_POINTS: [&str; 2] = ["invoice", "supply"];

/// One percentage component.
#[derive(Clone, Serialize, ToSchema)]
pub struct TaxComponent {
    pub code: String,
    pub name: String,
    /// Always `percentage`; fixed and compound are unsupported.
    pub kind: &'static str,
    /// Percent, exact decimal string, 0..=100, ≤ 8 decimals.
    #[schema(value_type = String, example = "18")]
    #[serde(serialize_with = "rate_text")]
    pub rate: Decimal,
    /// Purchase tax the business can reclaim (all or nothing).
    pub recoverable: bool,
}

fn rate_text<S: serde::Serializer>(v: &Decimal, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.normalize().to_string())
}

/// Zero tax for one relation or one product category.
#[derive(Clone, Serialize, ToSchema)]
pub struct TaxExemption {
    pub relation_id: Option<Uuid>,
    pub product_category_id: Option<Uuid>,
    pub reason: String,
    pub evidence_ref: String,
}

/// A whole validated draft.
#[derive(Clone)]
pub struct Draft {
    pub country: String,
    pub subdivision: Option<String>,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub transaction_types: Vec<String>,
    pub product_categories: Vec<Uuid>,
    pub components: Vec<TaxComponent>,
    pub exemptions: Vec<TaxExemption>,
    pub tax_point: String,
    pub evidence_ref: Option<String>,
}

impl Draft {
    /// Cross-field rules, errors under `payload.`.
    pub fn check(&self, r: &mut Rules) {
        r.check(
            "payload.effective_to",
            self.effective_to.is_none_or(|to| to > self.effective_from),
            "The effective to must be a date after effective from.",
        );
        if let Some(s) = &self.subdivision {
            r.check(
                "payload.jurisdiction_scope",
                s.starts_with(&format!("{}-", self.country)),
                "The jurisdiction scope must be a subdivision of the country.",
            );
        }
    }
}

/// TaxComponent as sent.
#[derive(Deserialize, ToSchema)]
pub struct TaxComponentIn {
    code: Option<String>,
    name: Option<String>,
    /// Must be `percentage`.
    kind: Option<String>,
    #[schema(value_type = String, example = "18")]
    rate: Option<Value>,
    #[schema(value_type = bool)]
    recoverable: Option<Value>,
}

/// TaxExemption as sent: exactly one of `relation_id` / `product_category_id`.
#[derive(Deserialize, ToSchema)]
pub struct TaxExemptionIn {
    #[schema(value_type = Option<Uuid>)]
    relation_id: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    product_category_id: Option<String>,
    reason: Option<String>,
    evidence_ref: Option<String>,
}

fn uuid(r: &mut Rules, field: &str, v: &str) -> Option<Uuid> {
    let id = Uuid::parse_str(v.trim()).ok();
    r.check(field, id.is_some(), "The value must be a valid UUID.");
    id
}

fn date(r: &mut Rules, field: &str, v: Option<String>) -> Option<NaiveDate> {
    let v = r.required(field, v)?;
    let d = NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok();
    r.check(field, d.is_some(), "The value must be a date (YYYY-MM-DD).");
    d
}

fn opt_date(r: &mut Rules, field: &str, v: Option<String>) -> Option<Option<NaiveDate>> {
    match optional(v) {
        None => Some(None),
        Some(v) => date(r, field, Some(v)).map(Some),
    }
}

fn country(r: &mut Rules, v: Option<String>) -> Option<String> {
    let v = r.required("country", v)?;
    r.country("country", &v)
}

fn subdivision(r: &mut Rules, v: Option<String>) -> Option<Option<String>> {
    let Some(v) = optional(v) else {
        return Some(None);
    };
    let v = v.to_ascii_uppercase();
    let b = v.as_bytes();
    let ok = b.len() >= 4
        && b.len() <= 6
        && b[..2].iter().all(u8::is_ascii_uppercase)
        && b[2] == b'-'
        && b[3..]
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    r.check(
        "jurisdiction_scope",
        ok,
        "The jurisdiction scope must be an ISO 3166-2 code, e.g. TZ-02.",
    );
    ok.then_some(Some(v))
}

fn types(r: &mut Rules, v: Option<Vec<String>>) -> Option<Vec<String>> {
    let Some(v) = v.filter(|v| !v.is_empty()) else {
        r.missing("transaction_types");
        return None;
    };
    let mut out = Vec::new();
    for (i, t) in v.into_iter().enumerate() {
        let t = t.trim().to_owned();
        if !TYPES.contains(&t.as_str()) {
            r.add(
                &format!("transaction_types.{i}"),
                "The transaction type must be sale or purchase.",
            );
        } else if !out.contains(&t) {
            out.push(t);
        }
    }
    Some(out)
}

fn categories(r: &mut Rules, v: Option<Vec<String>>) -> Vec<Uuid> {
    let mut out = Vec::new();
    for (i, c) in v.unwrap_or_default().iter().enumerate() {
        if let Some(id) = uuid(r, &format!("product_categories.{i}"), c)
            && !out.contains(&id)
        {
            out.push(id);
        }
    }
    out
}

fn tax_point(r: &mut Rules, v: Option<String>) -> Option<String> {
    let v = r.required("tax_point", v)?;
    let ok = TAX_POINTS.contains(&v.as_str());
    r.check(
        "tax_point",
        ok,
        "The tax point must be invoice or supply; payment-based tax timing is not supported.",
    );
    ok.then_some(v)
}

fn components(r: &mut Rules, v: Option<Vec<TaxComponentIn>>) -> Option<Vec<TaxComponent>> {
    let Some(v) = v.filter(|v| !v.is_empty()) else {
        r.missing("components");
        return None;
    };
    if v.len() > MAX_COMPONENTS {
        r.add("components", "A tax profile has at most 10 components.");
        return None;
    }
    let mut out: Vec<TaxComponent> = Vec::new();
    for (i, c) in v.into_iter().enumerate() {
        let f = |k: &str| format!("components.{i}.{k}");
        let code = r
            .required(&f("code"), c.code)
            .map(|s| s.to_ascii_uppercase());
        if let Some(code) = &code {
            let shape = code.len() <= 32
                && code
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
            r.check(
                &f("code"),
                shape,
                "The code may hold A-Z, 0-9 and _ (at most 32).",
            );
            r.check(
                &f("code"),
                !out.iter().any(|o| o.code == *code),
                "The code is already used in this profile.",
            );
        }
        let name = r.text(&f("name"), c.name, REF_MAX);
        let kind = r.required(&f("kind"), c.kind);
        if kind.as_deref().is_some_and(|k| k != "percentage") {
            r.add(
                &f("kind"),
                "Only percentage components are supported; fixed and compound tax are not.",
            );
        }
        let rate = r
            .required_value(&f("rate"), c.rate)
            .and_then(|v| r.decimal(&f("rate"), &v, Decimal::ZERO, Decimal::ONE_HUNDRED));
        let recoverable = r
            .required_value(&f("recoverable"), c.recoverable)
            .and_then(|v| r.boolean(&f("recoverable"), &v));
        if let (Some(code), Some(rate), Some(recoverable)) = (code, rate, recoverable) {
            out.push(TaxComponent {
                code,
                name,
                kind: "percentage",
                rate,
                recoverable,
            });
        }
    }
    Some(out)
}

fn exemptions(r: &mut Rules, v: Option<Vec<TaxExemptionIn>>) -> Vec<TaxExemption> {
    let v = v.unwrap_or_default();
    if v.len() > MAX_EXEMPTIONS {
        r.add("exemptions", "A tax profile has at most 100 exemptions.");
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, e) in v.into_iter().enumerate() {
        let f = |k: &str| format!("exemptions.{i}.{k}");
        let relation = optional(e.relation_id).and_then(|v| uuid(r, &f("relation_id"), &v));
        let category =
            optional(e.product_category_id).and_then(|v| uuid(r, &f("product_category_id"), &v));
        let reason = r.text(&f("reason"), e.reason, REASON_MAX);
        let evidence_ref = r.text(&f("evidence_ref"), e.evidence_ref, REF_MAX);
        if relation.is_some() == category.is_some() {
            r.add(
                &format!("exemptions.{i}"),
                "An exemption names exactly one of relation_id or product_category_id.",
            );
            continue;
        }
        out.push(TaxExemption {
            relation_id: relation,
            product_category_id: category,
            reason,
            evidence_ref,
        });
    }
    out
}

/// POST /tax-profiles payload.
#[derive(Deserialize, ToSchema)]
pub struct TaxProfileCreate {
    /// ISO 3166-1 alpha-2.
    country: Option<String>,
    /// ISO 3166-2 subdivision of `country`; null = whole country.
    jurisdiction_scope: Option<String>,
    effective_from: Option<String>,
    /// Exclusive end; null = open.
    effective_to: Option<String>,
    /// `sale` and/or `purchase`.
    transaction_types: Option<Vec<String>>,
    /// Product category ids; empty = every category.
    #[schema(value_type = Option<Vec<Uuid>>)]
    product_categories: Option<Vec<String>>,
    components: Option<Vec<TaxComponentIn>>,
    exemptions: Option<Vec<TaxExemptionIn>>,
    /// `invoice` or `supply`.
    tax_point: Option<String>,
    evidence_ref: Option<String>,
    /// Verified profile this draft corrects (new revision of its lineage).
    #[schema(value_type = Option<Uuid>)]
    corrects_id: Option<String>,
}

/// Validated create payload.
pub struct NewProfile {
    pub draft: Draft,
    pub corrects: Option<Uuid>,
}

impl Validate for TaxProfileCreate {
    type Valid = NewProfile;

    fn validate(self, r: &mut Rules) -> Option<NewProfile> {
        let country = country(r, self.country);
        let subdivision = subdivision(r, self.jurisdiction_scope);
        let effective_from = date(r, "effective_from", self.effective_from);
        let effective_to = opt_date(r, "effective_to", self.effective_to);
        let transaction_types = types(r, self.transaction_types);
        let product_categories = categories(r, self.product_categories);
        let components = components(r, self.components);
        let exemptions = exemptions(r, self.exemptions);
        let tax_point = tax_point(r, self.tax_point);
        let evidence_ref = r.opt_text("evidence_ref", self.evidence_ref, REF_MAX);
        let corrects = optional(self.corrects_id).and_then(|v| uuid(r, "corrects_id", &v));
        Some(NewProfile {
            draft: Draft {
                country: country?,
                subdivision: subdivision?,
                effective_from: effective_from?,
                effective_to: effective_to?,
                transaction_types: transaction_types?,
                product_categories,
                components: components?,
                exemptions,
                tax_point: tax_point?,
                evidence_ref,
            },
            corrects,
        })
    }
}

/// PATCH /tax-profiles/{id} payload: draft fields; absent = keep, null clears an optional one.
/// `components` / `exemptions` replace the whole list when sent.
#[derive(Deserialize, ToSchema)]
pub struct TaxProfilePatch {
    country: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    jurisdiction_scope: Option<Option<String>>,
    effective_from: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    effective_to: Option<Option<String>>,
    transaction_types: Option<Vec<String>>,
    #[schema(value_type = Option<Vec<Uuid>>)]
    product_categories: Option<Vec<String>>,
    components: Option<Vec<TaxComponentIn>>,
    exemptions: Option<Vec<TaxExemptionIn>>,
    tax_point: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    evidence_ref: Option<Option<String>>,
}

/// Validated patch; `None` keeps the stored value.
#[derive(Clone, Default)]
pub struct Patch {
    pub country: Option<String>,
    pub subdivision: Option<Option<String>>,
    pub effective_from: Option<NaiveDate>,
    pub effective_to: Option<Option<NaiveDate>>,
    pub transaction_types: Option<Vec<String>>,
    pub product_categories: Option<Vec<Uuid>>,
    pub components: Option<Vec<TaxComponent>>,
    pub exemptions: Option<Vec<TaxExemption>>,
    pub tax_point: Option<String>,
    pub evidence_ref: Option<Option<String>>,
}

impl Patch {
    /// `d` with this patch applied.
    pub fn apply(self, mut d: Draft) -> Draft {
        if let Some(v) = self.country {
            d.country = v;
        }
        if let Some(v) = self.subdivision {
            d.subdivision = v;
        }
        if let Some(v) = self.effective_from {
            d.effective_from = v;
        }
        if let Some(v) = self.effective_to {
            d.effective_to = v;
        }
        if let Some(v) = self.transaction_types {
            d.transaction_types = v;
        }
        if let Some(v) = self.product_categories {
            d.product_categories = v;
        }
        if let Some(v) = self.components {
            d.components = v;
        }
        if let Some(v) = self.exemptions {
            d.exemptions = v;
        }
        if let Some(v) = self.tax_point {
            d.tax_point = v;
        }
        if let Some(v) = self.evidence_ref {
            d.evidence_ref = v;
        }
        d
    }
}

impl Validate for TaxProfilePatch {
    type Valid = Patch;

    fn validate(self, r: &mut Rules) -> Option<Patch> {
        let mut p = Patch::default();
        if self.country.is_some() {
            p.country = country(r, self.country);
        }
        if let Some(v) = self.jurisdiction_scope {
            p.subdivision = subdivision(r, v);
        }
        if self.effective_from.is_some() {
            p.effective_from = date(r, "effective_from", self.effective_from);
        }
        if let Some(v) = self.effective_to {
            p.effective_to = opt_date(r, "effective_to", v);
        }
        if self.transaction_types.is_some() {
            p.transaction_types = types(r, self.transaction_types);
        }
        if self.product_categories.is_some() {
            p.product_categories = Some(categories(r, self.product_categories));
        }
        if self.components.is_some() {
            p.components = components(r, self.components);
        }
        if self.exemptions.is_some() {
            p.exemptions = Some(exemptions(r, self.exemptions));
        }
        if self.tax_point.is_some() {
            p.tax_point = tax_point(r, self.tax_point);
        }
        if let Some(v) = self.evidence_ref {
            p.evidence_ref = Some(r.opt_text("evidence_ref", v, REF_MAX));
        }
        Some(p)
    }
}

/// POST /tax-profiles/{id}/verify payload.
#[derive(Deserialize, ToSchema)]
pub struct TaxProfileVerify {
    /// From `POST /tax-profiles/{id}/verify/review`.
    review_fingerprint: Option<String>,
    /// Evidence the reviewer relied on.
    evidence_ref: Option<String>,
}

/// Validated verify payload.
pub struct VerifyInput {
    pub review_fingerprint: String,
    pub evidence_ref: String,
}

impl Validate for TaxProfileVerify {
    type Valid = VerifyInput;

    fn validate(self, r: &mut Rules) -> Option<VerifyInput> {
        let review_fingerprint = r.required("review_fingerprint", self.review_fingerprint);
        let evidence_ref = r.text("evidence_ref", self.evidence_ref, REF_MAX);
        Some(VerifyInput {
            review_fingerprint: review_fingerprint?,
            evidence_ref,
        })
    }
}

/// GET /tax-profiles query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TaxProfileListQuery {
    /// ISO 3166-1 alpha-2, any case.
    pub country: Option<String>,
    /// `draft` or `verified`.
    pub state: Option<String>,
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

impl TaxProfileListQuery {
    /// (country, state) filters; 422 when malformed.
    pub fn filters(&self) -> Result<(Option<String>, Option<String>), AppError> {
        let mut r = Rules::default();
        let country = optional(self.country.clone()).and_then(|c| r.country("country", &c));
        let state = optional(self.state.clone());
        if let Some(s) = &state {
            r.check(
                "state",
                s == "draft" || s == "verified",
                "The state must be draft or verified.",
            );
        }
        r.into_result()?;
        Ok((country, state))
    }
}

/// A tax profile revision.
#[derive(Serialize, ToSchema)]
pub struct TaxProfileResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// Revision 1's id; corrections share it.
    pub lineage_id: Uuid,
    pub revision: i32,
    pub corrects_id: Option<Uuid>,
    pub superseded_by_id: Option<Uuid>,
    /// `draft` or `verified`.
    pub state: String,
    pub version: i64,
    pub country: String,
    /// ISO 3166-2 subdivision; null = whole country.
    pub jurisdiction_scope: Option<String>,
    pub effective_from: NaiveDate,
    /// Exclusive end.
    pub effective_to: Option<NaiveDate>,
    pub transaction_types: Vec<String>,
    pub product_categories: Vec<Uuid>,
    pub components: Vec<TaxComponent>,
    pub exemptions: Vec<TaxExemption>,
    pub tax_point: String,
    pub evidence_ref: Option<String>,
    pub verified_by: Option<Uuid>,
    pub verified_at: Option<DateTime<Utc>>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// `update`, `verify` (draft); `correct` (verified, current).
    pub allowed_actions: Vec<&'static str>,
}

/// Server review of a draft before verify.
#[derive(Serialize, ToSchema)]
pub struct TaxProfileReview {
    pub profile: TaxProfileResource,
    /// Hex sha256 the verify call must echo.
    pub review_fingerprint: String,
}
