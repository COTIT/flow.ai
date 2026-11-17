//! Partial shipments of sales orders (Flow ERP M4, masterplan §12): draft planning capacity,
//! reviewed post consuming exactly the shipped stock, sale recognition in active books.

pub mod dto;
pub mod handlers;
pub mod recognition;
pub mod repo;
pub mod routes;
pub mod service;
