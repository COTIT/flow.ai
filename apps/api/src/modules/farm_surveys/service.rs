//! Survey rules (RUAHA `survey_guard`, `survey_block_reason`, `app_survey_submit`). Admin
//! authors and publishes; ops reads everything; farmers and officers see live/closed surveys
//! of their villages. Answering: once per household, eligibility from `eligibility::block_reason`
//! (shared with the list), one transaction for response, answers, voucher and its first event.

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{farm::FarmAccess, workspace::NOT_ALLOWED},
    modules::{
        farm_records::service::{one, path_id},
        farm_surveys::{
            dto::{
                CountMinor, Eligibility, Money, QuestionInput, ResponseInput, Submitted,
                SurveyInput, SurveyResults,
            },
            eligibility::{CallerFacts, HouseholdFacts, SurveyFacts, block_reason},
            repo::{
                self, NewVoucher, QuestionCols, SurveyCols, SurveyRow, SurveyScope,
                SurveyVoucherRow,
            },
        },
    },
};

/// Crockford base32, upper case: what the household reads out or shows as a QR code.
const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn unprocessable(message: impl Into<String>) -> AppError {
    AppError::Unprocessable(message.into())
}

fn scope<'a>(access: &FarmAccess, villages: &'a [Uuid]) -> SurveyScope<'a> {
    SurveyScope {
        ws: access.workspace_id,
        all: access.manages(),
        villages,
    }
}

fn require_admin(access: &FarmAccess) -> Result<(), AppError> {
    access
        .is_admin()
        .then_some(())
        .ok_or(AppError::Forbidden(NOT_ALLOWED))
}

/// One visible survey.
pub async fn get(state: &AppState, access: &FarmAccess, id: Uuid) -> Result<SurveyRow, AppError> {
    let villages = access.member_villages();
    one(repo::surveys(&state.db, &scope(access, &villages), Some(id), 1, 0).await?)
}

pub async fn list(
    state: &AppState,
    access: &FarmAccess,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SurveyRow>, i64), AppError> {
    let villages = access.member_villages();
    Ok(repo::surveys(&state.db, &scope(access, &villages), None, limit, offset).await?)
}

/// Checks the references of a survey's fields.
async fn check_refs(
    state: &AppState,
    access: &FarmAccess,
    c: &SurveyCols<'_>,
) -> Result<(), AppError> {
    let ws = access.workspace_id;
    if !repo::currency_exists(&state.db, ws, c.currency_id).await? {
        return Err(AppError::field(
            "currency_id",
            "no such currency in this workspace",
        ));
    }
    if let Some(v) = c.village_id
        && !crate::modules::farm_memberships::repo::village_exists(&state.db, ws, v).await?
    {
        return Err(AppError::field(
            "village_id",
            "no such village in this workspace",
        ));
    }
    Ok(())
}

/// A new draft.
pub async fn create(
    state: &AppState,
    access: &FarmAccess,
    i: SurveyInput,
) -> Result<SurveyRow, AppError> {
    require_admin(access)?;
    let title = i
        .title_en
        .ok_or_else(|| AppError::field("title_en", "The title en field is required."))?;
    let reward = i
        .reward_minor
        .ok_or_else(|| AppError::field("reward_minor", "The reward minor field is required."))?;
    let currency = i
        .currency_id
        .ok_or_else(|| AppError::field("currency_id", "The currency id field is required."))?;
    let (title_sw, description_en, description_sw) = (
        i.title_sw.flatten(),
        i.description_en.flatten(),
        i.description_sw.flatten(),
    );
    let cols = SurveyCols {
        village_id: i.village_id.flatten(),
        title_en: &title,
        title_sw: title_sw.as_deref(),
        description_en: description_en.as_deref(),
        description_sw: description_sw.as_deref(),
        reward_minor: reward,
        currency_id: currency,
        max_households: i.max_households.flatten(),
        audit_percent: i.audit_percent.unwrap_or(15),
        closes_at: i.closes_at.flatten(),
    };
    check_refs(state, access, &cols).await?;
    let mut tx = access.begin(&state.db).await?;
    let id = repo::insert(&mut *tx, access.workspace_id, access.user_id, &cols).await?;
    tx.commit().await?;
    get(state, access, id).await
}

