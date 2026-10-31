//! Farmer passbook (Phase 4b): append-only money lines per person (credits from sales,
//! debits from payouts) with a running balance per currency, and the passport consent flag.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
