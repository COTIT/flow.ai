//! Odoo 19 JSON-2 source adapter (WF-01, C3a). Built from `docs/integrations/facts/odoo-19.md`
//! only: verified against the documented contract, never against a live Odoo (gate L1).

mod client;
mod errors;
mod probe;
mod source;
#[cfg(test)]
mod tests;

pub use client::Client;
