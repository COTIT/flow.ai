//! Customer returns and credits (Flow ERP M4, masterplan §12 CUSTOMER CREDIT / RESTOCK /
//! CREDIT CORRECTIONS): credits of recognised sales, separate restock, typed corrections.

pub mod dto;
pub mod handlers;
pub mod legacy;
pub mod repo;
pub mod routes;
pub mod service;
