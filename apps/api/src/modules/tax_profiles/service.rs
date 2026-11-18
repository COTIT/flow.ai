//! Tax profile rules: drafts change by version; verify needs the server review's fingerprint,
//! refuses overlaps under the `erp.tax|<ws>` lock, and freezes the revision for good. A
//! correction is a new revision that supersedes the old one on verify. Writes are permanent
//! accounting commands (replayed by `client_ref`). No tax math here: that is SQL `tax_line`.

use axum::http::StatusCode;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{json::Rules, list::Page, workspace::WorkspaceAccess},
    idempotency::{self, Answer, Command},
    modules::{
        accounting::{
            dto::{Action, Create},
            service::{created, ok, stale},
        },
        tax_profiles::{
            dto::{
                Draft, TaxProfileCreate, TaxProfilePatch, TaxProfileResource, TaxProfileReview,
                TaxProfileVerify,
            },
            repo::{self, ProfileRow},
        },
    },
};

/// 409 `tax_profile_verified`: a verified revision never changes.
fn verified() -> AppError {
    AppError::Reason {
        status: StatusCode::CONFLICT,
        code: "tax_profile_verified",
        message: "This tax profile is verified and can't change. Create a correction instead."
            .into(),
    }
}

/// 422 `tax_profile_overlap`.
fn overlap() -> AppError {
    AppError::Reason {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "tax_profile_overlap",
        message: "Another verified tax profile already covers this jurisdiction, transaction \
                  type, product category and date range."
            .into(),
    }
}

/// 409 `tax_profile_correction_exists`.
fn correction_exists() -> AppError {
    AppError::Reason {
        status: StatusCode::CONFLICT,
        code: "tax_profile_correction_exists",
        message: "This tax profile already has a correction.".into(),
    }
}

fn draft_of(r: &ProfileRow, res: &TaxProfileResource) -> Draft {
    Draft {
        country: r.country.clone(),
        subdivision: r.subdivision.clone(),
        effective_from: r.effective_from,
        effective_to: r.effective_to,
        transaction_types: r.transaction_types.clone(),
        product_categories: r.product_category_ids.clone(),
        components: res.components.clone(),
        exemptions: res.exemptions.clone(),
        tax_point: r.tax_point.clone(),
        evidence_ref: r.evidence_ref.clone(),
    }
}

/// Rows → resources, parts loaded in two queries for the whole page.
async fn resources(
    conn: &mut PgConnection,
    ws: Uuid,
    rows: Vec<ProfileRow>,
) -> Result<Vec<(ProfileRow, TaxProfileResource)>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let comps = repo::components(&mut *conn, ws, &ids).await?;
    let exempts = repo::exemptions(&mut *conn, ws, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let allowed_actions = match (r.state.as_str(), r.superseded_by_id) {
                ("draft", _) => vec!["update", "verify"],
                (_, None) => vec!["correct"],
                _ => vec![],
            };
            let res = TaxProfileResource {
                id: r.id,
                workspace_id: ws,
                lineage_id: r.lineage_id,
                revision: r.revision,
                corrects_id: r.corrects_id,
                superseded_by_id: r.superseded_by_id,
                state: r.state.clone(),
                version: r.version,
                country: r.country.clone(),
                jurisdiction_scope: r.subdivision.clone(),
                effective_from: r.effective_from,
                effective_to: r.effective_to,
                transaction_types: r.transaction_types.clone(),
                product_categories: r.product_category_ids.clone(),
                components: comps
                    .iter()
                    .filter(|(p, _)| *p == r.id)
                    .map(|(_, c)| c.clone())
                    .collect(),
                exemptions: exempts
                    .iter()
                    .filter(|(p, _)| *p == r.id)
                    .map(|(_, e)| e.clone())
                    .collect(),
                tax_point: r.tax_point.clone(),
                evidence_ref: r.evidence_ref.clone(),
                verified_by: r.verified_by,
                verified_at: r.verified_at,
                created_by: r.created_by,
                created_at: r.created_at,
                updated_at: r.updated_at,
                allowed_actions,
            };
            (r, res)
        })
        .collect())
}

