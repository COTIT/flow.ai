//! Email profiles (Laravel WorkspaceSetting/EmailProfile): a sender (name, address, footer
//! HTML) with optionally its own SMTP server, whose password is sealed like the workspace
//! credentials.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
