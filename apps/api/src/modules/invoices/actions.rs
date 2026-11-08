//! What may happen to an invoice now. Each action route checks its rule here under the invoice
//! lock, and the detail's `allowed_actions` lists the ones that pass, so the two can't disagree.

use serde::Serialize;
use utoipa::ToSchema;

use crate::modules::invoices::repo::Locked;

/// An action Office can offer on an invoice (`allowed_actions`, in this order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceAction {
    /// `POST /invoices/{id}`.
    Edit,
    /// `POST /invoices/{id}/send`.
    Send,
    /// `POST /invoices/{id}/resend`.
    Resend,
    /// `DELETE /invoices/{id}`.
    Cancel,
    /// `POST /invoices/{id}/duplicate`.
    Duplicate,
    /// `POST /invoices/{id}/credit`.
    Credit,
    /// `POST /invoices/{id}/payments`.
    Pay,
    /// `POST /invoices/{id}/mark-as-credited`.
    MarkAsCredited,
}

impl InvoiceAction {
    /// Every action, in the order `allowed_actions` lists them.
    pub const ALL: [Self; 8] = [
        Self::Edit,
        Self::Send,
        Self::Resend,
        Self::Cancel,
        Self::Duplicate,
        Self::Credit,
        Self::Pay,
        Self::MarkAsCredited,
    ];
}

/// What the rules read from an invoice.
pub struct Facts<'a> {
    pub status: &'a str,
    pub is_credit: bool,
    pub numbered: bool,
    /// Its relation allows invoicing on account.
    pub on_account: bool,
}

impl<'a> Facts<'a> {
    /// The facts of a locked invoice. `on_account` matters to `Credit` only.
    pub fn of(l: &'a Locked, on_account: bool) -> Self {
        Self {
            status: &l.status,
            is_credit: l.is_credit,
            numbered: l.invoice_number.is_some(),
            on_account,
        }
    }
}

/// Statuses an invoice can still be paid in (the `Pay` rule; not for credit invoices).
pub const PAYABLE: [&str; 2] = ["pending", "partially_paid"];

/// Whether `action` passes on `f` now (status rules only, not permissions or bodies).
pub fn allows(action: InvoiceAction, f: &Facts) -> bool {
    use InvoiceAction::*;
    let draft = f.status == "draft";
    match action {
        Edit | Send | Cancel => draft,
        Resend => f.numbered,
        Duplicate => true,
        // Laravel canBeCredited; only a sent invoice (an unsent one has no number to credit).
        Credit => {
            !f.is_credit
                && f.numbered
                && if f.on_account {
                    f.status != "credited"
                } else {
                    matches!(f.status, "paid" | "partially_paid")
                }
        }
        Pay => !f.is_credit && PAYABLE.contains(&f.status),
        MarkAsCredited => f.is_credit && f.status == "pending",
    }
}

/// The actions that pass on `f`, in `InvoiceAction::ALL` order.
pub fn allowed(f: &Facts) -> Vec<InvoiceAction> {
    InvoiceAction::ALL
        .into_iter()
        .filter(|a| allows(*a, f))
        .collect()
}