async fn one(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
) -> Result<(ProfileRow, TaxProfileResource), AppError> {
    let rows = repo::profiles(&mut *conn, ws, Some(id), None, None, 1, 0).await?;
    resources(conn, ws, rows)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// GET /tax-profiles: one page plus the total.
pub async fn list(
    state: &AppState,
    ws: Uuid,
    country: Option<&str>,
    st: Option<&str>,
    page: Page,
) -> Result<(Vec<TaxProfileResource>, i64), AppError> {
    let mut conn = state.db.acquire().await?;
    let rows = repo::profiles(&mut *conn, ws, None, country, st, page.size, page.offset()).await?;
    let total = repo::count(&mut *conn, ws, country, st).await?;
    let data = resources(&mut conn, ws, rows).await?;
    Ok((data.into_iter().map(|(_, r)| r).collect(), total))
}

/// GET /tax-profiles/{id}.
pub async fn get(state: &AppState, ws: Uuid, id: Uuid) -> Result<TaxProfileResource, AppError> {
    let mut conn = state.db.acquire().await?;
    Ok(one(&mut conn, ws, id).await?.1)
}

/// Same-workspace checks on referenced categories and relations (`payload.` errors).
async fn references(conn: &mut PgConnection, ws: Uuid, d: &Draft) -> Result<(), AppError> {
    let mut r = Rules::default();
    d.check(&mut r);
    let cats: Vec<Uuid> = d
        .product_categories
        .iter()
        .chain(
            d.exemptions
                .iter()
                .filter_map(|e| e.product_category_id.as_ref()),
        )
        .copied()
        .collect();
    let rels: Vec<Uuid> = d.exemptions.iter().filter_map(|e| e.relation_id).collect();
    let known_cats = repo::known_categories(&mut *conn, ws, &cats).await?;
    let known_rels = repo::known_relations(&mut *conn, ws, &rels).await?;
    for (i, c) in d.product_categories.iter().enumerate() {
        r.check(
            &format!("payload.product_categories.{i}"),
            known_cats.contains(c),
            "The selected product category is invalid.",
        );
    }
    for (i, e) in d.exemptions.iter().enumerate() {
        if let Some(c) = e.product_category_id {
            r.check(
                &format!("payload.exemptions.{i}.product_category_id"),
                known_cats.contains(&c),
                "The selected product category is invalid.",
            );
        }
        if let Some(rel) = e.relation_id {
            r.check(
                &format!("payload.exemptions.{i}.relation_id"),
                known_rels.contains(&rel),
                "The selected relation is invalid.",
            );
        }
    }
    r.into_result()
}

/// POST /tax-profiles: a draft, or a correction (next revision) of a current verified one.
pub async fn create(
    state: &AppState,
    access: &WorkspaceAccess,
    cmd: Command<Create<TaxProfileCreate>>,
) -> Result<Answer, AppError> {
    const OP: &str = "tax_profiles_create";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    let input = &cmd.input.payload;
    references(&mut tx, ws, &input.draft).await?;
    let (lineage, revision) = match input.corrects {
        None => (None, 1),
        Some(prev) => {
            let rows = repo::profiles(&mut *tx, ws, Some(prev), None, None, 1, 0).await?;
            match rows.first() {
                Some(p) if p.state == "verified" && p.superseded_by_id.is_none() => {
                    (Some(p.lineage_id), p.revision + 1)
                }
                _ => {
                    return Err(AppError::field(
                        "payload.corrects_id",
                        "Only a current verified tax profile can be corrected.",
                    ));
                }
            }
        }
    };
    let id = repo::insert(
        &mut tx,
        ws,
        &input.draft,
        lineage,
        revision,
        input.corrects,
        access.user_id,
    )
    .await
    .map_err(|e| {
        let c = e.as_database_error().and_then(|d| d.constraint());
        if matches!(
            c,
            Some("tax_profiles_corrects_key" | "tax_profiles_revision_key")
        ) {
            correction_exists()
        } else {
            AppError::Database(e)
        }
    })?;
    repo::replace_parts(&mut tx, ws, id, &input.draft).await?;
    let answer = created(&one(&mut tx, ws, id).await?.1);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Lock a draft at the caller's version: 404 / 409 verified / 409 stale.
async fn lock_draft(
    conn: &mut PgConnection,
    ws: Uuid,
    id: Uuid,
    expected: i64,
) -> Result<Option<Uuid>, AppError> {
    let (st, version, corrects, _) = repo::lock(conn, ws, id).await?.ok_or(AppError::NotFound)?;
    if st == "verified" {
        return Err(verified());
    }
    if version != expected {
        return Err(stale());
    }
    Ok(corrects)
}

/// PATCH /tax-profiles/{id}: draft only.
pub async fn update(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<TaxProfilePatch>>,
) -> Result<Answer, AppError> {
    const OP: &str = "tax_profiles_update";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    lock_draft(&mut tx, ws, id, cmd.input.expected_version).await?;
    let (row, res) = one(&mut tx, ws, id).await?;
    let draft = cmd.input.payload.clone().apply(draft_of(&row, &res));
    references(&mut tx, ws, &draft).await?;
    repo::update(&mut tx, ws, id, &draft).await?;
    repo::replace_parts(&mut tx, ws, id, &draft).await?;
    let answer = ok(&one(&mut tx, ws, id).await?.1);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}

/// Content hash a verify must echo: id, version and every rule-bearing field.
fn fingerprint(r: &TaxProfileResource) -> [u8; 32] {
    let v = json!({
        "action": "verify", "id": r.id, "version": r.version, "country": r.country,
        "jurisdiction_scope": r.jurisdiction_scope, "effective_from": r.effective_from,
        "effective_to": r.effective_to, "transaction_types": r.transaction_types,
        "product_categories": r.product_categories, "components": r.components,
        "exemptions": r.exemptions, "tax_point": r.tax_point,
    });
    Sha256::digest(v.to_string().as_bytes()).into()
}

/// POST /tax-profiles/{id}/verify/review: no writes.
pub async fn review(state: &AppState, ws: Uuid, id: Uuid) -> Result<TaxProfileReview, AppError> {
    let mut conn = state.db.acquire().await?;
    let (_, profile) = one(&mut conn, ws, id).await?;
    if profile.state == "verified" {
        return Err(verified());
    }
    let review_fingerprint = hex::encode(fingerprint(&profile));
    Ok(TaxProfileReview {
        profile,
        review_fingerprint,
    })
}

/// POST /tax-profiles/{id}/verify.
pub async fn verify(
    state: &AppState,
    access: &WorkspaceAccess,
    id: Uuid,
    cmd: Command<Action<TaxProfileVerify>>,
) -> Result<Answer, AppError> {
    const OP: &str = "tax_profiles_verify";
    let ws = access.workspace_id;
    let mut tx = state.db.begin().await?;
    if let Some(a) = idempotency::command_replay(&mut tx, ws, OP, access.user_id, &cmd).await? {
        return Ok(a);
    }
    repo::verify_gate(&mut tx, ws).await?;
    let corrects = lock_draft(&mut tx, ws, id, cmd.input.expected_version).await?;
    let (_, res) = one(&mut tx, ws, id).await?;
    let fp = fingerprint(&res);
    if hex::encode(fp) != cmd.input.payload.review_fingerprint {
        return Err(stale());
    }
    if repo::overlaps(&mut tx, ws, id).await? {
        return Err(overlap());
    }
    let evidence = &cmd.input.payload.evidence_ref;
    repo::verify(&mut tx, ws, id, evidence, &fp, access.user_id, corrects).await?;
    let answer = ok(&one(&mut tx, ws, id).await?.1);
    idempotency::command_finish(&mut tx, ws, OP, &cmd, &answer, &[id]).await?;
    tx.commit().await?;
    Ok(answer)
}
