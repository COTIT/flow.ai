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

/// Relation joined with its relation type (`rt_*`) and source type (`st_*`); `total` is the
/// list's window count (None for single-row reads).
struct Joined {
    id: Uuid,
    relation_type_id: Uuid,
    source_type_id: Option<Uuid>,
    relation_number: Option<i64>,
    relation_appendix: Option<Value>,
    kind: String,
    allow_invoicing_on_account: bool,
    disable_order_workflow_emails: bool,
    alias: Option<String>,
    company_name: Option<String>,
    email: Option<String>,
    website: Option<String>,
    phone_number: Option<String>,
    coc_number: Option<String>,
    vat_number: Option<String>,
    import_source: Option<String>,
    import_id: Option<String>,
    default_language: Option<String>,
    addresses: Value,
    financial_account: Option<Value>,
    terms_and_periods: Option<Value>,
    supplier_data: Option<Value>,
    meta: Option<Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    rt_id: Option<Uuid>,
    rt_name: Option<String>,
    rt_domain: Option<String>,
    rt_properties: Option<Value>,
    rt_meta: Option<Value>,
    rt_created_at: Option<DateTime<Utc>>,
    rt_updated_at: Option<DateTime<Utc>>,
    st_id: Option<Uuid>,
    st_name: Option<String>,
    st_domain: Option<String>,
    st_properties: Option<Value>,
    st_meta: Option<Value>,
    st_created_at: Option<DateTime<Utc>>,
    st_updated_at: Option<DateTime<Utc>>,
    total: Option<i64>,
}

/// A relation plus its live relation type and source type (None when deleted).
pub type WithTypes = (RelationRow, Option<CustomTypeRow>, Option<CustomTypeRow>);

#[allow(clippy::too_many_arguments)]
fn custom_type(
    id: Option<Uuid>,
    name: Option<String>,
    domain: Option<String>,
    properties: Option<Value>,
    meta: Option<Value>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
) -> Option<CustomTypeRow> {
    Some(CustomTypeRow {
        id: id?,
        name: name?,
        domain: domain?,
        properties,
        meta,
        created_at: created_at?,
        updated_at: updated_at?,
    })
}

impl Joined {
    fn split(self) -> Result<WithTypes, sqlx::Error> {
        let relation_type = custom_type(
            self.rt_id,
            self.rt_name,
            self.rt_domain,
            self.rt_properties,
            self.rt_meta,
            self.rt_created_at,
            self.rt_updated_at,
        );
        let source_type = custom_type(
            self.st_id,
            self.st_name,
            self.st_domain,
            self.st_properties,
            self.st_meta,
            self.st_created_at,
            self.st_updated_at,
        );
        let relation = RelationRow {
            id: self.id,
            relation_type_id: self.relation_type_id,
            source_type_id: self.source_type_id,
            relation_number: self.relation_number,
            relation_appendix: self.relation_appendix,
            kind: self.kind,
            allow_invoicing_on_account: self.allow_invoicing_on_account,
            disable_order_workflow_emails: self.disable_order_workflow_emails,
            alias: self.alias,
            company_name: self.company_name,
            email: self.email,
            website: self.website,
            phone_number: self.phone_number,
            coc_number: self.coc_number,
            vat_number: self.vat_number,
            import_source: self.import_source,
            import_id: self.import_id,
            default_language: self.default_language,
            addresses: decode_addresses(self.addresses)?,
            financial_account: self.financial_account,
            terms_and_periods: self.terms_and_periods,
            supplier_data: self.supplier_data,
            meta: self.meta,
            created_at: self.created_at,
            updated_at: self.updated_at,
        };
        Ok((relation, relation_type, source_type))
    }
}

