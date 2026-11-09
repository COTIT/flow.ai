//! Invoice numbering and specific post-commit delivery. Intent commits with numbering;
//! request/worker share one durable claim. Interrupted attempts remain unknown until
//! send-log evidence resolves them. Lock order: invoice, currency, stock slots, counter.

use chrono::{DateTime, Days, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    AppState,
    error::AppError,
    modules::{
        auth::token,
        currencies, files,
        invoice_payments::{self, balance},
        invoices::{
            actions::{self, Facts, InvoiceAction},
            pdf, repo,
            service::{parse, store_totals},
        },
        messages::{
            dto::SendInput,
            service::{self as messages, Outgoing},
        },
        pdf_templates::{self, repo::TemplateRow},
        settings,
        stock::{
            repo::{self as stock, Cause, Floor, Mutated, Slot},
            reserve::{self, Taken, Want},
        },
    },
    money::round,
    numbering::{self, Appendix, Series},
};

const CAN_NOT_SEND: &str = "This Invoice does not have the correct status to be send.";
const CAN_NOT_RESEND: &str = "This Invoice has not been send yet.";
const NO_WAREHOUSE: &str = "No warehouse available";
const NO_CONTACT: &str = "The selected addressed contact id is invalid.";
const NO_TEMPLATE: &str = "The selected pdf template id is invalid.";
const NO_RECIPIENT: &str = "The to field is required when the contact has no email address.";
const TOO_LONG: &str = "The body is too long once its variables are filled in.";
const NUMBER_TAKEN: &str = "The invoice could not be numbered.";
/// Laravel `invoice.buttons.pay_for`.
const PAY_FOR: &str = "View and pay invoice";
/// Days to pay when neither the relation nor the settings say (Laravel).
const DEFAULT_TERM: u64 = 14;
/// Largest subject or body after `{invoice.*}` is filled in (values can be long notes).
const FILLED_MAX: usize = 2 * 1024 * 1024;

/// Which mail a delivery is (Laravel's `sent` / `resent` events).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Send,
    Resend,
}

impl Kind {
    fn message_type(self) -> &'static str {
        match self {
            Self::Send => "send_invoice",
            Self::Resend => "resend_invoice",
        }
    }
}

/// POST /invoices/{id}/send: number the draft, then render and mail it.
pub async fn send(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    input: SendInput,
) -> Result<super::delivery::Receipt, AppError> {
    let id = parse(id)?;
    ready(state, workspace_id, &input, Kind::Send).await?;
    let delivery = number_inner(state, workspace_id, user_id, id, &input, true)
        .await?
        .ok_or_else(|| AppError::Internal("missing delivery intent".into()))?;
    detached(state, workspace_id, delivery).await
}

