//! Cash places (Flow ERP M4): active book cash per drawer/vault (posted place rows, counts
//! excluded) and the reviewed cash adjustment shared by the Office and the privileged Till.
//! Lock order: command row → shared book gate → place row (register or vault) → accounts +
//! period (active books, in `post_source`) → effect → journal number → adjustment + ledger row.

use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::json::Validate,
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::Created,
            gate,
            journals::{SourceJournal, post_source},
            repo as books,
            service::{created, stale, unsupported},
            settlement::Side,
        },
        cash::{
            dto::{
                AdjustActor, AdjustInput, AdjustReviewResource, CashAdjustmentResource,
                CashPlaceResource, CountedAmount, JournalPlan, Place, PlaceCurrency, PlaceRef,
                ReviewInput,
            },
            repo::{self, NewAdjustment},
        },
    },
};

/// Effect source kind of cash adjustments.
const SOURCE: &str = "cash_adjustment";
const BAD_PLACE: &str = "The selected cash place is invalid.";
const NOTHING: &str = "The count matches the book cash; there is nothing to adjust.";
const TOO_PRECISE: &str = "The counted amount has more decimals than the place's currency.";
const FOREIGN: &str =
    "Cash in another currency than the book's functional currency can't be adjusted yet.";

/// Who approves: always a member; the Till adds its cashier as context.
#[derive(Clone, Copy)]
pub struct Approver {
    pub member: Uuid,
    pub till_user_id: Option<Uuid>,
}

fn split(place: Place) -> (Option<Uuid>, Option<Uuid>) {
    match place {
        Place::Drawer(id) => (Some(id), None),
        Place::Vault(id) => (None, Some(id)),
    }
}

/// Facts under the place lock; `fingerprint` binds them for the post.
struct Facts {
    review: AdjustReviewResource,
    currency_id: Uuid,
    fingerprint: [u8; 32],
}

async fn facts(conn: &mut PgConnection, ws: Uuid, input: &ReviewInput) -> Result<Facts, AppError> {
    let (register, vault) = split(input.place);
    let field = |m: &str| AppError::field("cash_place", m);
    let place = repo::lock_place(conn, ws, register, vault)
        .await?
        .ok_or_else(|| field(BAD_PLACE))?;
    let date = match input.date {
        Some(d) => d,
        None => repo::book_today(conn, ws).await?,
    };
    let (major, minor) = match &input.counted {
        CountedAmount::Major(v) => (Some(*v), None),
        CountedAmount::Minor(v) => (None, Some(*v)),
    };
    let c = repo::count_facts(conn, ws, register, vault, major, minor, place.decimals).await?;
    let (Some(counted), Some(difference), Some(amount)) = (c.counted, c.difference, c.amount)
    else {
        return Err(AppError::field("counted_amount", TOO_PRECISE));
    };
    if difference == 0 {
        return Err(AppError::field("counted_amount", NOTHING));
    }
    let book = books::settings(conn, ws, false).await?;
    let active = book.state == "active";
    if active && book.functional_currency_id != Some(place.currency_id) {
        return Err(unsupported(FOREIGN));
    }
    let journal = active.then(|| {
        let (debit_role, credit_role) = if difference > 0 {
            ("cash", "cash_over_short")
        } else {
            ("cash_over_short", "cash")
        };
        JournalPlan {
            debit_role,
            credit_role,
            amount,
        }
    });
    let fingerprint: [u8; 32] = Sha256::digest(
        json!({
            "place": [input.place.kind(), input.place.id()], "currency": place.currency_id,
            "expected": c.expected, "counted": counted, "difference": difference, "date": date,
            "book": book.state,
            "journal": journal.as_ref().map(|j| (j.debit_role, j.credit_role, j.amount)),
        })
        .to_string()
        .as_bytes(),
    )
    .into();
    Ok(Facts {
        currency_id: place.currency_id,
        fingerprint,
        review: AdjustReviewResource {
            cash_place: PlaceRef {
                kind: input.place.kind(),
                id: input.place.id(),
            },
            currency: PlaceCurrency {
                id: place.currency_id,
                code: place.code,
                decimals: place.decimals,
            },
            expected: c.expected,
            counted,
            difference,
            accounting_date: date,
            book: book.state,
            journal,
            review_fingerprint: hex::encode(fingerprint),
        },
    })
}

