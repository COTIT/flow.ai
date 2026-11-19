//! `PurchaseRequestSource` for Odoo: RFQ cursor poll (facts O1), full-scan ids, batch read into
//! neutral DTOs with exact decimals (O2, O6 display lines dropped; C6-O1 down-payment lines kept and
//! flagged) and the O5 approval controls.
//! `picking_type_id` (O4) is never requested; WF-01 takes the location from the pair config.
//! No money checks here (O6 → C4a/C4b).

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    str::FromStr,
};

use chrono::{DateTime, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::{Client, errors::unreadable};
use crate::integrations::capability::{
    Cursor, Page, ProviderError, PurchaseRequest, PurchaseRequestSource, RequestLine,
    SourceControls, SourceRef, SourceState,
};

/// `fields.Datetime.to_string` format: whole seconds, UTC (L95).
const TS: &str = "%Y-%m-%d %H:%M:%S";
/// Poll order: write_date then id, so equal stamps page by id (PLAN §6a).
pub(super) const POLL_ORDER: &str = "write_date asc, id asc";
/// RFQ states WF-01 takes (PLAN §1b).
const ELIGIBLE: [&str; 2] = ["draft", "sent"];
pub(super) const ORDER_FIELDS: &[&str] = &[
    "state",
    "company_id",
    "partner_id",
    "currency_id",
    "user_id",
    "date_order",
    "amount_untaxed",
    "amount_tax",
    "amount_total",
];
pub(super) const LINE_FIELDS: &[&str] = &[
    "order_id",
    "display_type",
    "is_downpayment",
    "product_id",
    "product_qty",
    "product_uom_id",
    "price_unit",
    "price_subtotal",
    "tax_ids",
    "date_planned",
];
pub(super) const CURRENCY_FIELDS: &[&str] = &["name", "decimal_places"];
pub(super) const CONTROL_FIELDS: &[&str] = &["po_double_validation", "po_double_validation_amount"];

/// Numeric Odoo ids of `ids`; non-numeric ones can't exist.
fn int_ids(ids: &[String]) -> Vec<i64> {
    ids.iter().filter_map(|i| i.parse().ok()).collect()
}

fn rows(v: Value) -> Result<Vec<Value>, ProviderError> {
    match v {
        Value::Array(a) => Ok(a),
        _ => Err(unreadable("expected a list")),
    }
}

fn id_of(row: &Value) -> Result<i64, ProviderError> {
    row.get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| unreadable("record without id"))
}

/// Exact decimal from the arbitrary-precision JSON number text; never through f64.
fn dec(row: &Value, field: &str) -> Result<Decimal, ProviderError> {
    let text = match row.get(field) {
        Some(Value::Number(n)) => n.to_string(),
        _ => return Err(unreadable(field)),
    };
    Decimal::from_str(&text)
        .or_else(|_| Decimal::from_scientific(&text))
        .map_err(|_| unreadable(field))
}

/// Many2one: `[id, name]`, bare `id`, or `false` (shape UNVERIFIED item 4: both accepted).
fn m2o(row: &Value, field: &str) -> Option<(String, Option<String>)> {
    match row.get(field)? {
        Value::Array(pair) => Some((
            pair.first()?.as_i64()?.to_string(),
            pair.get(1).and_then(Value::as_str).map(str::to_owned),
        )),
        Value::Number(n) => Some((n.as_i64()?.to_string(), None)),
        _ => None,
    }
}

/// Required many2one id; its name (when given) goes to `labels` as `<kind>:<id>`.
fn m2o_req(
    row: &Value,
    field: &str,
    kind: &str,
    labels: &mut BTreeMap<String, String>,
) -> Result<String, ProviderError> {
    let (id, name) = m2o(row, field).ok_or_else(|| unreadable(field))?;
    if let Some(name) = name {
        labels.insert(format!("{kind}:{id}"), name);
    }
    Ok(id)
}

