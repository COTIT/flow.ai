//! SQL for relations. Every statement takes `workspace_id` and skips soft-deleted rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::modules::{
    custom_types::repo::CustomTypeRow,
    relations::{address::Address, dto::NewRelation},
};

/// Laravel RelationSetting::start_number default; used while a workspace has no numbered row.
// shortcut: settings module not ported, so start number / prefix / digits are Laravel defaults.
pub const START_NUMBER: i64 = 1;

/// Live relation. `addresses` decoded from the JSON array column.
pub struct RelationRow {
    pub id: Uuid,
    pub relation_type_id: Uuid,
    pub source_type_id: Option<Uuid>,
    pub relation_number: Option<i64>,
    pub relation_appendix: Option<Value>,
    pub kind: String,
    pub allow_invoicing_on_account: bool,
    pub disable_order_workflow_emails: bool,
    pub alias: Option<String>,
    pub company_name: Option<String>,
    pub email: Option<String>,
    pub website: Option<String>,
    pub phone_number: Option<String>,
    pub coc_number: Option<String>,
    pub vat_number: Option<String>,
    pub import_source: Option<String>,
    pub import_id: Option<String>,
    pub default_language: Option<String>,
    pub addresses: Vec<Address>,
    pub financial_account: Option<Value>,
    pub terms_and_periods: Option<Value>,
    pub supplier_data: Option<Value>,
    pub meta: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// JSON column -> addresses; a malformed stored value is a decode error, never silently empty.
pub fn decode_addresses(v: Value) -> Result<Vec<Address>, sqlx::Error> {
    serde_json::from_value(v).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

/// Next relation number for the workspace, serialized per workspace by a transaction-scoped
/// advisory lock (Laravel had none, so parallel creates got the same number). MAX counts
/// soft-deleted rows too: numbers are never reused.
pub async fn next_number(conn: &mut PgConnection, workspace_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtext('relations.relation_number'), hashtext($1::text))",
        workspace_id.to_string(),
    )
    .execute(&mut *conn)
    .await?;
    let max = sqlx::query_scalar!(
        "SELECT max(relation_number) FROM relations WHERE workspace_id = $1",
        workspace_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(max.map_or(START_NUMBER, |n| n + 1))
}

/// New relation row; returns its id. `relation_type_id` already resolved.
pub async fn insert(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    created_by: Uuid,
    relation_type_id: Uuid,
    number: i64,
    appendix: &Value,
    r: &NewRelation,
) -> Result<Uuid, sqlx::Error> {
    let addresses =
        serde_json::to_value(&r.addresses).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
    sqlx::query_scalar!(
        "INSERT INTO relations (workspace_id, relation_type_id, source_type_id, relation_number,
                                relation_appendix, type, allow_invoicing_on_account,
                                disable_order_workflow_emails, alias, company_name, email, website,
                                phone_number, coc_number, vat_number, default_language, addresses,
                                financial_account, meta, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
                 $19, $20)
         RETURNING id",
        workspace_id,
        relation_type_id,
        r.source_type_id,
        number,
        appendix,
        r.kind,
        r.allow_invoicing_on_account,
        r.disable_order_workflow_emails,
        r.alias,
        r.company_name,
        r.email,
        r.website,
        r.phone_number,
        r.coc_number,
        r.vat_number,
        r.default_language,
        addresses,
        r.financial_account,
        r.meta,
        created_by,
    )
    .fetch_one(db)
    .await
}

/// Live relation with its relation type and source type (None when that type was deleted).
pub async fn find_detail(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<(RelationRow, Option<CustomTypeRow>, Option<CustomTypeRow>)>, sqlx::Error> {
    let Some(row) = sqlx::query!(
        r#"SELECT r.id, r.relation_type_id, r.source_type_id, r.relation_number, r.relation_appendix,
                  r.type AS kind, r.allow_invoicing_on_account, r.disable_order_workflow_emails,
                  r.alias, r.company_name, r.email, r.website, r.phone_number, r.coc_number,
                  r.vat_number, r.import_source, r.import_id, r.default_language, r.addresses,
                  r.financial_account, r.terms_and_periods, r.supplier_data, r.meta, r.created_at,
                  r.updated_at,
                  rt.id AS "rt_id?", rt.name AS "rt_name?", rt.domain AS "rt_domain?",
                  rt.properties AS rt_properties, rt.meta AS rt_meta,
                  rt.created_at AS "rt_created_at?", rt.updated_at AS "rt_updated_at?",
                  st.id AS "st_id?", st.name AS "st_name?", st.domain AS "st_domain?",
                  st.properties AS st_properties, st.meta AS st_meta,
                  st.created_at AS "st_created_at?", st.updated_at AS "st_updated_at?"
           FROM relations r
           LEFT JOIN custom_types rt ON rt.workspace_id = r.workspace_id
                AND rt.id = r.relation_type_id AND rt.deleted_at IS NULL
           LEFT JOIN custom_types st ON st.workspace_id = r.workspace_id
                AND st.id = r.source_type_id AND st.deleted_at IS NULL
           WHERE r.id = $1 AND r.workspace_id = $2 AND r.deleted_at IS NULL"#,
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let ty = |id: Option<Uuid>,
              name: Option<String>,
              domain: Option<String>,
              properties: Option<Value>,
              meta: Option<Value>,
              created_at: Option<DateTime<Utc>>,
              updated_at: Option<DateTime<Utc>>| {
        Some(CustomTypeRow {
            id: id?,
            name: name?,
            domain: domain?,
            properties,
            meta,
            created_at: created_at?,
            updated_at: updated_at?,
        })
    };
    let relation_type = ty(
        row.rt_id,
        row.rt_name,
        row.rt_domain,
        row.rt_properties,
        row.rt_meta,
        row.rt_created_at,
        row.rt_updated_at,
    );
    let source_type = ty(
        row.st_id,
        row.st_name,
        row.st_domain,
        row.st_properties,
        row.st_meta,
        row.st_created_at,
        row.st_updated_at,
    );
    let relation = RelationRow {
        id: row.id,
        relation_type_id: row.relation_type_id,
        source_type_id: row.source_type_id,
        relation_number: row.relation_number,
        relation_appendix: row.relation_appendix,
        kind: row.kind,
        allow_invoicing_on_account: row.allow_invoicing_on_account,
        disable_order_workflow_emails: row.disable_order_workflow_emails,
        alias: row.alias,
        company_name: row.company_name,
        email: row.email,
        website: row.website,
        phone_number: row.phone_number,
        coc_number: row.coc_number,
        vat_number: row.vat_number,
        import_source: row.import_source,
        import_id: row.import_id,
        default_language: row.default_language,
        addresses: decode_addresses(row.addresses)?,
        financial_account: row.financial_account,
        terms_and_periods: row.terms_and_periods,
        supplier_data: row.supplier_data,
        meta: row.meta,
        created_at: row.created_at,
        updated_at: row.updated_at,
    };
    Ok(Some((relation, relation_type, source_type)))
}
