//! Purchasing (Flow ERP M3): purchase orders, receipts, booking into valued stock, landed
//! costs, and the pending-receipt guard the stock layer and sale availability share.

pub mod dto;
pub mod handlers;
pub mod receipts;
pub mod repo;
pub mod routes;
pub mod service;