/// Text field; Odoo sends `false` for empty.
fn text(row: &Value, field: &str) -> Option<String> {
    row.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn ts(row: &Value) -> Result<DateTime<Utc>, ProviderError> {
    text(row, "write_date")
        .and_then(|s| NaiveDateTime::parse_from_str(&s, TS).ok())
        .map(|n| n.and_utc())
        .ok_or_else(|| unreadable("write_date"))
}

fn state(s: &str) -> SourceState {
    match s {
        "draft" => SourceState::Draft,
        "sent" => SourceState::Sent,
        "to approve" => SourceState::ToApprove,
        "purchase" => SourceState::Confirmed,
        "cancel" => SourceState::Cancelled,
        other => SourceState::Other(other.to_owned()),
    }
}

fn line(row: &Value, labels: &mut BTreeMap<String, String>) -> Result<RequestLine, ProviderError> {
    let tax_ids = match row.get("tax_ids") {
        Some(Value::Array(t)) => t
            .iter()
            .map(|t| t.as_i64().map(|i| i.to_string()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| unreadable("tax_ids"))?,
        _ => return Err(unreadable("tax_ids")),
    };
    // Down-payment lines may have no product or uom (purchase_order_line.py#L105-L107); kept, never skipped.
    let downpayment = row.get("is_downpayment").and_then(Value::as_bool) == Some(true);
    let mut m2o_line = |field: &str, kind: &str| {
        if downpayment && m2o(row, field).is_none() {
            Ok(String::new())
        } else {
            m2o_req(row, field, kind, labels)
        }
    };
    Ok(RequestLine {
        id: id_of(row)?.to_string(),
        product_id: m2o_line("product_id", "product")?,
        uom_id: m2o_line("product_uom_id", "uom")?,
        quantity: dec(row, "product_qty")?,
        price_unit: dec(row, "price_unit")?,
        subtotal: dec(row, "price_subtotal")?,
        tax_ids,
        date_planned: text(row, "date_planned"),
        downpayment,
    })
}

impl Client {
    async fn search_read(&self, model: &str, args: Value) -> Result<Vec<Value>, ProviderError> {
        rows(self.call(model, "search_read", args).await?)
    }
}

impl PurchaseRequestSource for Client {
    async fn list_changed(
        &self,
        company_ids: &[String],
        cursor: &Cursor,
        limit: u32,
    ) -> Result<Page<SourceRef>, ProviderError> {
        let at = cursor.ts.format(TS).to_string();
        let found = self
            .search_read(
                "purchase.order",
                json!({
                    "domain": [
                        ["company_id", "in", int_ids(company_ids)],
                        ["state", "in", ELIGIBLE],
                        "|", ["write_date", ">", at],
                        "&", ["write_date", "=", at], ["id", ">", cursor.id]
                    ],
                    "fields": ["write_date"],
                    "order": POLL_ORDER,
                    "limit": limit
                }),
            )
            .await?;
        let mut items = Vec::with_capacity(found.len());
        let mut last = None;
        for row in &found {
            let (id, changed_at) = (id_of(row)?, ts(row)?);
            last = Some(Cursor { ts: changed_at, id });
            items.push(SourceRef {
                id: id.to_string(),
                changed_at,
            });
        }
        let full = items.len() >= limit as usize;
        Ok(Page {
            items,
            next: last.filter(|_| full),
        })
    }

    async fn list_eligible_ids(
        &self,
        company_id: &str,
        after_id: i64,
        limit: u32,
    ) -> Result<Vec<String>, ProviderError> {
        let company: i64 = company_id
            .parse()
            .map_err(|_| unreadable("company id is not numeric"))?;
        let ids = self
            .call(
                "purchase.order",
                "search",
                json!({
                    "domain": [
                        ["company_id", "=", company],
                        ["state", "in", ELIGIBLE],
                        ["id", ">", after_id]
                    ],
                    "order": "id asc",
                    "limit": limit
                }),
            )
            .await?;
        rows(ids)?
            .iter()
            .map(|i| i.as_i64().map(|i| i.to_string()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| unreadable("id list"))
    }

    async fn read(
        &self,
        ids: &[String],
    ) -> Result<(Vec<PurchaseRequest>, Vec<String>), ProviderError> {
        if ids.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        // search_read, not read: a deleted id is absent instead of a MissingError (L67-70).
        let orders = self
            .search_read(
                "purchase.order",
                json!({"domain": [["id", "in", int_ids(ids)]], "fields": ORDER_FIELDS}),
            )
            .await?;
        let found: Vec<i64> = orders.iter().map(id_of).collect::<Result<_, _>>()?;
        let gone: Vec<String> = ids
            .iter()
            .filter(|i| !i.parse().is_ok_and(|n: i64| found.contains(&n)))
            .cloned()
            .collect();
        if found.is_empty() {
            return Ok((Vec::new(), gone));
        }

        let lines = self
            .search_read(
                "purchase.order.line",
                json!({
                    "domain": [["order_id", "in", found]],
                    "fields": LINE_FIELDS,
                    "order": "id asc"
                }),
            )
            .await?;
        let currency_ids: BTreeSet<i64> = orders
            .iter()
            .filter_map(|o| m2o(o, "currency_id")?.0.parse().ok())
            .collect();
        // Archived currencies stay readable for old RFQs.
        let currencies = self
            .search_read(
                "res.currency",
                json!({
                    "domain": [["id", "in", currency_ids]],
                    "fields": CURRENCY_FIELDS,
                    "context": {"active_test": false}
                }),
            )
            .await?;
        let mut currency: HashMap<String, (String, u8)> = HashMap::new();
        for c in &currencies {
            let places = c
                .get("decimal_places")
                .and_then(Value::as_u64)
                .and_then(|p| u8::try_from(p).ok())
                .ok_or_else(|| unreadable("decimal_places"))?;
            let name = text(c, "name").ok_or_else(|| unreadable("currency name"))?;
            currency.insert(id_of(c)?.to_string(), (name, places));
        }

        let mut by_order: HashMap<String, Vec<&Value>> = HashMap::new();
        for l in &lines {
            // Section and note lines carry a display_type; they are not order lines (L229).
            if l.get("display_type").is_some_and(|d| d.as_str().is_some()) {
                continue;
            }
            let order = m2o(l, "order_id").ok_or_else(|| unreadable("order_id"))?.0;
            by_order.entry(order).or_default().push(l);
        }

        let mut out = Vec::with_capacity(orders.len());
        for o in &orders {
            let id = id_of(o)?.to_string();
            let mut labels = BTreeMap::new();
            let currency_id = m2o_req(o, "currency_id", "currency", &mut labels)?;
            let (currency_ref, currency_decimals) = currency
                .get(&currency_id)
                .cloned()
                .ok_or_else(|| unreadable("currency not returned"))?;
            let mut req_lines = Vec::new();
            for l in by_order.get(&id).into_iter().flatten() {
                req_lines.push(line(l, &mut labels)?);
            }
            out.push(PurchaseRequest {
                state: state(&text(o, "state").ok_or_else(|| unreadable("state"))?),
                company_id: m2o_req(o, "company_id", "company", &mut labels)?,
                vendor_id: m2o_req(o, "partner_id", "vendor", &mut labels)?,
                buyer_id: m2o(o, "user_id").map(|(uid, name)| {
                    if let Some(name) = name {
                        labels.insert(format!("buyer:{uid}"), name);
                    }
                    uid
                }),
                location_id: None,
                date_order: text(o, "date_order"),
                amount_untaxed: dec(o, "amount_untaxed")?,
                amount_tax: dec(o, "amount_tax")?,
                amount_total: dec(o, "amount_total")?,
                lines: req_lines,
                currency_id,
                currency_ref,
                currency_decimals,
                labels,
                id,
            });
        }
        Ok((out, gone))
    }

    async fn read_controls(
        &self,
        company_ids: &[String],
    ) -> Result<Vec<SourceControls>, ProviderError> {
        if company_ids.is_empty() {
            return Ok(Vec::new());
        }
        // Companies hidden by record rules (L166) are left out; C4a treats them as unmet.
        let found = self
            .search_read(
                "res.company",
                json!({
                    "domain": [["id", "in", int_ids(company_ids)]],
                    "fields": CONTROL_FIELDS,
                    "context": {"active_test": false}
                }),
            )
            .await?;
        found
            .iter()
            .map(|c| {
                Ok(SourceControls {
                    company_id: id_of(c)?.to_string(),
                    two_step: text(c, "po_double_validation").as_deref() == Some("two_step"),
                    threshold_zero: dec(c, "po_double_validation_amount")?.is_zero(),
                })
            })
            .collect()
    }
}
