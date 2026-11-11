//! The fake provider's hosted pay page, `/v1/payments/fake/{id}`: what a provider's checkout
//! page is to a real payment, for dev and staging. It exists only while the fake is the
//! configured provider, which a release build refuses at boot, so production never serves it.
//! Pay and Cancel set the fake's status and apply it at once through the same re-fetch a
//! webhook triggers (queued instead when that fails); the customer then goes back to the
//! shop's `redirect_url`, which the page also links.

use axum::{
    Form,
    extract::{Path, State},
    http::HeaderMap,
    response::{Html, Redirect},
};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    AppState,
    error::AppError,
    modules::orders::payments,
    payments::{Provider, Status, fake::PayPage},
};

/// The fake and its page for `id`, else 404.
fn fake_page(
    state: &AppState,
    id: &str,
) -> Result<(crate::payments::fake::Fake, PayPage), AppError> {
    let Some(Provider::Fake(fake)) = state.payments.provider("fake") else {
        return Err(AppError::NotFound);
    };
    let page = fake.page(id).ok_or(AppError::NotFound)?;
    Ok((fake.clone(), page))
}

/// GET /payments/fake/{id}: the pay page.
#[utoipa::path(
    get, path = "/payments/fake/{id}", tag = "payments",
    operation_id = "payments_fake_page", summary = "The test provider's pay page",
    description = "Dev and staging only: exists while `PAYMENTS_PROVIDER=fake`, which a release build refuses. Shows the amount with Pay and Cancel buttons and a link back to the shop.",
    params(("id" = String, Path, description = "The fake's payment id")),
    responses(
        (status = 200, content_type = "text/html", body = String, description = "The page"),
        (status = 404, description = "No fake provider, or unknown payment"),
    ),
)]
pub async fn page(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Html<String>, AppError> {
    let (_, p) = fake_page(&state, &id)?;
    let scale = 10_i64.pow(p.decimals);
    let amount = format!(
        "{}.{:0width$}",
        p.amount / scale,
        p.amount % scale,
        width = p.decimals as usize
    );
    let amount = amount.trim_end_matches('.');
    let open = p.status == Status::Open;
    let back = format!(
        r#"<p><a href="{}">Back to the shop</a></p>"#,
        attr(&p.redirect_url)
    );
    let buttons = if open {
        r#"<form method="post"><button name="outcome" value="paid">Pay</button>
<button name="outcome" value="canceled">Cancel</button></form>"#
    } else {
        "<p>This payment is no longer open.</p>"
    };
    // `id` is checked by `fake_page`; currency is the fake's own ISO code.
    Ok(Html(format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\">\
         <title>Test payment</title><h1>Test payment</h1>\
         <p>{amount} {currency}</p><p>No money moves: this is the test provider.</p>{buttons}{back}",
        currency = p.currency,
    )))
}

/// What the customer chose.
#[derive(Deserialize, ToSchema)]
pub struct PayChoice {
    /// `paid` or `canceled`.
    #[schema(value_type = String, example = "paid")]
    outcome: Option<String>,
}

/// POST /payments/fake/{id}: pay or cancel, then back to the shop.
#[utoipa::path(
    post, path = "/payments/fake/{id}", tag = "payments",
    operation_id = "payments_fake_decide", summary = "Pay or cancel on the test provider's page",
    description = "Dev and staging only. Sets the fake payment's status and applies it at once through the same re-fetch as its webhook (queued when that fails), then redirects (303) to the shop's `redirect_url`. A payment no longer open changes nothing.",
    params(("id" = String, Path, description = "The fake's payment id")),
    request_body(content = PayChoice, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 303, description = "Back to the shop's redirect_url"),
        (status = 404, description = "No fake provider, or unknown payment"),
        (status = 422, description = "`outcome` is not `paid` or `canceled`"),
    ),
)]
pub async fn decide(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(choice): Form<PayChoice>,
) -> Result<Redirect, AppError> {
    let (fake, p) = fake_page(&state, &id)?;
    let status = match choice.outcome.as_deref() {
        Some("paid") => Status::Paid,
        Some("canceled") => Status::Canceled,
        _ => {
            return Err(AppError::field(
                "outcome",
                "The outcome must be paid or canceled.",
            ));
        }
    };
    if p.status == Status::Open {
        fake.set_status(&id, status);
        // Applied now, so Pay right after Cancel finds no live payment to hand back; the
        // queued re-fetch is the fallback when that can't happen.
        if !payments::settle_now(&state, "fake", p.workspace_id, &id).await? {
            let ws = p.workspace_id.to_string();
            payments::receive(&state, "fake", &ws, &headers, format!("id={id}").as_bytes()).await?;
        }
    }
    Ok(Redirect::to(&p.redirect_url))
}

/// `v` safe inside a double-quoted HTML attribute.
fn attr(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
