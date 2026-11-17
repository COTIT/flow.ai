//! POST /invoices/{id}/issue (+ /review): number and book a draft without sending email
//! (masterplan §12/§15). Same work as a send's numbering transaction (`send::issue_in`); in an
//! active book reclassifies linked recognitions or recognises a standalone invoice. Reviewed
//! (fingerprint), C/A envelope, required Idempotency-Key, permanent replay.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{
        json::{Rules, Validate},
        workspace::WorkspaceAccess,
    },
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::{Action, date, opt_uuid},
            service::{ok, stale},
        },
        currencies,
        invoices::{
            actions::{self, Facts, InvoiceAction},
            repo,
            send::{Addressee, issue_in},
        },
        purchasing::service::amount,
        shipments::{billing, recognition},
        stock::value,
    },
};

/// POST .../issue/review body.
#[derive(Deserialize, ToSchema)]
pub struct IssueReviewBody {
    /// `YYYY-MM-DD`; in an active book today in its timezone.
    pub accounting_date: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    pub addressed_contact_id: Option<String>,
}

impl Validate for IssueReviewBody {
    type Valid = (NaiveDate, Option<Uuid>);

    fn validate(self, r: &mut Rules) -> Option<(NaiveDate, Option<Uuid>)> {
        let d = date(r, "accounting_date", self.accounting_date);
        let c = opt_uuid(r, "addressed_contact_id", self.addressed_contact_id);
        Some((d?, c?))
    }
}

/// POST .../issue payload.
#[derive(Deserialize, ToSchema)]
pub struct IssuePayload {
    pub accounting_date: Option<String>,
    #[schema(value_type = Option<Uuid>)]
    pub addressed_contact_id: Option<String>,
    /// From `POST .../issue/review`.
    pub review_fingerprint: Option<String>,
}

/// Validated issue: date, optional contact, fingerprint.
pub struct IssueInput {
    pub date: NaiveDate,
    pub contact: Option<Uuid>,
    pub fingerprint: String,
}

impl Validate for IssuePayload {
    type Valid = IssueInput;

    fn validate(self, r: &mut Rules) -> Option<IssueInput> {
        let d = date(r, "accounting_date", self.accounting_date);
        let c = opt_uuid(r, "addressed_contact_id", self.addressed_contact_id);
        let f = r.required("review_fingerprint", self.review_fingerprint);
        Some(IssueInput {
            date: d?,
            contact: c?,
            fingerprint: f?,
        })
    }
}

/// A linked recognition and what its issue reclassifies (0 = settled, document only).
#[derive(Serialize, ToSchema)]
pub struct IssueReclass {
    pub recognition_id: Uuid,
    #[schema(example = "48.40")]
    pub amount: String,
}

/// A standalone invoice's recognition.
#[derive(Serialize, ToSchema)]
pub struct IssueRecognition {
    pub net: String,
    pub tax: String,
}

/// What issuing does now (no writes). Book effects are empty in a setup book.
#[derive(Serialize, ToSchema)]
pub struct IssueReview {
    pub invoice_id: Uuid,
    pub accounting_date: NaiveDate,
    pub reclassify: Vec<IssueReclass>,
    pub recognition: Option<IssueRecognition>,
    pub review_fingerprint: String,
}

/// The issued invoice.
#[derive(Serialize, ToSchema)]
pub struct Issued {
    pub id: Uuid,
    pub invoice_number: Option<i64>,
    pub status: String,
}

async fn review_in(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    (date, field): (NaiveDate, &str),
) -> Result<IssueReview, AppError> {
    let locked = repo::lock(&mut *conn, ws, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !actions::allows(InvoiceAction::Send, &Facts::of(&locked, false)) {
        return Err(AppError::Unprocessable(
            "Only a draft invoice can be issued.".into(),
        ));
    }
    let dp = currencies::repo::share_decimals(&mut *conn, ws, locked.currency_id)
        .await?
        .unwrap_or(2);
    let (reclassify, recognition) = if value::active(&mut *conn, ws).await? {
        recognition::check_date(&mut *conn, ws, date, field).await?;
        let p = billing::plan(&mut *conn, ws, id, (locked.currency_id, locked.is_credit)).await?;
        (p.reclassify, p.standalone)
    } else {
        (Vec::new(), None)
    };
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({ "issue": id, "date": date, "total": locked.total, "relation": locked.relation_id,
                "reclassify": reclassify, "recognition": recognition })
        .to_string(),
    )
    .into();
    Ok(IssueReview {
        invoice_id: id,
        accounting_date: date,
        reclassify: reclassify
            .into_iter()
            .map(|(recognition_id, a)| IssueReclass {
                recognition_id,
                amount: amount(a, dp),
            })
            .collect(),
        recognition: recognition.map(|(net, tax)| IssueRecognition {
            net: amount(net, dp),
            tax: amount(tax, dp),
        }),
        review_fingerprint: hex::encode(fingerprint),
    })
}

/// POST .../issue/review: no writes.
pub async fn review(
    state: &AppState,
    ws: Uuid,
    id: Uuid,
    date: NaiveDate,
) -> Result<IssueReview, AppError> {
    let mut tx = state.db.begin().await?;
    let r = review_in(&mut tx, ws, id, (date, "accounting_date")).await?;
    tx.rollback().await?;
    Ok(r)
}

/// POST .../issue: issue the reviewed draft once.
pub async fn issue(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<IssuePayload>>,
) -> Result<Answer, AppError> {
    const OP: &str = "invoices_issue";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let input = &cmd.input.payload;
    // shortcut: invoices carry no version; the review fingerprint guards what changed.
    let review = review_in(&mut tx, ws, id, (input.date, "payload.accounting_date")).await?;
    if review.review_fingerprint != input.fingerprint {
        return Err(stale());
    }
    issue_in(
        &mut tx,
        state,
        (ws, access.user_id, id),
        Addressee::Issue(input.contact),
        Some(input.date),
    )
    .await?;
    let (invoice_number, status): (Option<i64>, String) = sqlx::query_as(
        "SELECT invoice_number, status FROM invoices WHERE workspace_id = $1 AND id = $2",
    )
    .bind(ws)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let answer = ok(&Issued {
        id,
        invoice_number,
        status,
    });
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}
