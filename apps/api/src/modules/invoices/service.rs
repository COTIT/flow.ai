//! Invoice drafts: create, update (items diffed), cancel, duplicate, list and detail. Every
//! write runs in one transaction that locks what it reads (invoice row, relation, products),
//! writes the lines in batched statements and stores the totals before commit.

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
            dto::{InvoiceInput, ItemFields, ItemRow, RowKind},
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
const MAX_LINES: usize = crate::modules::pricing::body::MAX_ITEMS;
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
    // Lock order everywhere: invoice row, relation, products.
    let addresses = relation(&mut tx, workspace_id, relation_id, &mut errors).await?;
    let currency_id = currency(&mut tx, workspace_id, input.currency_id, &mut errors).await?;
    let contact = contact(&mut tx, workspace_id, relation_id, &input, &mut errors).await?;
    let refs = Refs::load(&mut tx, workspace_id, &input.items, &mut errors).await?;
    errors.into_result()?;
    let billing = match &input.billing_address {
        Some(Some(a)) => Some(to_json(a)?),
        Some(None) => None,
        None => default_billing(addresses.as_deref())?,
    };
    let id = repo::insert(
        &mut tx,
        workspace_id,
        &NewInvoice {
            relation_id,
            currency_id,
            is_credit: input.is_credit,
            payment_method: input.payment_method.as_deref().unwrap_or("bank_transfer"),
            reference: input.reference.as_ref().and_then(Option::as_deref),
            note: input.note.as_ref().and_then(Option::as_deref),
            billing_address: billing.as_ref(),
            addressed_contact: contact.as_ref().and_then(Option::as_ref),
            meta: input.meta.as_ref().and_then(Option::as_ref),
            created_by: user_id,
        },
    )
    .await?;
    write_lines(
        &mut tx,
        workspace_id,
        id,
        &input.items,
        &refs,
        &HashMap::new(),
    )
    .await?;
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
    let locked = draft(&mut tx, workspace_id, id).await?;
    let mut errors = Errors::default();
    let relation_id = input.relation_id.unwrap_or_default();
    let addresses = relation(&mut tx, workspace_id, relation_id, &mut errors).await?;
    let mut contact = contact(&mut tx, workspace_id, relation_id, &input, &mut errors).await?;
    let lines: HashMap<Uuid, Option<Uuid>> = repo::item_products(&mut tx, workspace_id, id)
        .await?
        .into_iter()
        .collect();
    let refs = Refs::load(&mut tx, workspace_id, &input.items, &mut errors).await?;
    errors.into_result()?;
    let mut billing = match &input.billing_address {
        Some(Some(a)) => Some(Some(to_json(a)?)),
        Some(None) => Some(None),
        None => None,
    };
    // Another relation: the old relation's contact and address must not stay on the invoice.
    if relation_id != locked.relation_id {
        contact.get_or_insert(None);
        if billing.is_none() {
            billing = Some(default_billing(addresses.as_deref())?);
        }
    }
    repo::update_header(
        &mut tx,
        workspace_id,
        id,
        &HeaderPatch {
            relation_id,
            set_reference: input.reference.is_some(),
            reference: input.reference.as_ref().and_then(Option::as_deref),
            set_note: input.note.is_some(),
            note: input.note.as_ref().and_then(Option::as_deref),
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
    write_lines(&mut tx, workspace_id, id, &input.items, &refs, &lines).await?;
    store_totals(&mut tx, workspace_id, id, locked.currency_id).await?;
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
/// The source is share locked, so its lines (and the products on them) hold still.
pub async fn duplicate(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
) -> Result<Uuid, AppError> {
    let source = parse(id)?;
    let mut tx = state.db.begin().await?;
    let row = repo::find_for_share(&mut tx, workspace_id, source)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&mut *tx, workspace_id, source).await?;
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
    for (i, l) in lines.into_iter().enumerate() {
        n.product_id.push(l.product_id);
        n.product_variant_id.push(l.product_variant_id);
        n.warehouse_id.push(l.mutate_from_warehouse_id);
        n.title.push(l.title);
        n.description.push(l.description);
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

/// One invoice in detail, else 404. Lines price at the decimals the totals were stored with,
/// so a later currency edit doesn't change the invoice.
pub async fn get(state: &AppState, workspace_id: Uuid, id: &str) -> Result<Value, AppError> {
    let id = parse(id)?;
    let row = repo::find(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&state.db, workspace_id, id).await?;
    let dp = match resource::read_summary(&row.summary).2 {
        Some(dp) => dp,
        None => dp(repo::currency_decimals(&state.db, workspace_id, row.currency_id).await?)?,
    };
    let (summary, margin) = price(&lines, dp)?;
    let mut e = embeds(state, workspace_id, std::slice::from_ref(&row)).await?;
    let product_ids = unique(lines.iter().filter_map(|l| l.product_id));
    e.products = products::service::render_many(state, workspace_id, &product_ids).await?;
    let warehouse_ids = unique(lines.iter().filter_map(|l| l.mutate_from_warehouse_id));
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

/// Rows of one page (or of `limit`) plus the total (not counted for `limit`).
pub async fn list(
    state: &AppState,
    workspace_id: Uuid,
    args: &ListArgs<'_>,
) -> Result<(Vec<Value>, i64), AppError> {
    let (size, offset) = match args.limit {
        Some(n) => (n, 0),
        None => (args.page.size, args.page.offset()),
    };
    let count = args.limit.is_none();
    let (rows, total) =
        repo::list(&state.db, workspace_id, &args.filter, size, offset, count).await?;
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

/// Lock the live invoice `id`. 404 when unknown, 422 unless a draft.
// shortcut: Laravel let credit invoices change in any status; credits arrive with F6.
async fn draft(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<repo::Locked, AppError> {
    let locked = repo::lock(conn, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if locked.status != "draft" {
        return Err(AppError::Unprocessable(NOT_DRAFT.into()));
    }
    Ok(locked)
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

/// Laravel's default billing address: the relation's `billing` one, else its main one, else
/// its first; as stored on the invoice.
fn default_billing(addresses: Option<&[Address]>) -> Result<Option<Value>, AppError> {
    let addresses = addresses.unwrap_or_default();
    addresses
        .iter()
        .find(|a| a.kind.as_deref() == Some("billing"))
        .or_else(|| address::default_of(addresses))
        .map(to_json)
        .transpose()
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

/// Fields of a new or updated row, with its body index.
fn sent_fields(rows: &[ItemRow]) -> impl Iterator<Item = (usize, &ItemFields)> {
    rows.iter().filter_map(|r| match &r.kind {
        RowKind::New(f) | RowKind::Update(_, f) => Some((r.index, f)),
        RowKind::Delete(_) | RowKind::Skip => None,
    })
}

/// What the item rows refer to: live products of new lines (key-share locked, so a product
/// delete waits for this transaction and then sees the lines), live variants by product, live
/// warehouses.
struct Refs {
    products: HashMap<Uuid, repo::ProductDefaults>,
    variants: HashMap<Uuid, Uuid>,
    warehouses: HashSet<Uuid>,
}

impl Refs {
    /// Collect 422s for unknown products and warehouses.
    async fn load(
        conn: &mut PgConnection,
        workspace_id: Uuid,
        rows: &[ItemRow],
        errors: &mut Errors,
    ) -> Result<Self, AppError> {
        let new_products = unique(rows.iter().filter_map(|r| match &r.kind {
            RowKind::New(f) => f.product_id.flatten(),
            RowKind::Update(..) | RowKind::Delete(_) | RowKind::Skip => None,
        }));
        let products: HashMap<Uuid, repo::ProductDefaults> =
            repo::lock_products(conn, workspace_id, &new_products)
                .await?
                .into_iter()
                .map(|p| (p.id, p))
                .collect();
        let sent_variants =
            unique(sent_fields(rows).filter_map(|(_, f)| f.product_variant_id.flatten()));
        let variants = repo::variants(conn, workspace_id, &sent_variants)
            .await?
            .into_iter()
            .map(|v| (v.id, v.product_id))
            .collect();
        // Sent warehouses plus the product defaults new lines take.
        let wanted = unique(
            sent_fields(rows)
                .filter_map(|(_, f)| f.mutate_from_warehouse_id.flatten().flatten())
                .chain(products.values().filter_map(|p| p.default_warehouse_id)),
        );
        let warehouses: HashSet<Uuid> = repo::warehouses(conn, workspace_id, &wanted)
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
        for (index, f) in sent_fields(rows) {
            if let Some(Some(w)) = f.mutate_from_warehouse_id
                && !w.is_some_and(|w| warehouses.contains(&w))
            {
                errors.invalid(&format!("items.{index}.mutate_from_warehouse_id"));
            }
        }
        Ok(Self {
            products,
            variants,
            warehouses,
        })
    }

    /// The sent variant when it is a live variant of `product` (Laravel dropped an unknown
    /// one), else the product's default for a new line.
    fn variant(&self, f: &ItemFields, product: Option<Uuid>) -> Option<Uuid> {
        f.product_variant_id
            .flatten()
            .filter(|v| product.is_some() && self.variants.get(v).copied() == product)
    }

    /// The warehouse a new line takes: the sent one, else the product's default while live.
    fn warehouse(&self, f: &ItemFields, product: Option<&repo::ProductDefaults>) -> Option<Uuid> {
        match f.mutate_from_warehouse_id {
            Some(sent) => sent.flatten(),
            None => product
                .and_then(|p| p.default_warehouse_id)
                .filter(|w| self.warehouses.contains(w)),
        }
    }
}

/// Apply the item rows: new lines inserted, sent existing lines updated, `deleted` ones
/// removed; rows not sent stay (Laravel HandleItems). An id that isn't a line of this invoice
/// is a 422 (Laravel changed another invoice's line); deleting one is a no-op. Several rows
/// with one id: the last wins, as Laravel applied them in order.
async fn write_lines(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    rows: &[ItemRow],
    refs: &Refs,
    lines: &HashMap<Uuid, Option<Uuid>>,
) -> Result<(), AppError> {
    let mut errors = Errors::default();
    let last: HashMap<Uuid, usize> = rows
        .iter()
        .filter_map(|r| match &r.kind {
            RowKind::Update(id, _) | RowKind::Delete(Some(id)) => Some((*id, r.index)),
            RowKind::New(_) | RowKind::Delete(None) | RowKind::Skip => None,
        })
        .collect();
    let (mut new, mut changed, mut deleted) =
        (NewLines::default(), LineChanges::default(), Vec::new());
    for row in rows {
        let superseded = |id: &Uuid| last.get(id) != Some(&row.index);
        match &row.kind {
            RowKind::New(f) => {
                let product = f.product_id.flatten().and_then(|p| refs.products.get(&p));
                let product_id = product.map(|p| p.id);
                new.product_id.push(product_id);
                new.product_variant_id.push(
                    refs.variant(f, product_id)
                        .or_else(|| product.and_then(|p| p.default_variant_id)),
                );
                new.warehouse_id.push(refs.warehouse(f, product));
                new.title.push(f.title.clone());
                new.description.push(f.description.clone().flatten());
                new.quantity.push(f.quantity);
                new.price.push(f.price.unwrap_or_default());
                new.vat.push(f.vat);
                new.discount.push(f.discount.unwrap_or_default());
                new.purchase_price
                    .push(f.purchase_price.unwrap_or_default());
                new.position
                    .push(f.position.or(i32::try_from(row.index).ok()).unwrap_or(1));
            }
            RowKind::Update(id, _) | RowKind::Delete(Some(id)) if superseded(id) => {}
            RowKind::Update(id, f) => {
                let Some(product) = lines.get(id) else {
                    errors.invalid(&format!("items.{}.id", row.index));
                    continue;
                };
                changed.id.push(*id);
                changed.product_variant_id.push(refs.variant(f, *product));
                changed
                    .set_warehouse
                    .push(f.mutate_from_warehouse_id.is_some());
                changed
                    .warehouse_id
                    .push(f.mutate_from_warehouse_id.flatten().flatten());
                changed.title.push(f.title.clone());
                changed.set_description.push(f.description.is_some());
                changed.description.push(f.description.clone().flatten());
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
            RowKind::Delete(Some(id)) if lines.contains_key(id) => deleted.push(*id),
            RowKind::Delete(_) | RowKind::Skip => {}
        }
    }
    errors.into_result()?;
    repo::delete_lines(conn, workspace_id, invoice_id, &deleted).await?;
    repo::update_lines(conn, workspace_id, invoice_id, &changed).await?;
    repo::insert_lines(conn, workspace_id, invoice_id, &new).await?;
    Ok(())
}

/// Totals and margin of `lines` at `dp`; 422 when a figure leaves Decimal's range.
fn price(lines: &[repo::ItemRow], dp: u32) -> Result<(engine::Summary, engine::Margin), AppError> {
    let engine_lines: Vec<engine::Line> = lines.iter().map(engine_line).collect();
    let summary = engine::summarise(&engine_lines, dp).map_err(|_| out_of_range())?;
    let margin = engine::margin(&engine_lines, &summary.lines, dp).map_err(|_| out_of_range())?;
    Ok((summary, margin))
}

/// Recompute the totals from the stored lines and write them (C1: same transaction). The
/// margin is computed too, so a line the detail can't price is refused here.
async fn store_totals(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    currency_id: Uuid,
) -> Result<(), AppError> {
    let lines = repo::lines(&mut *conn, workspace_id, invoice_id).await?;
    if lines.len() > MAX_LINES {
        return Err(AppError::field(
            "items",
            format!("An invoice holds at most {MAX_LINES} items."),
        ));
    }
    let dp = dp(repo::currency_decimals(&mut *conn, workspace_id, currency_id).await?)?;
    let (s, _) = price(&lines, dp)?;
    let discounted = s
        .lines
        .iter()
        .zip(&lines)
        .filter(|(a, l)| resource::discounted(l, a))
        .count();
    let scale = 10_i64
        .checked_pow(dp)
        .map(Decimal::from)
        .ok_or_else(|| AppError::Internal("currency decimals out of range".into()))?;
    let minor = |v: Decimal| {
        v.checked_mul(scale)
            .and_then(|m| i64::try_from(m.round()).ok())
            .ok_or_else(out_of_range)
    };
    repo::set_totals(
        conn,
        workspace_id,
        invoice_id,
        minor(s.excl)?,
        minor(s.total)?,
        &resource::stored_summary(&s, discounted, dp),
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

/// Render the invoice as a PDF (with `template_id` when it is an invoice template of this
/// workspace, else the built-in layout, as Laravel), store it as a file; that file.
pub async fn preview(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    template_id: Option<Uuid>,
) -> Result<crate::modules::files::dto::FileResource, AppError> {
    use crate::modules::{files, pdf_templates};
    let id = parse(id)?;
    files::service::store(state)?;
    let template = match template_id {
        Some(t) => pdf_templates::repo::find(&state.db, workspace_id, t, Some("invoice")).await?,
        None => None,
    };
    let pdf =
        crate::modules::invoices::pdf::render(state, workspace_id, id, template.as_ref()).await?;
    let file_id = files::service::create(
        state,
        workspace_id,
        user_id,
        files::service::NewFile {
            document_name: &pdf.file_name,
            description: None,
            meta: None,
            bytes: pdf.bytes,
        },
    )
    .await?;
    files::service::get(state, workspace_id, &file_id.to_string()).await
}
