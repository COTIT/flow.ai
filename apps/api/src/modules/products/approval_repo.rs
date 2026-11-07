//! Workspace-scoped currency and VAT rows held steady while native money values are checked.

use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Live currency meaning, share locked until the caller's transaction completes.
pub(crate) async fn currency(
    conn: &mut PgConnection,
    workspace: Uuid,
    id: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',id,'code',upper(asset_type),'decimals',decimals,'default',default_currency,'active',active,'symbol_prefix',symbol_prefix,'symbol_suffix',symbol_suffix) AS "value!" FROM currencies WHERE workspace_id=$1 AND id=$2 AND deleted_at IS NULL FOR SHARE"#,workspace,id).fetch_optional(conn).await
}

/// Live VAT meaning, share locked against edits and soft deletion.
pub(super) async fn vat(
    conn: &mut PgConnection,
    workspace: Uuid,
    id: Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT jsonb_build_object('id',id,'name',name,'rate',trim_scale(vat_amount)::text,'default',default_group,'country',country) AS "value!" FROM vat_groups WHERE workspace_id=$1 AND id=$2 AND deleted_at IS NULL FOR SHARE"#,workspace,id).fetch_optional(conn).await
}
