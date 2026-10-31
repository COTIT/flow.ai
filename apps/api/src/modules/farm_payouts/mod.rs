//! Payout desk (G13, Phase 4b): staff pay a farmer cash from their passbook balance under the
//! voucher four-eyes rules, ops void with reversing rows, and the officers' cash ledger feeds
//! the daily reconciliation.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
