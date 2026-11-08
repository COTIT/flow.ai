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
        files, invoice_payments,
        invoices::{
            actions::{self, Facts, InvoiceAction},
            dto::{InvoiceInput, ItemFields, ItemRow, RowKind},
            repo::{self, HeaderPatch, LineChanges, NewInvoice, NewLines},
            resource::{self, Embeds},
            send,
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
pub(super) struct Errors(BTreeMap<String, Vec<String>>);

impl Errors {
    pub(super) fn add(&mut self, field: &str, msg: impl Into<String>) {
        self.0.entry(field.to_owned()).or_default().push(msg.into());
    }
    fn invalid(&mut self, field: &str) {
        self.add(field, format!("The selected {} is invalid.", human(field)));
    }
    pub(super) fn into_result(self) -> Result<(), AppError> {
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
    let id = create_in(&mut tx, workspace_id, user_id, input).await?;
    tx.commit().await?;
    Ok(id)
}

/// [`create`] in the caller's transaction (order → invoice).
pub async fn create_in(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
    input: InvoiceInput,
) -> Result<Uuid, AppError> {
    let review_kind = if crate::modules::assistant::preconditions::expects("order") {
        Some("order")
    } else if crate::modules::assistant::preconditions::expects("invoice") {
        Some("invoice")
    } else {
        None
    };
    let defaults = resolve_create(tx, workspace_id, &input, review_kind.is_some()).await?;
    if let Some(kind) = review_kind {
        let actual = create_quote(tx, workspace_id, &input, &defaults).await?;
        crate::modules::assistant::preconditions::check_priced_required(
            workspace_id,
            kind,
            &actual,
        )?;
    }
    let CreateDefaults {
        relation_id,
        currency_id,
        billing,
        contact,
        refs,
    } = defaults;
    let id = repo::insert(
        &mut *tx,
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
            previous_invoice_id: None,
            customer_ref: None,
        },
    )
    .await?;
    write_lines(
        &mut *tx,
        workspace_id,
        id,
        &input.items,
        &refs,
        &HashMap::new(),
    )
    .await?;
    store_totals(&mut *tx, workspace_id, id, currency_id).await?;
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
    let defaults = resolve_update(
        &mut tx,
        workspace_id,
        id,
        &locked,
        &input,
        crate::modules::assistant::preconditions::expects("invoice"),
    )
    .await?;
    if crate::modules::assistant::preconditions::expects("invoice") {
        let actual = update_quote(id, &defaults);
        crate::modules::assistant::preconditions::check_priced_required(
            workspace_id,
            "invoice",
            &actual,
        )?;
    }
    let UpdateDefaults {
        relation_id,
        billing,
        contact,
        lines,
    } = defaults;
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
    write_line_plan(&mut tx, workspace_id, id, &lines).await?;
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
    super::approval::check(&mut tx, workspace_id, source, None).await?;
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
            previous_invoice_id: None,
            customer_ref: None,
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
        n.parent_item_id.push(None);
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
    let line_cents =
        repo::line_cents(&state.db, workspace_id, id, i32::try_from(dp).unwrap_or(2)).await?;
    let mut e = embeds(state, workspace_id, std::slice::from_ref(&row)).await?;
    e.payment_hash = send::payment_token(state, workspace_id, &row);
    e.line_cents = line_cents
        .into_iter()
        .map(|c| (c.id, (c.unit_price_minor, c.subtotal_minor, c.total_minor)))
        .collect();
    e.payments = invoice_payments::repo::list(&state.db, workspace_id, id)
        .await?
        .iter()
        .map(|p| invoice_payments::resource::payment(p, dp))
        .collect();
    // A deleted PDF or a store that isn't configured leaves the invoice readable.
    if let Some(file) = row.pdf_file_id {
        e.pdf_file = match files::service::get(state, workspace_id, &file.to_string()).await {
            Ok(f) => Some(to_json(&f)?),
            Err(AppError::NotFound | AppError::Unavailable(_)) => None,
            Err(err) => return Err(err),
        };
    }
    let on_account = repo::relation_terms(&state.db, workspace_id, row.relation_id)
        .await?
        .is_some_and(|t| t.allow_invoicing_on_account);
    e.allowed_actions = actions::allowed(&Facts {
        status: &row.status,
        is_credit: row.is_credit,
        numbered: row.invoice_number.is_some(),
        on_account,
    });
    e.order = repo::order_of(&state.db, workspace_id, id)
        .await?
        .map(|o| to_json(&o))
        .transpose()?;
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

/// Amount still to be paid per currency over the list filter (payable invoices only).
pub async fn open_totals(
    state: &AppState,
    workspace_id: Uuid,
    f: &repo::ListFilter<'_>,
) -> Result<Vec<super::dto::OpenTotal>, AppError> {
    Ok(repo::open_totals(&state.db, workspace_id, f)
        .await?
        .into_iter()
        .map(|r| super::dto::OpenTotal {
            currency: super::dto::OpenTotalCurrency {
                code: r.code,
                decimals: r.decimals,
            },
            open_minor: r.open_minor,
        })
        .collect())
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

pub(super) fn parse(id: &str) -> Result<Uuid, AppError> {
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

/// Lock the live invoice `id`. 404 when unknown, 422 unless a draft (Laravel let credit notes
/// change in any status; here they are edited as drafts only).
async fn draft(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<repo::Locked, AppError> {
    let locked = repo::lock(conn, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    super::approval::check(conn, workspace_id, id, None).await?;
    if !actions::allows(InvoiceAction::Edit, &Facts::of(&locked, false)) {
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
    let plan = line_plan(rows, refs, lines)?;
    write_line_plan(conn, workspace_id, invoice_id, &plan).await
}

struct LinePlan {
    new: NewLines,
    changed: LineChanges,
    deleted: Vec<Uuid>,
}

fn line_plan(
    rows: &[ItemRow],
    refs: &Refs,
    lines: &HashMap<Uuid, Option<Uuid>>,
) -> Result<LinePlan, AppError> {
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
            RowKind::New(f) => append_new_line(&mut new, f, row.index, refs),
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
    Ok(LinePlan {
        new,
        changed,
        deleted,
    })
}

async fn write_line_plan(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    plan: &LinePlan,
) -> Result<(), AppError> {
    repo::delete_lines(conn, workspace_id, invoice_id, &plan.deleted).await?;
    repo::update_lines(conn, workspace_id, invoice_id, &plan.changed).await?;
    repo::insert_lines(conn, workspace_id, invoice_id, &plan.new).await?;
    Ok(())
}

/// Prepared headers and line references consumed unchanged by native insertion.
struct CreateDefaults {
    relation_id: Uuid,
    currency_id: Uuid,
    billing: Option<Value>,
    contact: Option<Option<Value>>,
    refs: Refs,
}

async fn resolve_create(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    input: &InvoiceInput,
    lock_facts: bool,
) -> Result<CreateDefaults, AppError> {
    let relation_id = input.relation_id.unwrap_or_default();
    if lock_facts {
        let products = unique(input.items.iter().filter_map(|row| match &row.kind {
            RowKind::New(f) => f.product_id.flatten(),
            _ => None,
        }));
        share_input_defaults(conn, workspace_id, input, &products).await?;
    }
    let mut errors = Errors::default();
    // Lock order everywhere: invoice row, relation, products.
    let addresses = relation(&mut *conn, workspace_id, relation_id, &mut errors).await?;
    let currency_id = currency(&mut *conn, workspace_id, input.currency_id, &mut errors).await?;
    let contact = contact(&mut *conn, workspace_id, relation_id, input, &mut errors).await?;
    let refs = Refs::load(&mut *conn, workspace_id, &input.items, &mut errors).await?;
    errors.into_result()?;
    let billing = match &input.billing_address {
        Some(Some(a)) => Some(to_json(a)?),
        Some(None) => None,
        None => default_billing(addresses.as_deref())?,
    };
    Ok(CreateDefaults {
        relation_id,
        currency_id,
        billing,
        contact,
        refs,
    })
}

async fn share_input_defaults(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    input: &InvoiceInput,
    products: &[Uuid],
) -> Result<(), AppError> {
    let warehouses = unique(
        sent_fields(&input.items)
            .filter_map(|(_, f)| f.mutate_from_warehouse_id.flatten().flatten()),
    );
    super::approval_repo::share_defaults(
        conn,
        workspace_id,
        input.relation_id.unwrap_or_default(),
        input.addressed_contact_id.flatten().flatten(),
        products,
        &warehouses,
    )
    .await?;
    Ok(())
}

struct UpdateDefaults {
    relation_id: Uuid,
    billing: Option<Option<Value>>,
    contact: Option<Option<Value>>,
    lines: LinePlan,
}

async fn resolve_update(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    locked: &repo::Locked,
    input: &InvoiceInput,
    lock_facts: bool,
) -> Result<UpdateDefaults, AppError> {
    let existing: HashMap<Uuid, Option<Uuid>> = repo::item_products(conn, workspace_id, id)
        .await?
        .into_iter()
        .collect();
    if lock_facts {
        let products = unique(input.items.iter().filter_map(|row| match &row.kind {
            RowKind::New(f) => f.product_id.flatten(),
            RowKind::Update(id, _) => existing.get(id).copied().flatten(),
            RowKind::Delete(_) | RowKind::Skip => None,
        }));
        share_input_defaults(conn, workspace_id, input, &products).await?;
    }
    let mut errors = Errors::default();
    let relation_id = input.relation_id.unwrap_or_default();
    let addresses = relation(&mut *conn, workspace_id, relation_id, &mut errors).await?;
    let mut contact = contact(&mut *conn, workspace_id, relation_id, input, &mut errors).await?;
    let refs = Refs::load(&mut *conn, workspace_id, &input.items, &mut errors).await?;
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
    let lines = line_plan(&input.items, &refs, &existing)?;
    Ok(UpdateDefaults {
        relation_id,
        billing,
        contact,
        lines,
    })
}

/// Resolve the native draft patch under the same invoice and input-default locks.
pub(super) async fn review_update(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    input: &InvoiceInput,
) -> Result<Value, AppError> {
    let locked = draft(conn, workspace_id, id).await?;
    let defaults = resolve_update(conn, workspace_id, id, &locked, input, true).await?;
    Ok(update_quote(id, &defaults))
}

fn update_quote(id: Uuid, defaults: &UpdateDefaults) -> Value {
    let mut lines = quoted_new_lines(&defaults.lines.new);
    for line in &mut lines {
        line["change"] = serde_json::json!("new");
    }
    let changed = &defaults.lines.changed;
    let decimal = |value: Decimal| value.normalize().to_string();
    for i in 0..changed.id.len() {
        lines.push(serde_json::json!({"change":"update","id":changed.id[i],"variant_id":changed.product_variant_id[i],"set_variant":changed.product_variant_id[i].is_some(),"warehouse_id":changed.warehouse_id[i],"set_warehouse":changed.set_warehouse[i],"title":changed.title[i],"description":changed.description[i],"set_description":changed.set_description[i],"quantity":decimal(changed.quantity[i]),"price":changed.price[i].map(decimal),"set_price":changed.price[i].is_some(),"vat":decimal(changed.vat[i]),"discount":changed.discount[i].map(decimal),"set_discount":changed.discount[i].is_some(),"purchase_price":decimal(changed.purchase_price[i]),"position":changed.position[i],"set_position":changed.position[i].is_some()}));
    }
    for id in &defaults.lines.deleted {
        lines.push(serde_json::json!({"change":"delete","id":id}));
    }
    let mut full = serde_json::json!({"invoice_id":id,"relation_id":defaults.relation_id,"set_billing_address":defaults.billing.is_some(),"set_addressed_contact":defaults.contact.is_some(),"new_line_count":defaults.lines.new.title.len(),"changed_line_count":changed.id.len(),"deleted_line_count":defaults.lines.deleted.len(),"lines":lines});
    if let Some(billing) = &defaults.billing {
        full["billing_address"] = serde_json::json!(billing);
    }
    if let Some(contact) = &defaults.contact {
        full["addressed_contact"] = serde_json::json!(contact);
    }
    super::approval::bounded_review(full)
}

fn quoted_new_lines(new: &NewLines) -> Vec<Value> {
    let decimal = |value: Decimal| value.normalize().to_string();
    (0..new.title.len()).map(|i|serde_json::json!({"product_id":new.product_id[i],"variant_id":new.product_variant_id[i],"warehouse_id":new.warehouse_id[i],"title":new.title[i],"description":new.description[i],"quantity":decimal(new.quantity[i]),"price":decimal(new.price[i]),"vat":decimal(new.vat[i]),"discount":decimal(new.discount[i]),"purchase_price":decimal(new.purchase_price[i]),"position":new.position[i]})).collect()
}

/// One native new-line builder supplies both persisted values and approval pricing.
fn append_new_line(new: &mut NewLines, f: &ItemFields, index: usize, refs: &Refs) {
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
        .push(f.position.or(i32::try_from(index).ok()).unwrap_or(1));
    new.parent_item_id.push(None);
}

/// Resolve the same native header, contact, billing and line defaults without inserting.
pub(crate) async fn review_create(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    input: &InvoiceInput,
) -> Result<Value, AppError> {
    let defaults = resolve_create(conn, workspace_id, input, true).await?;
    create_quote(conn, workspace_id, input, &defaults).await
}

async fn create_quote(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    input: &InvoiceInput,
    defaults: &CreateDefaults,
) -> Result<Value, AppError> {
    let CreateDefaults {
        relation_id,
        currency_id,
        billing,
        contact,
        refs,
    } = defaults;
    let billing = billing.as_ref();
    let contact = contact.as_ref().and_then(Option::as_ref);
    let currency = products::approval_repo::currency(conn, workspace_id, *currency_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let decimals = currency["decimals"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| AppError::Internal("invoice currency decimals".into()))?;
    let mut new = NewLines::default();
    let mut errors = Errors::default();
    for row in &input.items {
        match &row.kind {
            RowKind::New(f) => append_new_line(&mut new, f, row.index, refs),
            RowKind::Update(_, _) => errors.invalid(&format!("items.{}.id", row.index)),
            RowKind::Delete(_) | RowKind::Skip => {}
        }
    }
    errors.into_result()?;
    let values: Vec<engine::Line> = (0..new.title.len())
        .map(|i| engine::Line {
            quantity: new.quantity[i],
            price: new.price[i],
            vat: new.vat[i],
            discount: new.discount[i],
            purchase_price: Some(new.purchase_price[i]),
            set_child: false,
        })
        .collect();
    let (summary, _) = price_values(&values, decimals)?;
    let lines = quoted_new_lines(&new);
    let full = serde_json::json!({"relation_id":relation_id,"currency":currency,"billing_address":billing,"addressed_contact":contact,"reference":input.reference.as_ref().and_then(Option::as_deref),"note":input.note.as_ref().and_then(Option::as_deref),"payment_method":input.payment_method.as_deref().unwrap_or("bank_transfer"),"is_credit":input.is_credit,"lines":lines,"total_minor":crate::money::minor(summary.total,decimals)?,"subtotal_minor":crate::money::minor(summary.excl,decimals)?});
    Ok(super::approval::bounded_review(full))
}

/// Totals and margin of `lines` at `dp`; 422 when a figure leaves Decimal's range.
pub(super) fn price(
    lines: &[repo::ItemRow],
    dp: u32,
) -> Result<(engine::Summary, engine::Margin), AppError> {
    let engine_lines: Vec<engine::Line> = lines.iter().map(engine_line).collect();
    price_values(&engine_lines, dp)
}

fn price_values(
    engine_lines: &[engine::Line],
    dp: u32,
) -> Result<(engine::Summary, engine::Margin), AppError> {
    let summary = engine::summarise(engine_lines, dp).map_err(|_| out_of_range())?;
    let margin = engine::margin(engine_lines, &summary.lines, dp).map_err(|_| out_of_range())?;
    Ok((summary, margin))
}

/// Recompute the totals from the stored lines and write them (C1: same transaction); the
/// total in minor units. The margin is computed too, so a line the detail can't price is
/// refused here.
pub(super) async fn store_totals(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    invoice_id: Uuid,
    currency_id: Uuid,
) -> Result<i64, AppError> {
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
    let total = minor(s.total)?;
    repo::set_totals(
        conn,
        workspace_id,
        invoice_id,
        minor(s.excl)?,
        total,
        &resource::stored_summary(&s, discounted, dp),
    )
    .await?;
    Ok(total)
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
    use crate::modules::pdf_templates;
    let id = parse(id)?;
    let template = match template_id {
        Some(t) => pdf_templates::repo::find(&state.db, workspace_id, t, Some("invoice")).await?,
        None => None,
    };
    let (file_id, _) = send::render(state, workspace_id, user_id, id, template.as_ref()).await?;
    files::service::get(state, workspace_id, &file_id.to_string()).await
}

/// GET /v1/invoices/fetch-from-hashed/{token}: a sent invoice by its payment token, for the
/// public payment page. 404 for anything else (no hint whether a token ever existed).
pub async fn public(state: &AppState, token: &str) -> Result<Value, AppError> {
    use crate::modules::{
        auth::token as secret,
        workspaces::{self, dto::Profile},
    };
    if !secret::looks_valid(token) {
        return Err(AppError::NotFound);
    }
    let (workspace_id, id) = repo::by_token(&state.db, &secret::hash(token))
        .await?
        .ok_or(AppError::NotFound)?;
    let row = repo::find(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = repo::lines(&state.db, workspace_id, id).await?;
    let dp = match resource::read_summary(&row.summary).2 {
        Some(dp) => dp,
        None => dp(repo::currency_decimals(&state.db, workspace_id, row.currency_id).await?)?,
    };
    let (summary, _) = price(&lines, dp)?;
    // Laravel ShortCurrencyResource.
    let currency = currencies::repo::find_many(&state.db, workspace_id, &[row.currency_id])
        .await?
        .pop()
        .map(|c| {
            serde_json::json!({ "name": c.name, "asset_type": c.asset_type,
                                "symbol_prefix": c.symbol_prefix, "symbol_suffix": c.symbol_suffix,
                                "decimals": c.decimals })
        })
        .unwrap_or(Value::Null);
    // A deleted workspace's links die with it.
    let workspace = workspaces::repo::record(&state.db, workspace_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut company = to_json(&Profile::from_stored(workspace.profile))?;
    // Bank details stay (bank transfer); the credit limit is internal.
    if let Some(account) = company.get_mut("account").and_then(Value::as_object_mut) {
        account.remove("credit_limit");
    }
    Ok(resource::public(
        &row, &lines, &summary, token, currency, company,
    ))
}
