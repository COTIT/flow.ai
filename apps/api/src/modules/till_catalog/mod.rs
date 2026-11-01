//! Till catalog: what a register sells, priced in its currency as `POST /till/sales` books it,
//! with its warehouse's stock, barcode lookup and delta sync.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
