//! In-process provider fake: no HTTP, scriptable from tests. Selected only by
//! `INTEGRATIONS_PROVIDERS=fake` in debug builds; clones share one state. Unscripted
//! connections pass every capability.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::integrations::capability::{CapabilityCheck, keys_of};

/// Shared script: failed checks per connection id.
#[derive(Clone, Default, Debug)]
pub struct Fake {
    failures: Arc<Mutex<HashMap<String, Vec<CapabilityCheck>>>>,
}

impl Fake {
    /// Next probes of `connection_id` report `failed` (by key); other keys pass.
    pub fn script(&self, connection_id: &str, failed: Vec<CapabilityCheck>) {
        if let Ok(mut map) = self.failures.lock() {
            map.insert(connection_id.to_owned(), failed);
        }
    }

    /// Drop the script of `connection_id`: every check passes again.
    pub fn clear(&self, connection_id: &str) {
        if let Ok(mut map) = self.failures.lock() {
            map.remove(connection_id);
        }
    }

    /// Probe result for one connection.
    pub fn test(&self, connection_id: &str, provider: &str) -> Vec<CapabilityCheck> {
        let failed = self
            .failures
            .lock()
            .ok()
            .and_then(|m| m.get(connection_id).cloned())
            .unwrap_or_default();
        keys_of(provider)
            .iter()
            .map(|key| {
                failed
                    .iter()
                    .find(|c| c.key == *key)
                    .cloned()
                    .unwrap_or(CapabilityCheck {
                        key: (*key).to_owned(),
                        ok: true,
                        error_class: None,
                        detail: None,
                    })
            })
            .collect()
    }
}

/// A fake client bound to one connection.
#[derive(Clone, Debug)]
pub struct Client {
    pub fake: Fake,
    pub connection_id: String,
    pub provider: String,
}
