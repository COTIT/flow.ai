//! Invoice facts captured under native write authority and checked under existing row locks.

use super::{repo, service};
use crate::{
    AppError,
    modules::{
        assistant::{dto::ProposalInput, preconditions, repo::Owner},
        permissions,
    },
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

/// Critical invoice mutations whose pending reviews must carry material facts.
pub fn requires_facts(operation: &str) -> bool {
    matches!(
        operation,
        "invoices_create"
            | "invoices_update"
            | "invoices_delete"
            | "invoices_duplicate"
            | "invoices_send"
            | "invoices_resend"
            | "invoices_credit"
            | "invoice_payments_create"
            | "invoice_payments_void"
            | "invoice_payments_mark_as_credited"
    )
}

/// Permission check precedes every invoice or contact read.
pub async fn capture(
    conn: &mut PgConnection,
    owner: Owner,
    input: &ProposalInput,
) -> Result<Option<Value>, AppError> {
    let operation = input.operation_id.as_str();
    if !requires_facts(operation) {
        return Ok(None);
    }
    let permission = match operation {
        "invoices_create" => "invoice.create",
        "invoices_update" => "invoice.update",
        "invoices_delete" => "invoice.delete",
        "invoices_duplicate" => "invoice.create",
        "invoices_send" | "invoices_resend" => "invoice.send",
        "invoices_credit" | "invoice_payments_mark_as_credited" => "invoice.credit",
        "invoice_payments_create" => "invoicepayment.create",
        _ => "invoicepayment.delete",
    };
    let access = permissions::repo::access(&mut *conn, owner.workspace_id, owner.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let grants = permissions::model::Granted::new(access.owner, access.permissions);
    if !access.active
        || !grants.allows(permission)
        || (matches!(operation, "invoices_send" | "invoices_resend")
            && !grants.allows("workspace.send-message"))
        || (operation == "invoices_credit" && !grants.allows("invoice.create"))
    {
        return Err(AppError::Forbidden(
            "You do not have permission for this action.",
        ));
    }
    if operation == "invoices_create" {
        let request = preconditions::validated::<super::dto::CreateInvoiceRequest>(
            input.body.clone().unwrap_or_else(|| json!({})),
        )?;
        let facts = service::review_create(conn, owner.workspace_id, &request).await?;
        let review =
            json!({"version":1,"operation_id":operation,"facts":{"kind":"invoice","priced":facts}});
        if review.to_string().len() > 32768 {
            return Err(AppError::field(
                "body",
                "The material review exceeds 32768 bytes. Narrow this action.",
            ));
        }
        return Ok(Some(review));
    }
    let id = input
        .path
        .split('/')
        .nth(5)
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(AppError::NotFound)?;
    let priced = if operation == "invoices_update" {
        let request = preconditions::validated::<super::dto::UpdateInvoiceRequest>(
            input.body.clone().unwrap_or_else(|| json!({})),
        )?;
        Some(service::review_update(conn, owner.workspace_id, id, &request).await?)
    } else {
        None
    };
    let contact = if matches!(operation, "invoices_send" | "invoices_resend") {
        input
            .body
            .as_ref()
            .and_then(|v| v["addressed_contact_id"].as_str())
            .and_then(|v| Uuid::parse_str(v).ok())
    } else {
        None
    };
    let facts = current(conn, owner.workspace_id, id, contact).await?;
    let mut review =
        json!({"version":1,"operation_id":operation,"facts":{"kind":"invoice","locked":facts}});
    if let Some(priced) = priced {
        review["facts"]["priced"] = priced;
    }
    if review.to_string().len() > 32768 {
        return Err(AppError::field(
            "body",
            "The material review exceeds 32768 bytes. Narrow this action.",
        ));
    }
    Ok(Some(review))
}

async fn current(
    conn: &mut PgConnection,
    workspace: Uuid,
    id: Uuid,
    contact: Option<Uuid>,
) -> Result<Value, AppError> {
    let mut facts = repo::approval_snapshot(&mut *conn, workspace, id, contact)
        .await?
        .ok_or(AppError::NotFound)?;
    let decimals = facts["currency"]["decimals"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| AppError::Internal("invoice currency decimals".into()))?;
    let lines = repo::lines(&mut *conn, workspace, id).await?;
    let (summary, _) = service::price(&lines, decimals)?;
    facts["total_minor"] = json!(crate::money::minor(summary.total, decimals)?);
    facts["subtotal_minor"] = json!(crate::money::minor(summary.excl, decimals)?);
    Ok(bounded_review(facts))
}

/// Bind every material line while exposing an explicit bounded preview.
pub(super) fn bounded_review(facts: Value) -> Value {
    // Whole resource remains bound even when preview shows only five lines.
    let digest = hex::encode(Sha256::digest(facts.to_string().as_bytes()));
    let mut summary = serde_json::Map::new();
    for key in [
        "id",
        "invoice_id",
        "set_billing_address",
        "set_addressed_contact",
        "new_line_count",
        "changed_line_count",
        "deleted_line_count",
        "status",
        "reference",
        "note",
        "updated_at",
        "stored_total_minor",
        "paid_minor",
        "number",
        "due_date",
        "is_credit",
        "payment_method",
        "relation_id",
        "currency",
        "selected_contact_id",
        "selected_contact",
        "total_minor",
        "subtotal_minor",
        "billing_address",
    ] {
        if let Some(value) = facts.get(key) {
            summary.insert(key.to_owned(), value.clone());
        }
    }
    if let Some(contact) = facts.get("addressed_contact").and_then(Value::as_object) {
        let visible = contact
            .iter()
            .filter(|(key, _)| {
                matches!(
                    key.as_str(),
                    "uuid" | "first_name" | "middle_name" | "last_name" | "email"
                )
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<serde_json::Map<_, _>>();
        summary.insert("addressed_contact".into(), Value::Object(visible));
    } else if facts.get("addressed_contact").is_some_and(Value::is_null) {
        summary.insert("addressed_contact".into(), Value::Null);
    }
    let lines = facts["lines"].as_array();
    let count = lines.map_or(0, Vec::len);
    summary.insert("line_count".into(), json!(count));
    summary.insert(
        "lines_preview".into(),
        json!(lines.into_iter().flatten().take(5).collect::<Vec<_>>()),
    );
    summary.insert("lines_omitted".into(), json!(count.saturating_sub(5)));
    summary.insert("material_sha256".into(), json!(digest));
    Value::Object(summary)
}

/// Native callers skip snapshot work; assistant comparison shares invoice transaction.
pub async fn check(
    conn: &mut PgConnection,
    workspace: Uuid,
    id: Uuid,
    contact: Option<Uuid>,
) -> Result<(), AppError> {
    if !preconditions::expects("invoice") {
        return Ok(());
    }
    repo::approval_share(conn, workspace, id, contact).await?;
    let actual = current(conn, workspace, id, contact).await?;
    preconditions::check_locked(workspace, "invoice", &actual)
}

#[cfg(test)]
mod bounded_facts_tests {
    use super::*;
    fn thousand_lines() -> Value {
        json!({"id":"invoice","currency":{"code":"EUR","decimals":2},"total_minor":1210000,"subtotal_minor":1000000,"paid_minor":0,"lines":(0..1000).map(|i|json!({"id":i,"title":"x".repeat(255),"quantity":"1","price":"10","vat":"21"})).collect::<Vec<_>>()})
    }
    #[test]
    fn all_native_invoice_lines_fit_bounded_explicit_summary() {
        let result = bounded_review(thousand_lines());
        assert!(
            result.to_string().len() < 32768,
            "1000-line invoice review must fit32KiB"
        );
        assert_eq!(result["line_count"], 1000);
        assert_eq!(result["lines_preview"].as_array().unwrap().len(), 5);
        assert_eq!(result["lines_omitted"], 995);
        assert_eq!(result["total_minor"], 1210000);
    }
    #[test]
    fn drift_beyond_visible_preview_changes_private_digest() {
        let full = thousand_lines();
        let first = bounded_review(full.clone());
        let mut changed = full;
        changed["lines"][999]["quantity"] = json!("2");
        let second = bounded_review(changed);
        assert_eq!(first["lines_preview"], second["lines_preview"]);
        assert_ne!(first["material_sha256"], second["material_sha256"]);
    }
}
