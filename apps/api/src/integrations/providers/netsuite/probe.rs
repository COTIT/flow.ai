//! NetSuite connection probe: token fetch, then one read `GET purchaseOrder?limit=1` (l.119).
//! A probe never writes, so create is reported from that read with a note.

use super::{Client, RECORD_PATH};
use crate::integrations::capability::{
    AUTH_TOKEN, CapabilityCheck, ConnectionProbe, NETSUITE_CAPABILITIES, PURCHASE_ORDER_CREATE,
    ProviderError,
};

fn check(key: &str, result: &Result<(), ProviderError>) -> CapabilityCheck {
    CapabilityCheck {
        key: key.to_owned(),
        ok: result.is_ok(),
        error_class: result.as_ref().err().map(|e| e.class),
        detail: result.as_ref().err().map(|e| e.detail.clone()),
    }
}

impl ConnectionProbe for Client {
    async fn test(&self) -> Vec<CapabilityCheck> {
        let token = self.token().await.map(|_| ());
        let read = match &token {
            Ok(()) => self
                .get_json(&format!("{RECORD_PATH}/purchaseOrder?limit=1"))
                .await
                .map(|_| ()),
            Err(e) => Err(e.clone()),
        };
        NETSUITE_CAPABILITIES
            .iter()
            .map(|key| match *key {
                AUTH_TOKEN => check(key, &token),
                PURCHASE_ORDER_CREATE if read.is_ok() => CapabilityCheck {
                    detail: Some("Create is not tested: a probe never writes (LIVE L5).".into()),
                    ..check(key, &read)
                },
                _ => check(key, &read),
            })
            .collect()
    }
}