/// Change a draft; a live or closed survey is frozen.
pub async fn update(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    i: SurveyInput,
) -> Result<SurveyRow, AppError> {
    require_admin(access)?;
    let id = path_id(id)?;
    let cur = get(state, access, id).await?;
    if cur.status != "draft" {
        return Err(unprocessable("a live survey cannot be edited"));
    }
    let title = i.title_en.unwrap_or(cur.title_en);
    let title_sw = i.title_sw.unwrap_or(cur.title_sw);
    let description_en = i.description_en.unwrap_or(cur.description_en);
    let description_sw = i.description_sw.unwrap_or(cur.description_sw);
    let cols = SurveyCols {
        village_id: i.village_id.unwrap_or(cur.village_id),
        title_en: &title,
        title_sw: title_sw.as_deref(),
        description_en: description_en.as_deref(),
        description_sw: description_sw.as_deref(),
        reward_minor: i.reward_minor.unwrap_or(cur.reward_minor),
        currency_id: i.currency_id.unwrap_or(cur.currency_id),
        max_households: i.max_households.unwrap_or(cur.max_households),
        audit_percent: i.audit_percent.unwrap_or(cur.audit_percent),
        closes_at: i.closes_at.unwrap_or(cur.closes_at),
    };
    check_refs(state, access, &cols).await?;
    let mut tx = access.begin(&state.db).await?;
    // The lock and the trigger catch a publish that raced this edit.
    let lock = repo::lock(&mut *tx, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if lock.status != "draft" {
        return Err(unprocessable("a live survey cannot be edited"));
    }
    repo::update(&mut *tx, access.workspace_id, id, &cols).await?;
    tx.commit().await?;
    get(state, access, id).await
}

/// Lock a draft for a question change; refusals in RUAHA's words.
async fn draft_for_questions(
    tx: &mut sqlx::PgConnection,
    access: &FarmAccess,
    survey: Uuid,
) -> Result<(), AppError> {
    let lock = repo::lock(&mut *tx, access.workspace_id, survey)
        .await?
        .ok_or(AppError::NotFound)?;
    if lock.status != "draft" {
        return Err(unprocessable(
            "questions can only change while the survey is a draft",
        ));
    }
    Ok(())
}

fn question_cols(q: &QuestionInput) -> QuestionCols<'_> {
    QuestionCols {
        position: q.position,
        kind: &q.kind,
        prompt_en: &q.prompt_en,
        prompt_sw: q.prompt_sw.as_deref(),
        required: q.required,
        options: &q.options,
    }
}