/// Live relation of the workspace with its types.
pub async fn find_detail(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<WithTypes>, sqlx::Error> {
    sqlx::query_as!(
        Joined,
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
                  st.created_at AS "st_created_at?", st.updated_at AS "st_updated_at?",
                  NULL::bigint AS total
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
    .map(Joined::split)
    .transpose()
}

/// List filters; `q` from `search::parse`. Patterns are escaped ILIKE values.
#[derive(Clone, Copy)]
pub struct ListFilter<'a> {
    pub suppliers: bool,
    /// Lowercased exact contact last name.
    pub last_name: Option<&'a str>,
    /// Compact uppercase zipcode (`1339NM`), compared the same way against stored addresses.
    pub zipcode: Option<&'a str>,
    /// Lowercased exact company name.
    pub company_name: Option<&'a str>,
    pub q: &'a crate::modules::relations::search::Query,
    pub limit: i64,
    pub offset: i64,
}

/// One page, newest first, plus the matching total (0 when the page is empty). One query:
/// Laravel's SQL search (relation fields, type name, contact names/email, phone spellings,
/// id-like exact matches) with trigram indexes behind the ILIKEs.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<WithTypes>, i64), sqlx::Error> {
    let words = serde_json::to_value(&f.q.words).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
    let rows = sqlx::query_as!(
        Joined,
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
                  st.created_at AS "st_created_at?", st.updated_at AS "st_updated_at?",
                  count(*) OVER () AS total
           FROM relations r
           LEFT JOIN custom_types rt ON rt.workspace_id = r.workspace_id
                AND rt.id = r.relation_type_id AND rt.deleted_at IS NULL
           LEFT JOIN custom_types st ON st.workspace_id = r.workspace_id
                AND st.id = r.source_type_id AND st.deleted_at IS NULL
           WHERE r.workspace_id = $1 AND r.deleted_at IS NULL
             AND (NOT $2 OR (r.company_name IS NOT NULL
                             AND rt.properties @> '{"is_supplier": true}'))
             AND ($3::text IS NULL OR EXISTS (
                   SELECT 1 FROM contacts c
                   WHERE c.workspace_id = r.workspace_id AND c.relation_id = r.id
                     AND c.deleted_at IS NULL AND lower(c.last_name) = $3))
             AND ($4::text IS NULL OR EXISTS (
                   SELECT 1 FROM jsonb_array_elements(r.addresses) a
                   WHERE upper(regexp_replace(a ->> 'zipcode', '[^0-9A-Za-z]', '', 'g')) = $4))
             AND ($5::text IS NULL OR lower(r.company_name) = $5)
             AND (cardinality($6::text[]) = 0
                  OR r.alias ILIKE ANY($6) OR r.company_name ILIKE ANY($6)
                  OR r.email ILIKE ANY($6) OR r.phone_number ILIKE ANY($6)
                  OR r.website ILIKE ANY($6) OR rt.name ILIKE ANY($6)
                  OR r.id = $8 OR r.import_id = $9 OR lower(r.email) = lower($9)
                  OR r.phone_number = $9 OR r.relation_number = $10
                  OR r.phone_number = ANY($11)
                  OR EXISTS (
                     SELECT 1 FROM contacts c
                     WHERE c.workspace_id = r.workspace_id AND c.relation_id = r.id
                       AND c.deleted_at IS NULL
                       AND (c.email ILIKE ANY($6)
                            OR c.phone_number = ANY($11)
                            OR c.phones::text ILIKE ANY($12)
                            -- every word of one phrase hits first, middle or last name
                            OR EXISTS (
                               SELECT 1 FROM jsonb_array_elements($7::jsonb) phrase
                               WHERE NOT EXISTS (
                                  SELECT 1 FROM jsonb_array_elements_text(phrase) w
                                  WHERE NOT (coalesce(c.first_name, '') ILIKE w
                                             OR coalesce(c.middle_name, '') ILIKE w
                                             OR coalesce(c.last_name, '') ILIKE w))))))
           ORDER BY r.id DESC
           LIMIT $13 OFFSET $14"#,
        workspace_id,
        f.suppliers,
        f.last_name,
        f.zipcode,
        f.company_name,
        &f.q.patterns,
        words,
        f.q.uuid,
        f.q.text,
        f.q.number,
        &f.q.phones,
        &f.q.phone_patterns,
        f.limit,
        f.offset,
    )
    .fetch_all(db)
    .await?;
    let total = rows.first().and_then(|r| r.total).unwrap_or(0);
    let rows = rows
        .into_iter()
        .map(Joined::split)
        .collect::<Result<_, _>>()?;
    Ok((rows, total))
}