/// Resend records its selected recipient and durable job in one transaction.
pub async fn resend(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: &str,
    mut input: SendInput,
) -> Result<super::delivery::Receipt, AppError> {
    let id = parse(id)?;
    ready(state, workspace_id, &input, Kind::Resend).await?;
    let mut tx = state.db.begin().await?;
    let locked = repo::lock(&mut tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    super::approval::check(&mut tx, workspace_id, id, Some(input.addressed_contact_id)).await?;
    if super::delivery_repo::pending(&mut *tx, workspace_id, id).await? {
        return Err(AppError::Conflict(
            "Previous invoice delivery is unresolved. Verify its send log before another send.",
        ));
    }
    if !actions::allows(InvoiceAction::Resend, &Facts::of(&locked, false)) {
        return Err(AppError::Unprocessable(CAN_NOT_RESEND.into()));
    }
    let (billing, contact) = recipient(&mut tx, workspace_id, locked.relation_id, &input).await?;
    freeze_recipient(&mut input, &contact);
    let token = if locked.has_token {
        None
    } else {
        new_token(state, workspace_id, id)?
    };
    repo::set_recipient(
        &mut tx,
        workspace_id,
        id,
        &billing,
        &contact,
        token.as_ref().map(Token::parts),
    )
    .await?;
    let delivery = enqueue(&mut tx, workspace_id, user_id, id, "resend", &input).await?;
    tx.commit().await?;
    detached(state, workspace_id, delivery).await
}

fn freeze_recipient(input: &mut SendInput, contact: &Value) {
    if !input.manual
        && input.to.is_empty()
        && let Some(email) = contact["email"].as_str()
    {
        input.to.push(email.trim().to_lowercase());
    }
}

async fn enqueue(
    conn: &mut PgConnection,
    workspace: Uuid,
    user: Uuid,
    id: Uuid,
    kind: &str,
    input: &SendInput,
) -> Result<Uuid, AppError> {
    let request = serde_json::to_value(input)
        .map_err(|_| AppError::Internal("invoice delivery input".into()))?;
    if request.to_string().len() > 3145728 {
        return Err(AppError::field(
            "body",
            "Invoice delivery input exceeds its storage limit.",
        ));
    }
    Ok(super::delivery_repo::insert(
        conn,
        workspace,
        id,
        user,
        kind,
        &request,
        crate::modules::assistant::preconditions::action_id(),
    )
    .await?)
}

/// Whether this server can render and store a PDF.
fn renders(state: &AppState) -> bool {
    state.pdf.is_some() && state.files.is_some()
}

/// What the PDF and the mail need, checked before anything is stored: the sidecar and the
/// store (not for a manual send), the PDF template, and (for an email) attachments, sender
/// and server. The invoice template; None for the built-in layout.
async fn ready(
    state: &AppState,
    workspace_id: Uuid,
    input: &SendInput,
    kind: Kind,
) -> Result<Option<TemplateRow>, AppError> {
    // A manual send needs no PDF, so it numbers without the sidecar or the store.
    if !(input.manual && kind == Kind::Send) {
        if state.pdf.is_none() {
            return Err(AppError::Unavailable(pdf::NO_RENDERER));
        }
        files::service::store(state)?;
    }
    let template = match input.pdf_template_id {
        Some(t) => Some(
            pdf_templates::repo::find(&state.db, workspace_id, t, Some("invoice"))
                .await?
                .ok_or_else(|| AppError::field("pdf_template_id", NO_TEMPLATE))?,
        ),
        None => None,
    };
    if !input.manual {
        messages::check(
            state,
            workspace_id,
            &input.attachments,
            input.email_profile_id,
            Some(kind.message_type()),
        )
        .await?;
    }
    Ok(template)
}

/// The numbering transaction. A second send of one draft waits on the row lock, then finds it
/// sent (422): no Laravel dedupe window needed. The demo seed calls it alone (no renderer there).
pub(crate) async fn number(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: Uuid,
    input: &SendInput,
) -> Result<(), AppError> {
    number_inner(state, workspace_id, user_id, id, input, false)
        .await
        .map(|_| ())
}

async fn number_inner(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: Uuid,
    input: &SendInput,
    queue: bool,
) -> Result<Option<Uuid>, AppError> {
    let mut tx = state.db.begin().await?;
    let locked = repo::lock(&mut tx, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    super::approval::check(&mut tx, workspace_id, id, Some(input.addressed_contact_id)).await?;
    if !actions::allows(InvoiceAction::Send, &Facts::of(&locked, false)) {
        return Err(AppError::Unprocessable(CAN_NOT_SEND.into()));
    }
    if super::delivery_repo::pending(&mut *tx, workspace_id, id).await? {
        return Err(AppError::Conflict(
            "Previous invoice delivery is unresolved. Verify its send log before another send.",
        ));
    }
    let (billing, contact) = recipient(&mut tx, workspace_id, locked.relation_id, input).await?;
    let total = priced(&mut tx, workspace_id, id, &locked).await?;
    move_stock(&mut tx, workspace_id, user_id, id).await?;
    let invoice = settings::service::section_or_default(&mut *tx, workspace_id, "invoice").await?;
    let credit;
    let own = if locked.is_credit {
        credit = settings::service::section_or_default(&mut *tx, workspace_id, "credit").await?;
        &credit
    } else {
        &invoice
    };
    let due_date = match locked.due_date {
        Some(_) => None,
        None => Some(
            due_date(
                &mut tx,
                workspace_id,
                locked.relation_id,
                locked.is_credit,
                own,
                &invoice,
            )
            .await?,
        ),
    };
    let token = if locked.has_token {
        None
    } else {
        new_token(state, workspace_id, id)?
    };
    // Laravel UpdateInvoiceBalanceAfterPayment on a zero total: paid, or credited. Payments
    // booked on the draft (an order paid online) count from the start.
    let (paid, cash) = invoice_payments::repo::sums(&mut tx, workspace_id, id).await?;
    let dp = currencies::repo::share_decimals(&mut tx, workspace_id, locked.currency_id)
        .await?
        .and_then(|d| u32::try_from(d).ok())
        .unwrap_or(2);
    let status = match (total, locked.is_credit) {
        (0, false) => "paid",
        (0, true) => "credited",
        _ => balance::status(
            "pending",
            locked.is_credit,
            balance::Balance {
                total,
                paid,
                cash,
                dp,
            },
        )
        .unwrap_or("pending"),
    };
    let appendix = Appendix::from_section(own).to_json();
    // Last: the counter row stays locked from here to commit only.
    let number = numbering::next(
        &mut tx,
        workspace_id,
        Series::Invoice,
        numbering::start_number(&invoice),
    )
    .await?;
    repo::mark_sent(
        &mut tx,
        workspace_id,
        id,
        &repo::Sent {
            number,
            appendix: &appendix,
            status,
            due_date,
            billing_address: &billing,
            addressed_contact: &contact,
            token: token.as_ref().map(Token::parts),
        },
    )
    .await
    .map_err(|e| taken(e, workspace_id))?;
    let delivery = if queue {
        let mut frozen = input.clone();
        freeze_recipient(&mut frozen, &contact);
        Some(enqueue(&mut tx, workspace_id, user_id, id, "send", &frozen).await?)
    } else {
        None
    };
    tx.commit().await.map_err(|e| taken(e, workspace_id))?;
    Ok(delivery)
}

/// The total in minor units at the currency's current decimals, share locking the currency so
/// a decimals edit waits for this send. A draft priced at other decimals is priced again.
async fn priced(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    id: Uuid,
    locked: &repo::Locked,
) -> Result<i64, AppError> {
    let decimals = currencies::repo::share_decimals(conn, workspace_id, locked.currency_id)
        .await?
        .ok_or_else(|| AppError::Internal("invoice currency missing".into()))?;
    if locked.dp == Some(i32::from(decimals)) {
        return Ok(locked.total);
    }
    store_totals(conn, workspace_id, id, locked.currency_id).await
}

/// A duplicate number means the counter fell behind the invoices (a bug, never a retry case;
/// docs/api/invoices.md has the repair): log it loudly, roll back, answer 503.
fn taken(e: sqlx::Error, workspace_id: Uuid) -> AppError {
    if e.as_database_error().and_then(|d| d.constraint()) == Some("invoices_number_key") {
        tracing::error!(workspace = %workspace_id, "invoice number taken: counter behind");
        return AppError::Unavailable(NUMBER_TAKEN);
    }
    e.into()
}

/// The billing address and addressed contact snapshots a send stores.
async fn recipient(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    relation_id: Uuid,
    input: &SendInput,
) -> Result<(Value, Value), AppError> {
    let contact = repo::addressed_contact(
        &mut *conn,
        workspace_id,
        relation_id,
        input.addressed_contact_id,
    )
    .await?
    .ok_or_else(|| AppError::field("addressed_contact_id", NO_CONTACT))?;
    let billing = input
        .address
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::Internal("validated send without an address".into()))?;
    let has_email = contact
        .get("email")
        .and_then(Value::as_str)
        .is_some_and(|e| !e.trim().is_empty());
    if !input.manual && input.to.is_empty() && !has_email {
        return Err(AppError::field("to", NO_RECIPIENT));
    }
    Ok((billing, contact))
}

/// Laravel SendInvoice::mutateStock: each stock-keeping line moves its quantity not yet moved
/// (a credit note's negative lines put it back). A store may go negative; another warehouse
/// may not, nor take what orders hold (422, nothing stored). The movement points at the invoice: its number comes later.
/// An order's invoice, and a credit note of one, move nothing: the order took the stock on
/// complete and puts it back through returns (`repo::stock_lines` leaves them out).
async fn move_stock(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
    id: Uuid,
) -> Result<(), AppError> {
    let lines = repo::stock_lines(conn, workspace_id, id).await?;
    if lines.is_empty() {
        return Ok(());
    }
    let mut slots = Vec::with_capacity(lines.len());
    for l in &lines {
        let warehouse_id = l
            .warehouse_id
            .ok_or_else(|| AppError::Unprocessable(NO_WAREHOUSE.into()))?;
        slots.push(Slot {
            product_id: l.product_id,
            warehouse_id,
            bin: None,
            lot_id: None,
        });
    }
    stock::lock_all(conn, workspace_id, &slots).await?;
    let cause = Cause {
        user_id: Some(user_id),
        related: Some(("invoice", id)),
        ..Cause::default()
    };
    for (l, slot) in lines.iter().zip(&slots) {
        let change = round(l.mutated_from_stock - l.quantity, 2);
        if change.is_zero() {
            continue;
        }
        let floor = if l.is_store == Some(true) {
            Floor::None
        } else {
            // Never stock an order holds: web orders are paid against their hold.
            Floor::Available
        };
        let short = |available: Decimal| {
            AppError::Unprocessable(format!(
                "Insufficient stock for product {} in {}. Available stock: {}. Required: {}",
                product_ref(l),
                l.warehouse_name.as_deref().unwrap_or_default(),
                available.normalize(),
                (-change).normalize(),
            ))
        };
        let out_of_range = || {
            AppError::Unprocessable(format!(
                "The stock level of {} in {} would be out of range.",
                product_ref(l),
                l.warehouse_name.as_deref().unwrap_or_default(),
            ))
        };
        // Credit notes put stock back on the no-lot row; sales take first-expired-first.
        if change > Decimal::ZERO {
            match stock::mutate(conn, workspace_id, *slot, change, floor, &cause).await? {
                Mutated::Done { .. } => {}
                Mutated::Short { level, .. } => return Err(short(level)),
                Mutated::OutOfRange => return Err(out_of_range()),
            }
            continue;
        }
        let want = Want {
            product_id: l.product_id,
            quantity: -change,
        };
        match reserve::take(
            conn,
            workspace_id,
            slot.warehouse_id,
            &[want],
            floor,
            &cause,
        )
        .await
        {
            Ok(Taken::Done) => {}
            Ok(Taken::Short { available, .. }) => return Err(short(available)),
            Err(reserve::Error::OutOfRange(_)) => return Err(out_of_range()),
            Err(e) => return Err(e.into()),
        }
    }
    let ids: Vec<Uuid> = lines.iter().map(|l| l.id).collect();
    repo::mark_mutated(conn, workspace_id, id, &ids).await?;
    Ok(())
}

/// Laravel's `reference_vendor - name`.
fn product_ref(l: &repo::StockLine) -> String {
    match l.reference_vendor.as_deref().filter(|r| !r.is_empty()) {
        Some(r) => format!("{r} - {}", l.product_name),
        None => l.product_name.clone(),
    }
}

/// Laravel's due date: the relation's term (credit term for credit notes), else the section's
/// default for its type (business: `_company`), else the invoice default, else 14 days; at
/// midnight UTC of that day.
async fn due_date(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    relation_id: Uuid,
    is_credit: bool,
    own: &Value,
    invoice: &Value,
) -> Result<DateTime<Utc>, AppError> {
    let terms = repo::relation_terms(&mut *conn, workspace_id, relation_id).await?;
    let business = terms.as_ref().is_some_and(|t| t.kind == "business");
    let key = if is_credit {
        "credit_invoice_payment_term"
    } else {
        "invoice_payment_term"
    };
    let default = if business {
        "default_payment_term_company"
    } else {
        "default_payment_term"
    };
    let days = terms
        .as_ref()
        .and_then(|t| t.terms_and_periods.as_ref())
        .and_then(|t| days(t.get(key)))
        .or_else(|| days(own.get(default)))
        .or_else(|| days(invoice.get("default_payment_term")))
        .unwrap_or(DEFAULT_TERM);
    Utc::now()
        .date_naive()
        .checked_add_days(Days::new(days))
        .map(|d| d.and_time(NaiveTime::MIN).and_utc())
        .ok_or_else(|| AppError::Internal("due date out of range".into()))
}

/// A whole number of days (number or numeric text) in 1..=36500; PHP's falsy 0 and anything
/// else fall through to the next source.
fn days(v: Option<&Value>) -> Option<u64> {
    let n = match v? {
        Value::Number(n) => n.as_u64()?,
        Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    (1..=36_500).contains(&n).then_some(n)
}

/// A new payment token: its sha256 (the lookup) and a sealed copy (printed again later).
struct Token {
    hash: [u8; 32],
    sealed: Vec<u8>,
    key_id: String,
}

impl Token {
    fn parts(&self) -> (&[u8], &[u8], &str) {
        (&self.hash, &self.sealed, &self.key_id)
    }
}

/// None without a credential key: the invoice then has no public link.
fn new_token(state: &AppState, workspace_id: Uuid, id: Uuid) -> Result<Option<Token>, AppError> {
    let Some(key) = &state.credentials else {
        tracing::warn!(invoice = %id, "no credential key: invoice sent without a payment link");
        return Ok(None);
    };
    let (plain, hash) = token::generate()?;
    Ok(Some(Token {
        hash,
        sealed: key.seal(workspace_id, &token_path(id), plain.expose())?,
        key_id: key.id().to_owned(),
    }))
}

fn token_path(id: Uuid) -> String {
    format!("invoices.{id}.payment_token")
}

/// The invoice's payment token, opened; None without one, or when the key is missing or not
/// the one it was sealed with.
pub fn payment_token(
    state: &AppState,
    workspace_id: Uuid,
    row: &repo::InvoiceRow,
) -> Option<String> {
    let (sealed, key_id) = (
        row.payment_token.as_ref()?,
        row.payment_token_key_id.as_ref()?,
    );
    let key = state.credentials.as_ref()?;
    if key.id() != key_id {
        tracing::warn!(invoice = %row.id, "payment token sealed with another key");
        return None;
    }
    match key.open(workspace_id, &token_path(row.id), sealed) {
        Ok(plain) => Some(plain.expose().to_owned()),
        Err(e) => {
            tracing::warn!(invoice = %row.id, error = %e, "payment token doesn't open");
            None
        }
    }
}

/// The SPA's public invoice page for `token`.
pub fn payment_link(state: &AppState, token: &str) -> String {
    format!("{}/clients/pay-for-invoice/{token}", state.frontend_url)
}

/// Request disconnect cannot cancel committed specific delivery attempt.
async fn detached(
    state: &AppState,
    workspace: Uuid,
    delivery: Uuid,
) -> Result<super::delivery::Receipt, AppError> {
    let state = state.clone();
    tokio::spawn(async move { super::delivery::attempt(&state, workspace, delivery).await })
        .await
        .map_err(|e| AppError::Internal(format!("invoice delivery task: {e}")))?
}

/// After commit: the PDF (a send renders; a resend renders only with a template or when the
/// stored one is gone, as Laravel's InvoiceSend listener), then the mail unless `manual`.
#[allow(clippy::too_many_arguments)]
pub(super) async fn deliver_reserved(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: Uuid,
    input: SendInput,
    kind: &str,
    delivery: Uuid,
    reserved_pdf: Uuid,
    action: Option<Uuid>,
) -> Result<crate::modules::messages::dto::SendReceipt, AppError> {
    let kind = if kind == "resend" {
        Kind::Resend
    } else {
        Kind::Send
    };
    let template = ready(state, workspace_id, &input, kind).await?;
    if input.manual && !renders(state) {
        return Ok(crate::modules::messages::dto::SendReceipt {
            status: crate::modules::messages::dto::SendStatus::Manual,
            send_log_id: None,
        });
    }
    let row = repo::find(&state.db, workspace_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let stored = match row.pdf_file_id {
        Some(file) if kind == Kind::Resend && template.is_none() => {
            match files::service::get(state, workspace_id, &file.to_string()).await {
                Ok(_) => Some(file),
                Err(AppError::NotFound) => None,
                Err(e) => return Err(e),
            }
        }
        _ => None,
    };
    let (pdf_file, vars) = match stored {
        Some(file) => (file, None),
        None => {
            let pdf = pdf::render(state, workspace_id, id, template.as_ref()).await?;
            let vars = pdf.variables;
            let file = files::service::create_reserved(
                state,
                workspace_id,
                user_id,
                reserved_pdf,
                files::service::NewFile {
                    document_name: &pdf.file_name,
                    description: None,
                    meta: None,
                    bytes: pdf.bytes,
                    expires_at: None,
                },
            )
            .await?;
            repo::set_pdf(&state.db, workspace_id, id, file).await?;
            (file, Some(vars))
        }
    };
    if input.manual {
        return Ok(crate::modules::messages::dto::SendReceipt {
            status: crate::modules::messages::dto::SendStatus::Manual,
            send_log_id: None,
        });
    }
    let vars = match vars {
        Some(v) => v,
        None => pdf::mail_variables(state, workspace_id, id).await?,
    };
    // Laravel mailed only the first `to`; every one gets it here. None: the addressed contact.
    let to = if input.to.is_empty() {
        row.addressed_contact
            .as_ref()
            .and_then(|c| c.get("email"))
            .and_then(Value::as_str)
            .map(|e| vec![e.trim().to_lowercase()])
            .unwrap_or_default()
    } else {
        input.to
    };
    let mut attachments = input.attachments;
    attachments.push(pdf_file);
    // A pay button while something is to pay (Laravel put one on every mail, credit notes too).
    let open = !row.is_credit && !matches!(row.status.as_str(), "paid" | "cancelled" | "credited");
    let link = open
        .then(|| payment_token(state, workspace_id, &row))
        .flatten()
        .map(|t| payment_link(state, &t));
    let too_long = || AppError::field("body", TOO_LONG);
    let subject = substitute(&input.subject, &vars, false).ok_or_else(too_long)?;
    let body = substitute(&input.body, &vars, true).ok_or_else(too_long)?;
    messages::send_reserved(
        state,
        workspace_id,
        user_id,
        Outgoing {
            to,
            cc: input.cc,
            bcc: input.bcc,
            subject: &subject,
            body: &body,
            attachments: &attachments,
            email_profile_id: input.email_profile_id,
            message_type: Some(kind.message_type()),
            entity: Some(("invoice", id)),
            button: link.as_deref().map(|l| (PAY_FOR, l)),
        },
        delivery,
        action,
    )
    .await
}

/// Render invoice `id` with `template` (an invoice template), else the built-in layout
/// (Laravel CreateInvoicePdf), and store it as a file: its id and the invoice's variables.
pub(super) async fn render(
    state: &AppState,
    workspace_id: Uuid,
    user_id: Uuid,
    id: Uuid,
    template: Option<&TemplateRow>,
) -> Result<(Uuid, Vec<(String, String)>), AppError> {
    files::service::store(state)?;
    let pdf = pdf::render(state, workspace_id, id, template).await?;
    let file = files::service::create(
        state,
        workspace_id,
        user_id,
        files::service::NewFile {
            document_name: &pdf.file_name,
            description: None,
            meta: None,
            bytes: pdf.bytes,
            expires_at: None,
        },
    )
    .await?;
    Ok((file, pdf.variables))
}

/// Laravel VariableHelper::convertVariables in one left-to-right pass: `{key}` of a known
/// variable becomes its value (HTML-escaped in a body, except the already-HTML
/// `customer_references`); any other `{…}` stays as typed. A value is never expanded again.
/// Linear: a key's `}` is only looked for up to the next `{`. None past `FILLED_MAX`.
fn substitute(text: &str, vars: &[(String, String)], html: bool) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let found = after
            .find(['{', '}'])
            .filter(|&end| after.as_bytes()[end] == b'}')
            .and_then(|end| {
                let key = &after[..end];
                vars.iter()
                    .find(|(k, _)| k == key)
                    .map(|(k, v)| (end, k, v))
            });
        match found {
            Some((end, k, v)) => {
                if html && k != "invoice.customer_references" {
                    out.push_str(&pdf::escape(v));
                } else {
                    out.push_str(v);
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
        if out.len() > FILLED_MAX {
            return None;
        }
    }
    out.push_str(rest);
    (out.len() <= FILLED_MAX).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vars() -> Vec<(String, String)> {
        vec![
            ("invoice.invoice_nr".into(), "INV-1".into()),
            ("invoice.relation.company_name".into(), "A & B <i>".into()),
            (
                "invoice.customer_references".into(),
                "<br/>PO: 1<br/>".into(),
            ),
            ("invoice.note".into(), "{invoice.invoice_nr}".into()),
        ]
    }

    #[test]
    fn substitute_fills_known_keys_once_and_keeps_the_rest() {
        let v = vars();
        let fill = |t: &str, html: bool| substitute(t, &v, html).unwrap();
        assert_eq!(
            fill(
                "{invoice.invoice_nr} for {invoice.relation.company_name} {x} {",
                true
            ),
            "INV-1 for A &amp; B &lt;i&gt; {x} {"
        );
        assert_eq!(
            fill("{invoice.relation.company_name}", false),
            "A & B <i>",
            "subjects aren't HTML"
        );
        assert_eq!(
            fill("{invoice.customer_references}", true),
            "<br/>PO: 1<br/>"
        );
        assert_eq!(
            fill("{invoice.note}", true),
            "{invoice.invoice_nr}",
            "a value is not expanded again"
        );
        assert_eq!(fill("{{invoice.invoice_nr}}", true), "{INV-1}");
        assert_eq!(fill("{a {invoice.invoice_nr}", true), "{a INV-1");
    }

    #[test]
    fn substitute_is_linear_and_capped() {
        let v = vars();
        let braces = "{".repeat(1 << 20);
        let started = std::time::Instant::now();
        assert_eq!(
            substitute(&braces, &v, true).as_deref(),
            Some(braces.as_str())
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        let long = vec![("invoice.note".to_owned(), "x".repeat(1000))];
        let many = "{invoice.note}".repeat(FILLED_MAX / 1000 + 1);
        assert_eq!(substitute(&many, &long, true), None);
    }

    #[test]
    fn days_takes_positive_whole_numbers_only() {
        assert_eq!(days(Some(&json!(30))), Some(30));
        assert_eq!(days(Some(&json!(" 7 "))), Some(7));
        for junk in [
            json!(0),
            json!(-3),
            json!(2.5),
            json!("x"),
            json!(null),
            json!(40_000),
        ] {
            assert_eq!(days(Some(&junk)), None, "{junk}");
        }
        assert_eq!(days(None), None);
    }
}