pub async fn add_question(
    state: &AppState,
    access: &FarmAccess,
    survey: &str,
    q: QuestionInput,
) -> Result<Uuid, AppError> {
    require_admin(access)?;
    let survey = path_id(survey)?;
    let mut tx = access.begin(&state.db).await?;
    draft_for_questions(&mut tx, access, survey).await?;
    let id =
        repo::insert_question(&mut *tx, access.workspace_id, survey, &question_cols(&q)).await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn replace_question(
    state: &AppState,
    access: &FarmAccess,
    survey: &str,
    id: &str,
    q: QuestionInput,
) -> Result<(), AppError> {
    require_admin(access)?;
    let (survey, id) = (path_id(survey)?, path_id(id)?);
    let mut tx = access.begin(&state.db).await?;
    draft_for_questions(&mut tx, access, survey).await?;
    if !repo::update_question(&mut *tx, survey, id, &question_cols(&q)).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn remove_question(
    state: &AppState,
    access: &FarmAccess,
    survey: &str,
    id: &str,
) -> Result<(), AppError> {
    require_admin(access)?;
    let (survey, id) = (path_id(survey)?, path_id(id)?);
    let mut tx = access.begin(&state.db).await?;
    draft_for_questions(&mut tx, access, survey).await?;
    if !repo::delete_question(&mut *tx, survey, id).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// What a survey needs before it goes live.
fn publish_problem(s: &SurveyRow) -> Option<&'static str> {
    let questions = s
        .questions
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if questions.is_empty() {
        return Some("a survey needs at least one question before it goes live");
    }
    let choice = |q: &&Value| matches!(q["kind"].as_str(), Some("single_choice" | "multi_choice"));
    let options = |q: &Value| q["options"].as_array().cloned().unwrap_or_default();
    if questions
        .iter()
        .filter(choice)
        .any(|q| options(q).len() < 2)
    {
        return Some("every choice question needs at least two options");
    }
    let blank = |o: &Value, k: &str| o[k].as_str().is_none_or(|s| s.trim().is_empty());
    if questions
        .iter()
        .flat_map(options)
        .any(|o| blank(&o, "value") || blank(&o, "label_en"))
    {
        return Some("every option needs a value and an English label");
    }
    if s.closes_at.is_some_and(|t| t <= Utc::now()) {
        return Some("the closing date must be in the future");
    }
    None
}

pub async fn publish(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
) -> Result<SurveyRow, AppError> {
    require_admin(access)?;
    let id = path_id(id)?;
    let mut tx = access.begin(&state.db).await?;
    let lock = repo::lock(&mut *tx, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if lock.status != "draft" {
        return Err(unprocessable(format!(
            "illegal survey transition {} -> live",
            lock.status
        )));
    }
    let cur = get(state, access, id).await?;
    if let Some(problem) = publish_problem(&cur) {
        return Err(unprocessable(problem));
    }
    repo::publish(&mut *tx, id, access.user_id).await?;
    tx.commit().await?;
    get(state, access, id).await
}

pub async fn close(state: &AppState, access: &FarmAccess, id: &str) -> Result<SurveyRow, AppError> {
    require_admin(access)?;
    let id = path_id(id)?;
    let mut tx = access.begin(&state.db).await?;
    let lock = repo::lock(&mut *tx, access.workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if lock.status != "live" {
        return Err(unprocessable(format!(
            "illegal survey transition {} -> closed",
            lock.status
        )));
    }
    repo::close(&mut *tx, id).await?;
    tx.commit().await?;
    get(state, access, id).await
}

// ---- answering ----

fn survey_facts(s: &SurveyRow) -> SurveyFacts {
    SurveyFacts {
        status: s.status.clone(),
        closes_at: s.closes_at,
        village_id: s.village_id,
        published_at: s.published_at,
        max_households: s.max_households,
        answered: s.answered,
    }
}

fn household_facts(rows: &[repo::HouseholdRow], survey: Uuid) -> Vec<HouseholdFacts> {
    rows.iter()
        .map(|h| HouseholdFacts {
            id: h.id,
            village_id: h.village_id,
            verification: h.verification.clone(),
            captured_by: h.captured_by,
            verified_by: h.verified_by,
            created_at: h.created_at,
            answered_this: h.answered.contains(&survey),
        })
        .collect()
}

/// Every live or closed survey the caller sees, with whether their household may answer it.
/// The reason is the same text the submit would answer with.
pub async fn eligibility(
    state: &AppState,
    access: &FarmAccess,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Eligibility>, i64), AppError> {
    let villages = access.member_villages();
    let (surveys, total) =
        repo::surveys(&state.db, &scope(access, &villages), None, limit, offset).await?;
    let must_change = repo::must_change_password(&state.db, access.user_id).await?;
    let households = match access.person_id {
        Some(p) => repo::households_of(&state.db, p).await?,
        None => Vec::new(),
    };
    let house_ids: Vec<Uuid> = households.iter().map(|h| h.id).collect();
    let answered = repo::answered_by(&state.db, &house_ids).await?;
    let now = Utc::now();
    let rows = surveys
        .iter()
        .filter(|s| s.status != "draft")
        .map(|s| {
            let facts = household_facts(&households, s.id);
            let caller = CallerFacts {
                linked: access.person_id.is_some(),
                must_change_password: must_change,
                households: &facts,
            };
            let done = answered.iter().find(|a| a.survey_id == s.id);
            let reason = block_reason(&survey_facts(s), &caller, now);
            Eligibility {
                survey_id: s.id,
                eligible: reason.is_none(),
                reason: reason.map(str::to_owned),
                response_id: done.map(|a| a.response_id),
                voucher_id: done.and_then(|a| a.voucher_id),
            }
        })
        .collect();
    // A manager's page also counts drafts, which get no eligibility row.
    Ok((rows, total))
}

/// Check and clean one answer; None = left blank.
fn clean_answer(
    position: i32,
    kind: &str,
    options: &Value,
    v: &Value,
) -> Result<Option<Value>, AppError> {
    let blank = v.is_null()
        || v.as_str().is_some_and(|s| s.trim().is_empty())
        || v.as_array().is_some_and(Vec::is_empty);
    if blank {
        return Ok(None);
    }
    let listed = |x: &str| {
        options
            .as_array()
            .is_some_and(|o| o.iter().any(|o| o["value"].as_str() == Some(x)))
    };
    let fail = |m: &str| Err(unprocessable(format!("question {position}: {m}")));
    match kind {
        "single_choice" => match v.as_str() {
            Some(s) if listed(s) => Ok(Some(v.clone())),
            _ => fail("choose one of the listed options"),
        },
        "multi_choice" => match v.as_array() {
            Some(a) if a.iter().all(|x| x.as_str().is_some_and(listed)) => Ok(Some(v.clone())),
            _ => fail("choose from the listed options"),
        },
        "number" => match v {
            Value::Number(_) => Ok(Some(v.clone())),
            Value::String(s) => match s.trim().parse::<f64>() {
                Ok(n) if n.is_finite() => Ok(Some(json!(n))),
                _ => fail("enter a number"),
            },
            _ => fail("enter a number"),
        },
        "text" => match v.as_str() {
            Some(s) if s.chars().count() <= 2000 => Ok(Some(json!(s.trim()))),
            _ => fail("answer in at most 2000 characters"),
        },
        "yes_no" => match v {
            Value::Bool(_) => Ok(Some(v.clone())),
            _ => fail("answer yes or no"),
        },
        _ => fail("unknown question type"),
    }
}

/// Validate answers against the survey's questions: (question ids, values) to store.
fn clean_answers(
    s: &SurveyRow,
    answers: &serde_json::Map<String, Value>,
) -> Result<(Vec<Uuid>, Vec<Value>), AppError> {
    let questions = s
        .questions
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let known = |k: &str| questions.iter().any(|q| q["id"].as_str() == Some(k));
    if answers.keys().any(|k| !known(k)) {
        return Err(unprocessable(
            "an answer does not match any question in this survey",
        ));
    }
    let (mut ids, mut values) = (Vec::new(), Vec::new());
    for q in questions {
        let id = q["id"].as_str().unwrap_or_default();
        let position = q["position"]
            .as_i64()
            .and_then(|p| i32::try_from(p).ok())
            .unwrap_or(0);
        let kind = q["kind"].as_str().unwrap_or_default();
        let value = answers
            .get(id)
            .map(|v| clean_answer(position, kind, &q["options"], v))
            .transpose()?
            .flatten();
        match value {
            None if q["required"].as_bool().unwrap_or(true) => {
                return Err(unprocessable(format!("question {position} is required")));
            }
            None => {}
            Some(v) => {
                ids.push(Uuid::parse_str(id).map_err(|e| AppError::Internal(e.to_string()))?);
                values.push(v);
            }
        }
    }
    Ok((ids, values))
}

fn random_bytes<const N: usize>() -> Result<[u8; N], AppError> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| AppError::Internal(format!("os rng: {e}")))?;
    Ok(b)
}

/// Answer a survey: response, answers, voucher and its `issued` event in one transaction.
pub async fn submit(
    state: &AppState,
    access: &FarmAccess,
    survey: &str,
    input: ResponseInput,
) -> Result<(Submitted, bool), AppError> {
    let survey = path_id(survey)?;
    let ws = access.workspace_id;
    if let Some(r) = repo::replay(&state.db, ws, input.client_ref).await? {
        if r.captured_by != Some(access.user_id) {
            return Err(unprocessable(
                "this answer reference belongs to someone else",
            ));
        }
        let submitted = Submitted {
            response_id: r.response_id,
            voucher_id: r.voucher_id,
            code: r.code,
            amount: Money {
                minor: r.amount_minor,
                currency: r.currency,
                decimals: r.decimals,
                currency_id: r.currency_id,
            },
            expires_at: r.expires_at,
            replayed: true,
        };
        return Ok((submitted, false));
    }
    let mut tx = access.begin(&state.db).await?;
    // One submit per survey at a time, so the household cap can't race.
    repo::lock(&mut *tx, ws, survey)
        .await?
        .ok_or(AppError::NotFound)?;
    let s = get(state, access, survey).await?;
    let households = match access.person_id {
        Some(p) => repo::households_of(&mut *tx, p).await?,
        None => Vec::new(),
    };
    let facts = household_facts(&households, s.id);
    let caller = CallerFacts {
        linked: access.person_id.is_some(),
        must_change_password: repo::must_change_password(&mut *tx, access.user_id).await?,
        households: &facts,
    };
    if let Some(reason) = block_reason(&survey_facts(&s), &caller, Utc::now()) {
        return Err(unprocessable(reason));
    }
    let (Some(person), [house]) = (access.person_id, facts.as_slice()) else {
        return Err(AppError::Internal(
            "eligible caller without one household".into(),
        ));
    };
    let (question_ids, values) = clean_answers(&s, &input.answers)?;
    let response = repo::insert_response(
        &mut *tx,
        ws,
        house.village_id,
        s.id,
        house.id,
        person,
        input.client_ref,
        access.user_id,
    )
    .await?;
    repo::insert_answers(&mut *tx, ws, response, &question_ids, &values).await?;
    let audit_required = i16::from(random_bytes::<1>()?[0] % 100) < s.audit_percent;
    let mut issued = None;
    for _ in 0..5 {
        let code: String = random_bytes::<10>()?
            .iter()
            .map(|b| char::from(CODE_ALPHABET[usize::from(b % 32)]))
            .collect();
        let v = NewVoucher {
            ws,
            village: house.village_id,
            response,
            survey: s.id,
            household: house.id,
            code: &code,
            amount_minor: s.reward_minor,
            currency_id: s.currency_id,
            audit_required,
        };
        if let Some((id, expires_at)) = repo::insert_voucher(&mut *tx, &v).await? {
            issued = Some((id, code, expires_at));
            break;
        }
    }
    let (voucher, code, expires_at) =
        issued.ok_or_else(|| AppError::Internal("no free voucher code".into()))?;
    let detail = json!({ "audit_required": audit_required, "amount_minor": s.reward_minor, "currency": s.currency });
    repo::insert_event(
        &mut *tx,
        ws,
        voucher,
        "issued",
        access.user_id,
        access.role_label(),
        &detail,
    )
    .await?;
    tx.commit().await?;
    let submitted = Submitted {
        response_id: response,
        voucher_id: voucher,
        code,
        amount: Money {
            minor: s.reward_minor,
            currency: s.currency,
            decimals: s.decimals,
            currency_id: s.currency_id,
        },
        expires_at,
        replayed: false,
    };
    Ok((submitted, true))
}

// ---- staff views ----

/// Answer counts and voucher money of a survey: staff only.
pub async fn results(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
) -> Result<SurveyResults, AppError> {
    if !access.is_staff() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let s = get(state, access, path_id(id)?).await?;
    let sum = repo::summary(&state.db, s.id).await?;
    let (tally, numbers) = repo::tallies(&state.db, s.id).await?;
    Ok(SurveyResults {
        responses: sum.responses,
        currency: s.currency,
        issued: CountMinor {
            count: sum.issued_count,
            minor: sum.issued_minor,
        },
        redeemed: CountMinor {
            count: sum.redeemed_count,
            minor: sum.redeemed_minor,
        },
        outstanding: CountMinor {
            count: sum.outstanding_count,
            minor: sum.outstanding_minor,
        },
        expired_count: sum.expired_count,
        void_count: sum.void_count,
        tally,
        numbers,
    })
}

/// A survey's vouchers, without codes: ops and admin.
pub async fn vouchers(
    state: &AppState,
    access: &FarmAccess,
    id: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SurveyVoucherRow>, i64), AppError> {
    if !access.manages() {
        return Err(AppError::Forbidden(NOT_ALLOWED));
    }
    let s = get(state, access, path_id(id)?).await?;
    Ok(repo::survey_vouchers(&state.db, access.workspace_id, s.id, limit, offset).await?)
}
