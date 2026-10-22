//! `available-fields/{domain}`: Laravel PdfTemplateFields' `{fields, variables}` per domain,
//! generated from vimdesk-api by `tests/fixtures/pdf_template_fields_oracle.php` and served as
//! written (PHP's key order is the order vimdesk-web lists them in).

use std::{collections::HashMap, sync::OnceLock};

/// `{fields, variables}` of `domain` as JSON text, else None (Laravel answered `[]`/`[]`).
pub fn raw(domain: &str) -> Option<&'static str> {
    Some(match domain {
        "invoice" => include_str!("fields/invoice.json"),
        "estimate" => include_str!("fields/estimate.json"),
        "order" => include_str!("fields/order.json"),
        "purchase_order" => include_str!("fields/purchase_order.json"),
        "collective_invoice" => include_str!("fields/collective_invoice.json"),
        "work_order" => include_str!("fields/work_order.json"),
        "cash_register" => include_str!("fields/cash_register.json"),
        "relation" => include_str!("fields/relation.json"),
        _ => return None,
    })
}

/// The `variables` part of `raw(domain)` as written: email templates serve it in their own
/// envelope (it is the object's last key).
pub fn variables_raw(domain: &str) -> Option<&'static str> {
    let (_, rest) = raw(domain)?.trim_end().rsplit_once("\"variables\":")?;
    rest.strip_suffix('}')
}

/// Each domain's item field key -> its `type`, parsed once.
fn types() -> &'static HashMap<&'static str, HashMap<String, String>> {
    static TYPES: OnceLock<HashMap<&'static str, HashMap<String, String>>> = OnceLock::new();
    TYPES.get_or_init(|| {
        crate::modules::pdf_templates::dto::DOMAINS
            .iter()
            .map(|d| {
                let types = raw(d)
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
                    .and_then(|v| {
                        v["fields"].as_object().map(|o| {
                            o.iter()
                                .map(|(k, f)| {
                                    (k.clone(), f["type"].as_str().unwrap_or_default().to_owned())
                                })
                                .collect()
                        })
                    })
                    .unwrap_or_default();
                (*d, types)
            })
            .collect()
    })
}

/// Item field keys a column of `domain` may show.
pub fn keys(domain: &str) -> Vec<&'static str> {
    types()
        .get(domain)
        .map(|k| k.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

/// One field's `type` (`text`, `currency`, `image`, ...) for `domain`.
pub fn kind(domain: &str, key: &str) -> Option<&'static str> {
    types().get(domain)?.get(key).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_domain_parses_and_invoice_has_its_fields() {
        for d in crate::modules::pdf_templates::dto::DOMAINS {
            let v: serde_json::Value = serde_json::from_str(raw(d).unwrap()).unwrap();
            assert!(
                v.get("fields").is_some() && v.get("variables").is_some(),
                "{d}"
            );
        }
        let invoice = keys("invoice");
        assert_eq!(invoice.len(), 25);
        assert!(invoice.contains(&"product.product_nr") && invoice.contains(&"total"));
        assert!(keys("relation").is_empty());
        assert_eq!(kind("invoice", "price"), Some("currency"));
        assert!(raw("nope").is_none());
    }

    #[test]
    fn variables_slice_is_the_parsed_variables() {
        for d in crate::modules::pdf_templates::dto::DOMAINS {
            let whole: serde_json::Value = serde_json::from_str(raw(d).unwrap()).unwrap();
            let part: serde_json::Value = serde_json::from_str(variables_raw(d).unwrap()).unwrap();
            assert_eq!(part, whole["variables"], "{d}");
        }
    }
}
