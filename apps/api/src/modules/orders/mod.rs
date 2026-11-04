//! Orders (office drafts and web shop checkouts): lines priced in minor units, a checked
//! status machine and the stock each status holds or takes (`stock::reserve`).

pub mod checkout;
pub mod dto;
pub mod fake_pay;
pub mod handlers;
pub mod payments;
pub mod repo;
pub mod routes;
pub mod service;
pub mod storefront;
pub mod trail;
pub mod webhook;
