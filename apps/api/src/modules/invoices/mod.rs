//! Invoices (Laravel Finance/Invoice): draft invoices and their item lines (create, update
//! with item diff, cancel, duplicate, list, detail) with totals stored per write; sending
//! (numbering, stock, PDF, mail, public link) and credit notes.

pub mod actions;
pub mod approval;
pub(crate) mod approval_repo;
pub mod credit;
pub mod delivery;
pub mod delivery_repo;
pub mod dto;
pub mod handlers;
pub mod pdf;
pub mod repo;
pub mod resource;
pub mod routes;
pub mod send;
pub mod service;
