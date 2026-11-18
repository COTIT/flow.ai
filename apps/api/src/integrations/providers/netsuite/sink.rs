//! `PurchaseOrderSink` for NetSuite: externalId lookup (`q=` form, N2 l.195), create by POST with
//! `externalId` in the body (never PUT upsert, never PATCH), read-back normalised for WF-01 verify
//! (field names N1 l.133-181). Item sublist writability is UNVERIFIED (L3, l.104-107): the body
//! follows the documented sample.

use std::str::FromStr;

use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::{Client, RECORD_PATH, errors};
use crate::integrations::capability::{
    CreateOutcome, ErrorClass, NewPo, NewPoLine, ProviderError, PurchaseOrderSink, TargetPo,
};

fn unknown(detail: String) -> ProviderError {
    errors::err(ErrorClass::Unknown, detail, None, None)
}

/// External ids allow `[A-Za-z0-9_-]` (N2 l.194). Also keeps `q=` free of quotes.
fn valid_eid(eid: &str) -> bool {
    !eid.is_empty()
        && eid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Internal id of a created PO from `Location: …/record/v1/purchaseOrder/<digits>` (l.125).
pub fn created_id(location: &str) -> Option<String> {
    let id = location.split_once("/record/v1/purchaseOrder/")?.1;
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then(|| id.to_owned())
}

/// Exact JSON number (serde_json `arbitrary_precision` keeps the digits).
fn number(d: Decimal) -> Result<Value, ProviderError> {
    serde_json::Number::from_str(&d.to_string())
        .map(Value::Number)
        .map_err(|_| unknown(format!("{d} is not a JSON number")))
}

/// POST body (l.105, l.133-168). `units` is a string on the line (l.168); id vs name is UNVERIFIED.
pub fn payload(po: &NewPo) -> Result<Value, ProviderError> {
    let mut items = Vec::with_capacity(po.lines.len());
    for l in &po.lines {
        let mut line = json!({
            "item": {"id": l.item_id},
            "quantity": number(l.quantity)?,
            "rate": number(l.rate)?,
        });
        if let Some(unit) = &l.unit_id {
            line["units"] = json!(unit);
        }
        items.push(line);
    }
    let mut body = json!({
        "externalId": po.external_id,
        "entity": {"id": po.vendor_id},
        "subsidiary": {"id": po.subsidiary_id},
        "currency": {"id": po.currency_id},
        "item": {"items": items},
    });
    if let Some(loc) = &po.location_id {
        body["location"] = json!({"id": loc});
    }
    Ok(body)
}

/// Reference id: `{id, refName}` (l.131); the id may come as text or number.
fn ref_id(v: &Value) -> Option<String> {
    match &v["id"] {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn decimal(v: &Value) -> Option<Decimal> {
    let text = match v {
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    Decimal::from_str(&text)
        .or_else(|_| Decimal::from_scientific(&text))
        .ok()
}

/// Normalise a GET `purchaseOrder` reply (expanded sublists) for WF-01 verify.
pub fn normalise(id: &str, v: &Value) -> Result<TargetPo, ProviderError> {
    let need = |field: &str| {
        ref_id(&v[field]).ok_or_else(|| unknown(format!("Purchase order {id} has no {field}.")))
    };
    let mut lines = Vec::new();
    for (n, l) in v["item"]["items"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let bad = |f: &str| unknown(format!("Purchase order {id} line {n} has no {f}."));
        lines.push(NewPoLine {
            item_id: ref_id(&l["item"]).ok_or_else(|| bad("item"))?,
            unit_id: match &l["units"] {
                Value::String(s) => Some(s.clone()),
                other => ref_id(other),
            },
            quantity: decimal(&l["quantity"]).ok_or_else(|| bad("quantity"))?,
            rate: decimal(&l["rate"]).ok_or_else(|| bad("rate"))?,
        });
    }
    Ok(TargetPo {
        id: id.to_owned(),
        external_id: v["externalId"].as_str().map(str::to_owned),
        vendor_id: need("entity")?,
        subsidiary_id: need("subsidiary")?,
        currency_id: need("currency")?,
        lines,
        untaxed: decimal(&v["subtotal"]),
        tax: decimal(&v["taxTotal"]),
    })
}

impl PurchaseOrderSink for Client {
    /// PARTIAL (N2): the `q=externalId IS "…"` form; a miss (empty list or 404) is None, and None
    /// is never proof of absence (R3-§6c).
    async fn find_by_external_id(&self, eid: &str) -> Result<Option<TargetPo>, ProviderError> {
        if !valid_eid(eid) {
            return Err(errors::err(
                ErrorClass::Validation,
                "External id may only hold letters, digits, _ and -.",
                None,
                None,
            ));
        }
        let path = format!("{RECORD_PATH}/purchaseOrder?q=externalId%20IS%20%22{eid}%22");
        let found = match self.get_json(&path).await {
            Ok(v) => v,
            Err(e) if e.class == ErrorClass::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let hits: Vec<String> = found["items"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(ref_id)
            .collect();
        match hits.as_slice() {
            [] => Ok(None),
            [one] => self.read(one).await.map(Some),
            many => Err(errors::err(
                ErrorClass::Conflict,
                format!("{} purchase orders share external id {eid}.", many.len()),
                None,
                None,
            )),
        }
    }

    /// One POST, never retried here (at-most-once per attempt belongs to the caller). `Rejected`
    /// means NetSuite did not create it; `Ambiguous` means it may have.
    async fn create(&self, po: &NewPo) -> CreateOutcome {
        let body = match payload(po) {
            Ok(b) => b.to_string(),
            Err(e) => return CreateOutcome::Rejected(e),
        };
        let token = match self.token().await {
            Ok(t) => t,
            Err(e) => return CreateOutcome::Rejected(errors::for_read(e)),
        };
        let req = self
            .rest(
                reqwest::Method::POST,
                &format!("{RECORD_PATH}/purchaseOrder"),
                &token,
            )
            .header("Content-Type", "application/json")
            // Unknown field names fail instead of being dropped (N6 l.282).
            .header("X-NetSuite-PropertyNameValidation", "error")
            .body(body);
        let reply = match self.send(req).await {
            Ok(r) => r,
            Err(e) if e.class == ErrorClass::Ambiguous => return CreateOutcome::Ambiguous(e),
            Err(e) => return CreateOutcome::Rejected(e),
        };
        match reply.status {
            204 => {
                let location = reply
                    .headers
                    .get("location")
                    .and_then(|h| h.to_str().ok())
                    .map(str::to_owned);
                match location.as_deref().and_then(created_id) {
                    Some(id) => CreateOutcome::Created { id, location },
                    None => CreateOutcome::Ambiguous(errors::err(
                        ErrorClass::Ambiguous,
                        "NetSuite answered 204 without a readable purchase order Location.",
                        None,
                        Some(204),
                    )),
                }
            }
            409 => CreateOutcome::Conflict(errors::from_status(409, &reply.body)),
            400..=499 => CreateOutcome::Rejected(errors::from_status(reply.status, &reply.body)),
            // 5xx, 202 (async), other 2xx/3xx: may have committed.
            s => {
                let mut e = errors::from_status(s, &reply.body);
                e.class = ErrorClass::Ambiguous;
                CreateOutcome::Ambiguous(e)
            }
        }
    }

    /// GET with expanded sublists (l.116), normalised.
    async fn read(&self, id: &str) -> Result<TargetPo, ProviderError> {
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(errors::err(
                ErrorClass::Validation,
                "A NetSuite internal id is digits only.",
                None,
                None,
            ));
        }
        let v = self
            .get_json(&format!(
                "{RECORD_PATH}/purchaseOrder/{id}?expandSubResources=true"
            ))
            .await?;
        normalise(id, &v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_parsing() {
        assert_eq!(
            created_id("https://x/services/rest/record/v1/purchaseOrder/647").as_deref(),
            Some("647")
        );
        assert_eq!(created_id("/services/rest/record/v1/customer/647"), None);
        assert_eq!(created_id("https://x/record/v1/purchaseOrder/"), None);
        assert_eq!(created_id("https://x/record/v1/purchaseOrder/64a"), None);
    }

    #[test]
    fn eid_charset() {
        assert!(valid_eid("vz-0123abcd_X"));
        assert!(!valid_eid(""));
        assert!(!valid_eid("a\"b"));
        assert!(!valid_eid("a b"));
        assert!(!valid_eid("RED|PO"));
    }

    #[test]
    fn payload_keeps_exact_numbers() {
        let po = NewPo {
            external_id: "vz-1".into(),
            vendor_id: "11".into(),
            subsidiary_id: "1".into(),
            currency_id: "2".into(),
            location_id: None,
            lines: vec![NewPoLine {
                item_id: "99".into(),
                unit_id: None,
                quantity: Decimal::from_str("3.000").unwrap(),
                rate: Decimal::from_str("0.1234567890123456789").unwrap(),
            }],
        };
        let s = payload(&po).unwrap().to_string();
        assert!(s.contains(r#""rate":0.1234567890123456789"#), "{s}");
        assert!(s.contains(r#""quantity":3.000"#), "{s}");
        assert!(!s.contains("location") && !s.contains("units"));
    }
}