/// Review: no writes (the place lock ends with the rolled-back transaction).
pub async fn review(
    state: &AppState,
    ws: Uuid,
    input: &ReviewInput,
) -> Result<AdjustReviewResource, AppError> {
    let mut tx = state.db.begin().await?;
    let f = facts(&mut tx, ws, input).await?;
    tx.rollback().await?;
    Ok(f.review)
}

/// Post a reviewed adjustment once: place movement in every book; cash_over_short/cash journal
/// in active books. 409 `stale_document` when the facts moved since the review.
pub async fn adjust<T>(
    state: &AppState,
    ws: Uuid,
    operation: &str,
    by: Approver,
    cmd: Command<T>,
) -> Result<Answer, AppError>
where
    T: Validate<Valid = Created<AdjustInput>>,
{
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, operation, by.member, &cmd).await? {
        return Ok(a);
    }
    gate::shared(&mut tx, &[ws]).await?;
    let input = &cmd.input.payload;
    let f = facts(&mut tx, ws, &input.review).await?;
    if f.review.review_fingerprint != input.fingerprint {
        return Err(stale());
    }
    let r = f.review;
    let id = Uuid::now_v7();
    let (effect_id, journal_entry_id) = match &r.journal {
        Some(j) => {
            let entry = post_source(
                &mut tx,
                ws,
                &SourceJournal {
                    source_kind: SOURCE,
                    source_id: id,
                    effect_kind: "posted",
                    date: r.accounting_date,
                    reason: &input.reason,
                    debit: Side::Role(j.debit_role),
                    credit: Side::Role(j.credit_role),
                    relation_id: None,
                    bank_account_id: None,
                    amount: j.amount,
                    fingerprint: &f.fingerprint,
                    by: by.member,
                },
            )
            .await?;
            (repo::entry_effect(&mut tx, ws, entry).await?, Some(entry))
        }
        None => {
            let effect =
                books::insert_effect(&mut tx, ws, SOURCE, id, "posted", &f.fingerprint, by.member)
                    .await?;
            (effect, None)
        }
    };
    let (register_id, vault_id) = split(input.review.place);
    let created_at = repo::insert_adjustment(
        &mut tx,
        ws,
        &NewAdjustment {
            id,
            register_id,
            vault_id,
            currency_id: f.currency_id,
            expected: r.expected,
            counted: r.counted,
            difference: r.difference,
            evidence: &input.evidence,
            reason: &input.reason,
            date: r.accounting_date,
            member_user_id: by.member,
            till_user_id: by.till_user_id,
            effect_id,
            journal_entry_id,
        },
    )
    .await?;
    let answer = created(&CashAdjustmentResource {
        id,
        cash_place: r.cash_place,
        currency: r.currency,
        expected: r.expected,
        counted: r.counted,
        difference: r.difference,
        accounting_date: r.accounting_date,
        count_evidence_ref: input.evidence.clone(),
        reason: input.reason.clone(),
        actor: AdjustActor {
            kind: "member",
            user_id: by.member,
        },
        till_user_id: by.till_user_id,
        journal_entry_id,
        created_at,
    });
    idempotency::command_finish(&mut tx, ws, operation, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Every drawer and vault with its active book cash.
pub async fn list(state: &AppState, ws: Uuid) -> Result<Vec<CashPlaceResource>, AppError> {
    Ok(repo::list_places(&state.db, ws)
        .await?
        .into_iter()
        .map(|p| CashPlaceResource {
            kind: p.kind,
            id: p.id,
            name: p.name,
            currency: PlaceCurrency {
                id: p.currency_id,
                code: p.code,
                decimals: p.decimals,
            },
            balance: p.balance,
            open_shift_id: p.open_shift_id,
        })
        .collect())
}
