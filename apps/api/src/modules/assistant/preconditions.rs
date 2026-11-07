//! Request-local reviewed facts checked inside ordinary business transactions.

use super::preconditions_repo as snapshot_repo;
use super::{dto::ProposalInput, repo::Owner};
use crate::AppError;
use crate::{
    http::json::{Rules, Validate},
    modules::{orders, permissions, products, stock},
};
use serde_json::Value;
use sqlx::PgConnection;
use std::{
    future::Future,
    sync::atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

struct Execution {
    action: Uuid,
    workspace: Uuid,
    operation: String,
    review: Value,
    stale: AtomicBool,
}
tokio::task_local! { static EXECUTION: Execution; }

/// Install immutable facts only after the durable action claim succeeds.
pub async fn scope<T>(
    action_id: Uuid,
    workspace_id: Uuid,
    operation_id: String,
    review_facts: Value,
    future: impl Future<Output = T>,
) -> T {
    EXECUTION
        .scope(
            Execution {
                action: action_id,
                workspace: workspace_id,
                operation: operation_id,
                review: review_facts,
                stale: AtomicBool::new(false),
            },
            future,
        )
        .await
}

/// Correlate post-commit delivery work without retaining credentials or chat.
pub fn action_id() -> Option<Uuid> {
    EXECUTION.try_with(|scope| scope.action).ok()
}

/// Skip child resources of another kind and all ordinary native requests.
pub fn expects(kind: &str) -> bool {
    EXECUTION
        .try_with(|scope| scope.review["facts"]["kind"].as_str() == Some(kind))
        .unwrap_or(false)
}

/// Distinguish a checked pre-commit conflict from an uncertain post-commit outcome.
pub fn is_stale() -> bool {
    EXECUTION
        .try_with(|scope| scope.stale.load(Ordering::Relaxed))
        .unwrap_or(false)
}

fn check(
    workspace: Uuid,
    kind: &str,
    stage: &str,
    actual: &Value,
    required: bool,
) -> Result<(), AppError> {
    EXECUTION
        .try_with(|scope| {
            let facts = &scope.review["facts"];
            if facts["kind"].as_str() != Some(kind) {
                return Ok(());
            }
            let expected = &facts[stage];
            if expected.is_null() && !required {
                return Ok(());
            }
            if scope.workspace == workspace
                && expected.get("id").is_some_and(|id| {
                    !id.is_null()
                        && actual
                            .get("id")
                            .is_some_and(|other| !other.is_null() && id != other)
                })
            {
                return Ok(());
            }
            if (required && expected.is_null())
                || scope.workspace != workspace
                || scope.review["version"] != 1
                || scope.review["operation_id"].as_str() != Some(scope.operation.as_str())
                || expected != actual
            {
                scope.stale.store(true, Ordering::Relaxed);
                return Err(AppError::Conflict(
                    "Assistant review is stale. Review current facts before approving again.",
                ));
            }
            Ok(())
        })
        .unwrap_or(Ok(()))
}

/// Compare snapshots after the target's existing transaction locks are held.
pub fn check_locked(workspace_id: Uuid, kind: &str, actual: &Value) -> Result<(), AppError> {
    check(workspace_id, kind, "locked", actual, false)
}

/// Compare the exact resolved defaults and priced values the transaction will commit.
pub fn check_priced(workspace_id: Uuid, kind: &str, actual: &Value) -> Result<(), AppError> {
    check(workspace_id, kind, "priced", actual, false)
}

/// Require proposed values for legacy-sensitive mutations while native callers remain unchanged.
pub fn check_priced_required(
    workspace_id: Uuid,
    kind: &str,
    actual: &Value,
) -> Result<(), AppError> {
    check(workspace_id, kind, "priced", actual, true)
}

/// Capture safe domain facts before presenting an immutable approval draft.
pub async fn capture(
    conn: &mut PgConnection,
    owner: Owner,
    input: &ProposalInput,
) -> Result<Value, AppError> {
    use serde_json::json;
    let permission = match input.operation_id.as_str() {
        "orders_create" => "order.create",
        "orders_delete" | "orders_refund" => "order.delete",
        "stock_set_levels" => "stock.set_stock_levels",
        operation if requires_facts(operation) => "order.update",
        _ => return Ok(json!({})),
    };
    let access = permissions::repo::access(&mut *conn, owner.workspace_id, owner.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !access.active
        || !permissions::model::Granted::new(access.owner, access.permissions).allows(permission)
    {
        return Err(AppError::Forbidden(
            "You do not have permission for this action.",
        ));
    }
    let workspace = owner.workspace_id;
    let body = input.body.clone().unwrap_or(json!({}));
    let facts = match input.operation_id.as_str() {
        "orders_create" => {
            let order = validated::<orders::dto::CreateOrderRequest>(body)?;
            let (currency, decimals) =
                orders::service::currency(conn, workspace, order.currency_id).await?;
            let warehouse = orders::service::warehouse(conn, workspace, order.warehouse_id).await?;
            let priced =
                orders::service::price(conn, workspace, currency, decimals, &order.lines, false)
                    .await?;
            json!({"kind":"order","priced":orders::service::review_priced(conn,workspace,currency,decimals,warehouse,&priced).await?})
        }
        "stock_set_levels" => {
            let entries = validated::<stock::dto::SetLevelsBody>(body)?;
            json!({"kind":"stock","locked":stock_facts(conn,workspace,target(input)?,&entries).await?})
        }
        _ => {
            let id = target(input)?;
            let order = orders::repo::find(&mut *conn, workspace, id)
                .await?
                .ok_or(AppError::NotFound)?;
            let lines = orders::repo::lines(&mut *conn, workspace, &[id]).await?;
            let mut facts = json!({"kind":"order","locked":orders::service::review_locked(conn,workspace,&order,&lines).await?});
            if input.operation_id == "orders_invoice" {
                facts["priced"] =
                    orders::service::review_invoice(conn, workspace, &order, &lines).await?;
            }
            if input.operation_id == "orders_update" {
                let patch = validated::<orders::dto::UpdateOrderRequest>(body)?;
                if let Some(lines) = patch.lines {
                    let decimals = u32::try_from(order.decimals)
                        .map_err(|_| AppError::Internal("negative currency decimals".into()))?;
                    let warehouse = orders::service::warehouse(
                        conn,
                        workspace,
                        Some(patch.warehouse_id.unwrap_or(order.warehouse_id)),
                    )
                    .await?;
                    let priced = orders::service::price(
                        conn,
                        workspace,
                        order.currency_id,
                        decimals,
                        &lines,
                        false,
                    )
                    .await?;
                    facts["priced"] = orders::service::review_priced(
                        conn,
                        workspace,
                        order.currency_id,
                        decimals,
                        warehouse,
                        &priced,
                    )
                    .await?;
                }
            }
            facts
        }
    };
    let review = json!({"version":1,"operation_id":input.operation_id,"facts":facts});
    if review.to_string().len() > 32768 {
        return Err(AppError::field(
            "body",
            "The material review exceeds 32768 bytes. Narrow this action.",
        ));
    }
    Ok(review)
}

fn target(input: &ProposalInput) -> Result<Uuid, AppError> {
    input
        .path
        .split('/')
        .nth(5)
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or(AppError::NotFound)
}

/// Reuse native DTO validation while capturing canonical business facts.
pub(crate) fn validated<T: serde::de::DeserializeOwned + Validate>(
    body: Value,
) -> Result<T::Valid, AppError> {
    let request: T = serde_json::from_value(body)
        .map_err(|_| AppError::field("body", "Invalid business draft."))?;
    let mut rules = Rules::default();
    let input = request.validate(&mut rules);
    rules.into_result()?;
    input.ok_or_else(|| AppError::field("body", "Invalid business draft."))
}

/// Bounded requested slots only; missing rows have the same zero values as first writes.
pub async fn stock_facts(
    conn: &mut PgConnection,
    workspace: Uuid,
    product_id: Uuid,
    entries: &[stock::dto::Entry],
) -> Result<Value, AppError> {
    let product = products::repo::find(&mut *conn, workspace, product_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let warehouses: Vec<_> = entries.iter().map(|entry| entry.warehouse_id).collect();
    let bins: Vec<_> = entries.iter().map(|entry| entry.bin_id).collect();
    let mut slots =
        snapshot_repo::stock_slots(conn, workspace, product_id, &warehouses, &bins).await?;
    for (index, slot) in slots.as_array_mut().into_iter().flatten().enumerate() {
        if slot["valid"] != true {
            return Err(AppError::field(
                &format!("warehouses.{index}.warehouse_id"),
                "The warehouse or bin is no longer available.",
            ));
        }
        if let Some(slot) = slot.as_object_mut() {
            slot.remove("valid");
        }
    }
    Ok(
        serde_json::json!({"id":product.id,"product_name":product.name,"tracks_stock":stock::service::tracks_stock(&product),"slots":slots}),
    )
}

/// Operations whose defaults, prices or absolute before-values require a fresh review.
pub fn requires_facts(operation_id: &str) -> bool {
    matches!(
        operation_id,
        "orders_create"
            | "orders_update"
            | "orders_delete"
            | "orders_confirm"
            | "orders_pick"
            | "orders_complete"
            | "orders_cancel"
            | "orders_returns"
            | "orders_invoice"
            | "orders_refund"
            | "stock_set_levels"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn native_requests_skip_checks_and_scoped_checks_reject_changed_money() {
        let workspace = Uuid::now_v7();
        let action = Uuid::now_v7();
        assert!(check_priced(workspace, "order", &json!({"total_minor":1})).is_ok());
        let review = json!({"version":1,"operation_id":"orders_create","facts":{"kind":"order","priced":{"total_minor":100}}});
        scope(action, workspace, "orders_create".into(), review, async {
            assert_eq!(action_id(), Some(action));
            assert!(!is_stale());
            assert!(check_priced(workspace, "order", &json!({"total_minor":101})).is_err());
            assert!(is_stale());
        })
        .await;
        assert_eq!(action_id(), None);
    }

    #[tokio::test]
    async fn child_resources_skip_parent_snapshot_but_wrong_workspace_is_rejected() {
        let workspace = Uuid::now_v7();
        let id = Uuid::now_v7();
        let actual = json!({"id":id,"status":"draft"});
        let review = json!({"version":1,"operation_id":"orders_cancel","facts":{"kind":"order","locked":actual}});
        scope(
            Uuid::now_v7(),
            workspace,
            "orders_cancel".into(),
            review,
            async {
                assert!(check_locked(workspace, "invoice", &json!({})).is_ok());
                assert!(
                    check_locked(
                        workspace,
                        "order",
                        &json!({"id":Uuid::now_v7(),"status":"open"})
                    )
                    .is_ok()
                );
                assert!(check_locked(Uuid::now_v7(), "order", &actual).is_err());
            },
        )
        .await;
    }

    #[tokio::test]
    async fn parallel_task_contexts_keep_action_identity_and_facts_separate() {
        async fn checked(action: Uuid, workspace: Uuid, total: i64) {
            let actual = json!({"total_minor":total});
            let review = json!({"version":1,"operation_id":"orders_create","facts":{"kind":"order","priced":actual}});
            scope(action, workspace, "orders_create".into(), review, async {
                tokio::task::yield_now().await;
                assert_eq!(action_id(), Some(action));
                assert!(check_priced(workspace, "order", &actual).is_ok());
            })
            .await;
        }
        tokio::join!(
            checked(Uuid::now_v7(), Uuid::now_v7(), 100),
            checked(Uuid::now_v7(), Uuid::now_v7(), 200)
        );
    }
}

#[cfg(test)]
mod required_priced_tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    async fn foreign_workspace_is_not_skipped_as_a_different_child_resource() {
        let workspace = Uuid::now_v7();
        scope(Uuid::now_v7(),workspace,"invoices_update".into(),json!({"version":1,"operation_id":"invoices_update","facts":{"kind":"invoice","priced":{"id":"expected"}}}),async {
            assert!(check_priced_required(Uuid::now_v7(),"invoice",&json!({"id":"different"})).is_err());
            assert!(is_stale());
        }).await;
    }

    #[tokio::test]
    async fn legacy_missing_proposed_stage_is_stale_but_native_calls_are_unchanged() {
        let workspace = Uuid::now_v7();
        assert!(check_priced_required(workspace, "invoice", &json!({"total_minor":1})).is_ok());
        scope(Uuid::now_v7(),workspace,"invoices_update".into(),json!({"version":1,"operation_id":"invoices_update","facts":{"kind":"invoice","locked":{"id":"existing"}}}),async {
            assert!(check_priced_required(workspace,"invoice",&json!({"total_minor":1})).is_err());
            assert!(is_stale());
        }).await;
    }
}
