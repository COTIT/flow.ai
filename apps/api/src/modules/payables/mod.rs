//! Payables (Flow ERP M5-A, masterplan §11/§13/§15): supplier bills matched to booked
//! receipts, supplier payments and refunds received, purchase returns, purchase cost
//! adjustments, and their typed reversals.

pub mod bills;
pub mod common;
pub mod costs;
pub mod dto;
pub mod handlers;
pub mod payments;
pub mod returns;
pub mod routes;
