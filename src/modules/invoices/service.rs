//! Invoice drafts: create, update (items diffed), cancel, duplicate, list and detail. Every
//! write runs in one transaction that locks what it reads (relation, products, the invoice
//! row), writes the lines in batched statements and stores the totals before commit.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    http::{json::human, list::Page},
    modules::{
        currencies::{self, dto::CurrencyResource},
        invoices::{
            dto::{InvoiceInput, ItemFields, RowKind},
            repo::{self, HeaderPatch, LineChanges, NewInvoice, NewLines},
            resource::{self, Embeds},
        },
        pricing::engine,
        products,
        relations::{
            self,
            address::{self, Address},
        },
        warehouses::{self, dto::WarehouseCompacted},
    },
};

/// Most lines one invoice holds (a request carries at most as many).
const MAX_LINES: i64 = crate::modules::pricing::body::MAX_ITEMS as i64;
const NOT_DRAFT: &str = "This Invoice does not have the correct status to be updated.";
/// Laravel CurrencyExist / `currency.response.no_default_currency`.
const NO_CURRENCY: &str = "This currency id does not exist";
const NO_MAIN_CURRENCY: &str = "No default currency has been found. Please create one";

/// Collected 422s of one request.
#[derive(Default)]
struct Errors(BTreeMap<String, Vec<String>>);

impl Errors {
    fn add(&mut self, field: &str, msg: impl Into<String>) {
        self.0.entry(field.to_owned()).or_default().push(msg.into());
    }
    fn invalid(&mut self, field: &str) {
        self.add(field, format!("The selected {} is invalid.", human(field)));
    }
    fn into_result(self) -> Result<(), AppError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation(self.0))
        }
    }
}

/// New draft; returns its id.
pub async fn create(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    input: InvoiceInput,
) -> Result<Uuid, AppError> {
    let mut tx = state.db.begin().await?;
    let mut errors = Errors::default();
    let relation_id = input.relation_id.unwrap_or_default();
    // Lock order everywhere: invoice row, relation, products, variants, warehouses.
    let addresses = relation(&mut tx, workspace_id, relation_id, &mut errors).await?;
    let currency_id = currency(&mut tx, workspace_id, input.currency_id, &mut errors).await?;
    let contact = contact(&mut tx, workspace_id, relation_id, &input, &mut errors).await?;
    let refs = Refs::load(&mut tx, workspace_id, &input.items, &mut errors).await?;
    errors.into_result()?;
    let billing = match &input.billing_address {
        Some(Some(a)) => Some(a.clone()),
        Some(None) => None,
        None => addresses.as_deref().and_then(billing_default).cloned(),
    };
    let billing = billing.map(|a| to_json(&a)).transpose()?;
    let meta = input.meta.clone().flatten();
    let id = repo::insert(
        &mut tx,
        workspace_id,
        &NewInvoice {
            relation_id,
            currency_id,
            is_credit: input.is_credit,
            payment_method: input.payment_method.as_deref().unwrap_or("bank_transfer"),
            reference: input.reference.clone().flatten().as_deref(),
            note: input.note.clone().flatten().as_deref(),
            billing_address: billing.as_ref(),
            addressed_contact: contact.flatten().as_ref(),
            meta: meta.as_ref(),
            created_by: user_id,
        },
    )
    .await?;
    write_lines(&mut tx, workspace_id, id, &input, &refs, &HashSet::new()).await?;
    store_totals(&mut tx, workspace_id, id, currency_id).await?;
    tx.commit().await?;
    Ok(id)
}

