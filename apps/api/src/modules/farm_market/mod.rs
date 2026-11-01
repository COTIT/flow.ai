//! Farm market (RUAHA `buyer`, `buyer_demand`, `opportunity`): the ops order book and the
//! village opportunities proposed against it. An opportunity is never a sale.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod rules;
pub mod service;
