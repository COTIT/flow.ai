//! Odoo capability probe: each WF-01 read run once with `limit 1` and the real field lists, so a
//! missing module, field or access right shows up before the first poll.

use serde_json::{Value, json};

use super::{
    Client,
    source::{CONTROL_FIELDS, CURRENCY_FIELDS, LINE_FIELDS, ORDER_FIELDS, POLL_ORDER},
};
use crate::integrations::capability::{
    CapabilityCheck, ConnectionProbe, PURCHASE_REQUEST_LIST, PURCHASE_REQUEST_READ, ProviderError,
    SOURCE_CONTROLS_READ,
};

fn check(key: &str, result: Result<(), ProviderError>) -> CapabilityCheck {
    let err = result.err();
    CapabilityCheck {
        key: key.to_owned(),
        ok: err.is_none(),
        error_class: err.as_ref().map(|e| e.class),
        detail: err.map(|e| e.detail),
    }
}

impl Client {
    async fn one(&self, model: &str, fields: &[&str], extra: Value) -> Result<(), ProviderError> {
        let mut args = json!({"domain": [], "fields": fields, "limit": 1});
        if let (Some(a), Value::Object(e)) = (args.as_object_mut(), extra) {
            a.extend(e);
        }
        self.call(model, "search_read", args).await.map(drop)
    }
}

impl ConnectionProbe for Client {
    async fn test(&self) -> Vec<CapabilityCheck> {
        let list = self
            .one(
                "purchase.order",
                &["write_date"],
                json!({"order": POLL_ORDER}),
            )
            .await;
        let read = async {
            self.one("purchase.order", ORDER_FIELDS, json!({})).await?;
            self.one("purchase.order.line", LINE_FIELDS, json!({}))
                .await?;
            self.one("res.currency", CURRENCY_FIELDS, json!({})).await
        }
        .await;
        let controls = self.one("res.company", CONTROL_FIELDS, json!({})).await;
        vec![
            check(PURCHASE_REQUEST_LIST, list),
            check(PURCHASE_REQUEST_READ, read),
            check(SOURCE_CONTROLS_READ, controls),
        ]
    }
}