/// Patch a draft and diff its lines (Laravel UpdateInvoice); 422 unless it is a draft.
pub async fn update(
    state: &AppState,
    workspace_id: Uuid,
    id: &str,
    input: InvoiceInput,
) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    let currency_id = draft(&mut tx, workspace_id, id).await?;
    let mut errors = Errors::default();
    let relation_id = input.relation_id.unwrap_or_default();
    relation(&mut tx, workspace_id, relation_id, &mut errors).await?;
    let contact = contact(&mut tx, workspace_id, relation_id, &input, &mut errors).await?;
    let refs = Refs::load(&mut tx, workspace_id, &input.items, &mut errors).await?;
    errors.into_result()?;
    let billing = match &input.billing_address {
        Some(Some(a)) => Some(Some(to_json(a)?)),
        Some(None) => Some(None),
        None => None,
    };
    repo::update_header(
        &mut tx,
        workspace_id,
        id,
        &HeaderPatch {
            relation_id,
            set_reference: input.reference.is_some(),
            reference: input.reference.clone().flatten().as_deref(),
            set_note: input.note.is_some(),
            note: input.note.clone().flatten().as_deref(),
            set_due_date: input.due_date.is_some(),
            due_date: input.due_date.flatten(),
            set_billing_address: billing.is_some(),
            billing_address: billing.as_ref().and_then(Option::as_ref),
            set_addressed_contact: contact.is_some(),
            addressed_contact: contact.as_ref().and_then(Option::as_ref),
            set_customer_ref: input.customer_ref.is_some(),
            customer_ref: input.customer_ref.as_ref().and_then(Option::as_ref),
            set_meta: input.meta.is_some(),
            meta: input.meta.as_ref().and_then(Option::as_ref),
        },
    )
    .await?;
    let existing: HashSet<Uuid> = repo::item_ids(&mut tx, id).await?.into_iter().collect();
    write_lines(&mut tx, workspace_id, id, &input, &refs, &existing).await?;
    store_totals(&mut tx, workspace_id, id, currency_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Laravel DeleteInvoice: cancels a draft (the row and its lines stay).
pub async fn cancel(state: &AppState, workspace_id: Uuid, id: &str) -> Result<(), AppError> {
    let id = parse(id)?;
    let mut tx = state.db.begin().await?;
    draft(&mut tx, workspace_id, id).await?;
    repo::set_status(&mut tx, workspace_id, id, "cancelled").await?;
    tx.commit().await?;
    Ok(())
}

/// Copy of any live invoice as a new draft: relation, currency, credit flag, note, meta and
/// the lines (Laravel DuplicateInvoice; purchase prices are kept, Laravel reset them to 0).
pub async fn duplicate(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
) -> Result<Uuid, AppError> {
    let source = parse(id)?;
    let mut tx = state.db.begin().await?;
    let row = repo::find(&mut *tx, workspace_id, source)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&mut *tx, source).await?;
    let copy = repo::insert(
        &mut tx,
        workspace_id,
        &NewInvoice {
            relation_id: row.relation_id,
            currency_id: row.currency_id,
            is_credit: row.is_credit,
            payment_method: "bank_transfer",
            reference: None,
            note: row.note.as_deref(),
            billing_address: None,
            addressed_contact: None,
            meta: row.meta.as_ref(),
            created_by: user_id,
        },
    )
    .await?;
    let mut n = NewLines::default();
    for (i, l) in lines.iter().enumerate() {
        n.product_id.push(l.product_id);
        n.product_variant_id.push(l.product_variant_id);
        n.warehouse_id.push(l.mutate_from_warehouse_id);
        n.title.push(l.title.clone());
        n.description.push(l.description.clone());
        n.quantity.push(l.quantity);
        n.price.push(l.price);
        n.vat.push(l.vat);
        n.discount.push(l.discount_percentage);
        n.purchase_price.push(l.purchase_price);
        n.position.push(i32::try_from(i).unwrap_or(i32::MAX));
    }
    repo::insert_lines(&mut tx, workspace_id, copy, &n).await?;
    store_totals(&mut tx, workspace_id, copy, row.currency_id).await?;
    tx.commit().await?;
    Ok(copy)
}

/// One invoice in detail, else 404.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<Value, AppError> {
    let id = parse(id)?;
    let row = repo::find(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&state.db, id).await?;
    let dp = decimals(state, workspace_id, row.currency_id).await?;
    let engine_lines: Vec<engine::Line> = lines.iter().map(engine_line).collect();
    let summary = engine::summarise(&engine_lines, dp).map_err(|_| out_of_range())?;
    let margin = engine::margin(&engine_lines, &summary.lines, dp).map_err(|_| out_of_range())?;
    let mut e = embeds(state, workspace_id, std::slice::from_ref(&row)).await?;
    let product_ids: Vec<Uuid> = unique(lines.iter().filter_map(|l| l.product_id));
    e.products = products::service::render_many(state, workspace_id, &product_ids).await?;
    let warehouse_ids: Vec<Uuid> = unique(lines.iter().filter_map(|l| l.mutate_from_warehouse_id));
    for w in warehouses::repo::compact_many(&state.db, workspace_id, &warehouse_ids).await? {
        e.warehouses
            .insert(w.id, to_json(&WarehouseCompacted::from(&w))?);
    }
    Ok(resource::detail(&row, &lines, &summary, &margin, &e))
}

