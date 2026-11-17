//! Connections to customer systems: config, write-only secrets, probe and status.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod secrets;
pub mod service;

pub use service::{load, mark};
