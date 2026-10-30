//! Special offers (Laravel ERP/SpecialOffer): promotional prices per product, plus the
//! advanced rule fields Laravel stored for its cart rules. A product's active offer sets its
//! price through `products::pricing::special`.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