/// List query, validated.
pub struct ListArgs<'a> {
    pub filter: repo::ListFilter<'a>,
    /// Laravel `limit`: that many rows, no paging meta.
    pub limit: Option<i64>,
    pub page: Page,
}

/// Rows of one page (or of `limit`) plus the total.
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    args: &ListArgs<'_>,
) -> Result<(Vec<Value>, i64), AppError> {
    let (size, offset) = match args.limit {
        Some(n) => (n, 0),
        None => (args.page.size, args.page.offset()),
    };
    let (rows, total) = repo::list(&state.db, workspace_id, &args.filter, size, offset).await?;
    let e = embeds(state, workspace_id, &rows).await?;
    Ok((rows.iter().map(|r| resource::index(r, &e)).collect(), total))
}

/// Laravel DeleteProduct guard: 422 while any line uses the product.
pub async fn product_deletable(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    product_id: Uuid,
) -> Result<(), AppError> {
    if repo::product_in_use(conn, workspace_id, product_id).await? {
        return Err(AppError::Unprocessable(
            "Product can not be deleted because it is used in items.".into(),
        ));
    }
    Ok(())
}

/// Laravel DeleteRelation guard: 422 on `relation_id` while it has invoices.
pub async fn relation_deletable(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    relation_id: Uuid,
) -> Result<(), AppError> {
    if repo::relation_has_documents(conn, workspace_id, relation_id).await? {
        return Err(AppError::field(
            "relation_id",
            "This relation cannot be deleted because it has invoices, orders, estimates, or work orders.",
        ));
    }
    Ok(())
}

