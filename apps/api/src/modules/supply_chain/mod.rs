//! Supply chain moves of supply lots (Phase 4b): packing a supply lot into a child lot of
//! jars, and transferring lot stock over a live supply link into the receiver's own lot.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
