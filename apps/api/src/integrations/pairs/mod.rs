//! Source→target pairs running one workflow (WF-01): config, status, activation.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;

pub use service::{load_active, set_status};
