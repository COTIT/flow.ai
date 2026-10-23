//! Invoice payments (Laravel Finance/Invoice payments): an append-only ledger per invoice, the
//! status its sum gives the invoice, settles against credit notes and mark-as-credited.

pub mod balance;
pub mod dto;
pub mod handlers;
pub mod repo;
pub mod resource;
pub mod routes;
pub mod service;