fn parse(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<Value, AppError> {
    serde_json::to_value(v).map_err(|e| AppError::Internal(e.to_string()))
}

fn unique(ids: impl Iterator<Item = Uuid>) -> Vec<Uuid> {
    ids.collect::<BTreeSet<_>>().into_iter().collect()
}

fn out_of_range() -> AppError {
    AppError::field("items", "The item totals are out of range.")
}

/// Lock the live invoice `id`; its currency. 404 when unknown, 422 unless a draft.
// shortcut: Laravel let credit invoices change in any status; credits arrive with F6.
async fn draft(conn: &mut PgConnection, workspace_id: Uuid, id: Uuid) -> Result<Uuid, AppError> {
    let (status, _, currency_id) = repo::lock(conn, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if status != "draft" {
        return Err(AppError::Unprocessable(NOT_DRAFT.into()));
    }
    Ok(currency_id)
}

/// Lock the live relation; its addresses. 422 when unknown or deleted.
async fn relation(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    errors: &mut Errors,
) -> Result<Option<Vec<Address>>, AppError> {
    match repo::lock_relation(conn, workspace_id, id).await? {
        Some(v) => Ok(Some(relations::repo::decode_addresses(v)?)),
        None => {
            errors.add(
                "relation_id",
                "The relation id is an invalid id for the relation domain.",
            );
            Ok(None)
        }
    }
}

/// Laravel's default billing address: the `billing` one, else the main one, else the first.
fn billing_default(addresses: &[Address]) -> Option<&Address> {
    addresses
        .iter()
        .find(|a| a.kind.as_deref() == Some("billing"))
        .or_else(|| address::default_of(addresses))
}

/// The sent currency, else the workspace's main one.
async fn currency(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    sent: Option<Option<Uuid>>,
    errors: &mut Errors,
) -> Result<Uuid, AppError> {
    let found = match sent {
        Some(Some(id)) => currencies::repo::find(&mut *conn, workspace_id, id)
            .await?
            .map(|c| c.id),
        Some(None) => None,
        None => {
            let main = products::repo::main_currency(&mut *conn, workspace_id).await?;
            if main.is_none() {
                errors.add("currency_id", NO_MAIN_CURRENCY);
                return Ok(Uuid::nil());
            }
            main
        }
    };
    Ok(found.unwrap_or_else(|| {
        errors.add("currency_id", NO_CURRENCY);
        Uuid::nil()
    }))
}

/// Snapshot of the addressed contact: Some(None) clears it, None (not sent) keeps it.
async fn contact(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    relation_id: Uuid,
    input: &InvoiceInput,
    errors: &mut Errors,
) -> Result<Option<Option<Value>>, AppError> {
    let Some(sent) = input.addressed_contact_id else {
        return Ok(None);
    };
    let Some(raw) = sent else {
        return Ok(Some(None));
    };
    let found = match raw {
        Some(id) => repo::addressed_contact(&mut *conn, workspace_id, relation_id, id).await?,
        None => None,
    };
    if found.is_none() {
        errors.invalid("addressed_contact_id");
    }
    Ok(Some(found))
}

/// Live products, variants and warehouses the new lines name, key-share locked so a delete
/// waits for this transaction (and then sees the lines).
struct Refs {
    products: HashMap<Uuid, repo::ProductDefaults>,
    variants: HashSet<Uuid>,
}

impl Refs {
    async fn load(
        conn: &mut PgConnection,
        workspace_id: Uuid,
        rows: &[crate::modules::invoices::dto::ItemRow],
        errors: &mut Errors,
    ) -> Result<Self, AppError> {
        let fields = || {
            rows.iter().filter_map(|r| match &r.kind {
                RowKind::New(f) | RowKind::Update(_, f) => Some((r.index, f)),
                _ => None,
            })
        };
        let new_products = unique(rows.iter().filter_map(|r| match &r.kind {
            RowKind::New(f) => f.product_id.flatten(),
            _ => None,
        }));
        let products: HashMap<Uuid, repo::ProductDefaults> =
            repo::lock_products(conn, workspace_id, &new_products)
                .await?
                .into_iter()
                .map(|p| (p.id, p))
                .collect();
        let variants: HashSet<Uuid> = repo::lock_variants(
            conn,
            workspace_id,
            &unique(fields().filter_map(|(_, f)| f.product_variant_id.flatten())),
        )
        .await?
        .into_iter()
        .collect();
        let warehouses: HashSet<Uuid> = repo::lock_warehouses(
            conn,
            workspace_id,
            &unique(fields().filter_map(|(_, f)| f.mutate_from_warehouse_id.flatten())),
        )
        .await?
        .into_iter()
        .collect();
        for r in rows {
            if let RowKind::New(f) = &r.kind
                && let Some(p) = f.product_id
                && !p.is_some_and(|p| products.contains_key(&p))
            {
                errors.invalid(&format!("items.{}.product_id", r.index));
            }
        }
        for (index, f) in fields() {
            if let Some(w) = f.mutate_from_warehouse_id
                && !w.is_some_and(|w| warehouses.contains(&w))
            {
                errors.invalid(&format!("items.{index}.mutate_from_warehouse_id"));
            }
        }
        Ok(Self { products, variants })
    }

    /// The sent variant when live (Laravel dropped an unknown one), else the product's default.
    fn variant(&self, f: &ItemFields, product: Option<&repo::ProductDefaults>) -> Option<Uuid> {
        f.product_variant_id
            .flatten()
            .filter(|v| self.variants.contains(v))
            .or_else(|| product.and_then(|p| p.default_variant_id))
    }
}

/// Apply the item rows: new lines inserted, sent existing lines updated, `deleted` ones
/// removed; rows not sent stay (Laravel HandleItems). An id that isn't a line of this invoice
/// is a 422 (Laravel changed another invoice's line); a deleted one is skipped.
async fn write_lines(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    input: &InvoiceInput,
    refs: &Refs,
    existing: &HashSet<Uuid>,
) -> Result<(), AppError> {
    let mut errors = Errors::default();
    let (mut new, mut changed, mut deleted) =
        (NewLines::default(), LineChanges::default(), Vec::new());
    for row in &input.items {
        let position = |f: &ItemFields| f.position.or(i32::try_from(row.index).ok());
        match &row.kind {
            RowKind::New(f) => {
                let product = f.product_id.flatten().and_then(|p| refs.products.get(&p));
                new.product_id.push(product.map(|p| p.id));
                new.product_variant_id.push(refs.variant(f, product));
                new.warehouse_id.push(
                    f.mutate_from_warehouse_id
                        .flatten()
                        .or_else(|| product.and_then(|p| p.default_warehouse_id)),
                );
                new.title.push(f.title.clone());
                new.description.push(f.description.clone());
                new.quantity.push(f.quantity);
                new.price.push(f.price.unwrap_or_default());
                new.vat.push(f.vat);
                new.discount.push(f.discount.unwrap_or_default());
                new.purchase_price
                    .push(f.purchase_price.unwrap_or_default());
                new.position.push(position(f).unwrap_or(1));
            }
            RowKind::Update(id, f) if existing.contains(id) => {
                changed.id.push(*id);
                changed.product_variant_id.push(
                    f.product_variant_id
                        .flatten()
                        .filter(|v| refs.variants.contains(v)),
                );
                changed
                    .warehouse_id
                    .push(f.mutate_from_warehouse_id.flatten());
                changed.title.push(f.title.clone());
                changed.description.push(f.description.clone());
                changed.quantity.push(f.quantity);
                changed.price.push(f.price);
                changed.vat.push(f.vat);
                changed.discount.push(f.discount);
                // Laravel UpdateItem: a falsy purchase price stores 0.
                changed
                    .purchase_price
                    .push(f.purchase_price.unwrap_or_default());
                changed.position.push(f.position);
            }
            RowKind::Update(..) => errors.invalid(&format!("items.{}.id", row.index)),
            RowKind::Delete(Some(id)) if existing.contains(id) => deleted.push(*id),
            RowKind::Delete(_) | RowKind::Skip => {}
        }
    }
    errors.into_result()?;
    repo::delete_lines(conn, invoice_id, &deleted).await?;
    repo::update_lines(conn, invoice_id, &changed).await?;
    repo::insert_lines(conn, workspace_id, invoice_id, &new).await?;
    Ok(())
}

/// Recompute the totals from the stored lines and write them (C1: same transaction).
async fn store_totals(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    currency_id: Uuid,
) -> Result<(), AppError> {
    let lines = repo::lines(&mut *conn, invoice_id).await?;
    if lines.len() as i64 > MAX_LINES {
        return Err(AppError::field(
            "items",
            format!("An invoice holds at most {MAX_LINES} items."),
        ));
    }
    let dp = dp(repo::currency_decimals(&mut *conn, workspace_id, currency_id).await?)?;
    let engine_lines: Vec<engine::Line> = lines.iter().map(engine_line).collect();
    let s = engine::summarise(&engine_lines, dp).map_err(|_| out_of_range())?;
    let discounted = lines
        .iter()
        .zip(&s.lines)
        .filter(|(l, a)| l.discount_percentage > Decimal::ZERO && a.gross > Decimal::ZERO)
        .count();
    let minor = |v: Decimal| {
        v.checked_mul(Decimal::from(10_i64.pow(dp)))
            .and_then(|m| i64::try_from(m.round()).ok())
            .ok_or_else(out_of_range)
    };
    repo::set_totals(
        conn,
        invoice_id,
        minor(s.excl)?,
        minor(s.total)?,
        &resource::stored_summary(&s, discounted),
    )
    .await?;
    Ok(())
}

fn engine_line(l: &repo::ItemRow) -> engine::Line {
    engine::Line {
        quantity: l.quantity,
        price: l.price,
        vat: l.vat,
        discount: l.discount_percentage,
        purchase_price: Some(l.purchase_price),
        set_child: false,
    }
}

async fn decimals(
    state: &AppState,
    workspace_id: Uuid,
    currency_id: Uuid,
) -> Result<u32, AppError> {
    dp(repo::currency_decimals(&state.db, workspace_id, currency_id).await?)
}

/// CHECK (decimals BETWEEN 0 AND 8) on the column.
fn dp(decimals: i16) -> Result<u32, AppError> {
    u32::try_from(decimals).map_err(|_| AppError::Internal("negative currency decimals".into()))
}

/// Relations and currencies of `rows`, in their resource shape.
async fn embeds(
    state: &AppState,
    workspace_id: Uuid,
    rows: &[repo::InvoiceRow],
) -> Result<Embeds, AppError> {
    let relation_ids = unique(rows.iter().map(|r| r.relation_id));
    let currency_ids = unique(rows.iter().map(|r| r.currency_id));
    let mut e = Embeds {
        relations: relations::service::index_values(state, workspace_id, &relation_ids).await?,
        ..Embeds::default()
    };
    for c in currencies::repo::find_many(&state.db, workspace_id, &currency_ids).await? {
        e.currencies
            .insert(c.id, to_json(&CurrencyResource::from(c))?);
    }
    Ok(e)
}
