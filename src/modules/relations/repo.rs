//! SQL for relations. Every statement takes `workspace_id`; all but numbering skip
//! soft-deleted rows (numbers are never reused).

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
    // A waiter holds a pooled connection; give up instead of starving the pool.
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await?;
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
    max.map_or(Ok(START_NUMBER), |n| {
        n.checked_add(1)
            .ok_or_else(|| sqlx::Error::Protocol("relation number overflow".into()))
    })
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
        r#"SELECT r.id, r.relation_number, r.relation_appendix,
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
/// matching ids first (workspace-scoped scan: relation fields, type name, contact names and
/// email, phone spellings, id-like exact matches), counted over ids only, then the page rows
/// joined. Laravel's SQL search; no index serves the OR, see docs/api/relations.md for size.
pub async fn list(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    f: &ListFilter<'_>,
) -> Result<(Vec<WithTypes>, i64), sqlx::Error> {
    let words = serde_json::to_value(&f.q.words).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
    let rows = sqlx::query_as!(
        Joined,
        r#"WITH matched AS (
               SELECT r.id
               FROM relations r
               LEFT JOIN custom_types rt ON rt.workspace_id = r.workspace_id
                    AND rt.id = r.relation_type_id AND rt.deleted_at IS NULL
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
                                -- every word of one phrase is in the full name (words have
                                -- no spaces, so none can straddle two name parts)
                                OR EXISTS (
                                   SELECT 1 FROM jsonb_array_elements($7::jsonb) phrase
                                   WHERE concat_ws(' ', c.first_name, c.middle_name, c.last_name)
                                         ILIKE ALL (ARRAY(SELECT jsonb_array_elements_text(phrase)))))))
           ),
           page AS (
               SELECT id, count(*) OVER () AS total FROM matched
               ORDER BY id DESC LIMIT $13 OFFSET $14
           )
           SELECT r.id, r.relation_number, r.relation_appendix,
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
                  page.total
           FROM page
           JOIN relations r ON r.id = page.id
           LEFT JOIN custom_types rt ON rt.workspace_id = r.workspace_id
                AND rt.id = r.relation_type_id AND rt.deleted_at IS NULL
           LEFT JOIN custom_types st ON st.workspace_id = r.workspace_id
                AND st.id = r.source_type_id AND st.deleted_at IS NULL
           ORDER BY r.id DESC"#,
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

/// Stored addresses of a live relation, row locked for this transaction (updates and contact
/// writes read the country, and default-contact changes serialize on it). NO KEY: child
/// inserts (contacts now, orders later) still take their FK share lock. None = 404.
pub async fn addresses_for_update(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<Vec<Address>>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT addresses FROM relations
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL
         FOR NO KEY UPDATE",
        id,
        workspace_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .map(decode_addresses)
    .transpose()
}

