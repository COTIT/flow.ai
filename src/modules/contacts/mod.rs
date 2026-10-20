//! CRM contacts (Laravel CRM/Contact): people at a relation. Reached only through their
//! relation (`/relations/{relation}/contacts`).

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