/// Apply a validated patch in one statement (absent = keep). `keep_ftp_password`: the new FTP
/// config left the password blank, so carry the stored one over when the server and login
/// are unchanged (else store null). False = no such live row.
pub async fn update(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
    relation_type_id: Uuid,
    p: &crate::modules::relations::dto::RelationPatch,
    keep_ftp_password: bool,
) -> Result<bool, sqlx::Error> {
    let text = |v: &Option<Option<String>>| (v.is_some(), v.clone().flatten());
    let json = |v: &Option<Option<Value>>| (v.is_some(), v.clone().flatten());
    let (s_alias, alias) = text(&p.alias);
    let (s_company, company) = text(&p.company_name);
    let (s_coc, coc) = text(&p.coc_number);
    let (s_vat, vat) = text(&p.vat_number);
    let (s_email, email) = text(&p.email);
    let (s_website, website) = text(&p.website);
    let (s_phone, phone) = text(&p.phone_number);
    let (s_language, language) = text(&p.default_language);
    let (s_import_source, import_source) = text(&p.import_source);
    let (s_import_id, import_id) = text(&p.import_id);
    let (s_account, account) = json(&p.financial_account);
    let (s_terms, terms) = json(&p.terms_and_periods);
    let (s_supplier, supplier) = json(&p.supplier_data);
    let (s_meta, meta) = json(&p.meta);
    let addresses = p
        .addresses
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
    sqlx::query!(
        r#"UPDATE relations
           SET type = $3,
               relation_type_id = $4,
               alias = CASE WHEN $5 THEN $6 ELSE alias END,
               company_name = CASE WHEN $7 THEN $8 ELSE company_name END,
               coc_number = CASE WHEN $9 THEN $10 ELSE coc_number END,
               vat_number = CASE WHEN $11 THEN $12 ELSE vat_number END,
               email = CASE WHEN $13 THEN $14 ELSE email END,
               website = CASE WHEN $15 THEN $16 ELSE website END,
               phone_number = CASE WHEN $17 THEN $18 ELSE phone_number END,
               default_language = CASE WHEN $19 THEN $20 ELSE default_language END,
               import_source = CASE WHEN $21 THEN $22 ELSE import_source END,
               import_id = CASE WHEN $23 THEN $24 ELSE import_id END,
               addresses = COALESCE($25, addresses),
               financial_account = CASE WHEN $26 THEN $27 ELSE financial_account END,
               terms_and_periods = CASE WHEN $28 THEN $29 ELSE terms_and_periods END,
               supplier_data = CASE
                   WHEN NOT $30 THEN supplier_data
                   -- Keep the stored password only for the same server and login: a new
                   -- host must never inherit the old secret.
                   WHEN $31 AND supplier_data #> '{order_ftp,password}' IS NOT NULL
                        AND $32::jsonb #> '{order_ftp,host}' IS NOT DISTINCT FROM supplier_data #> '{order_ftp,host}'
                        AND $32::jsonb #> '{order_ftp,port}' IS NOT DISTINCT FROM supplier_data #> '{order_ftp,port}'
                        AND $32::jsonb #> '{order_ftp,username}' IS NOT DISTINCT FROM supplier_data #> '{order_ftp,username}'
                        AND $32::jsonb #> '{order_ftp,type}' IS NOT DISTINCT FROM supplier_data #> '{order_ftp,type}'
                       THEN jsonb_set($32::jsonb, '{order_ftp,password}', supplier_data #> '{order_ftp,password}')
                   WHEN $31 THEN jsonb_set($32::jsonb, '{order_ftp,password}', 'null')
                   ELSE $32 END,
               allow_invoicing_on_account = COALESCE($33, allow_invoicing_on_account),
               disable_order_workflow_emails = COALESCE($34, disable_order_workflow_emails),
               meta = CASE WHEN $35 THEN $36 ELSE meta END,
               updated_at = now()
           WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL"#,
        id,
        workspace_id,
        p.kind,
        relation_type_id,
        s_alias,
        alias,
        s_company,
        company,
        s_coc,
        coc,
        s_vat,
        vat,
        s_email,
        email,
        s_website,
        website,
        s_phone,
        phone,
        s_language,
        language,
        s_import_source,
        import_source,
        s_import_id,
        import_id,
        addresses,
        s_account,
        account,
        s_terms,
        terms,
        s_supplier,
        keep_ftp_password,
        supplier,
        p.allow_invoicing_on_account,
        p.disable_order_workflow_emails,
        s_meta,
        meta,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// Soft delete. False = no such live row. Contacts stay (Laravel parity).
pub async fn soft_delete(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        "UPDATE relations SET deleted_at = now(), updated_at = now()
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .execute(db)
    .await
    .map(|done| done.rows_affected() == 1)
}

/// What the financial summary needs from a live relation: stored account and creation time.
pub async fn summary_source(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<Option<(Option<Value>, DateTime<Utc>)>, sqlx::Error> {
    Ok(sqlx::query!(
        "SELECT financial_account, created_at FROM relations
         WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        id,
        workspace_id,
    )
    .fetch_optional(db)
    .await?
    .map(|r| (r.financial_account, r.created_at)))
}
