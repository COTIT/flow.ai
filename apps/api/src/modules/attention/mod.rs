//! GET /attention: one read-only summary of what an office operator should act on today.
//! Each section reuses an existing rule; a section the caller can't list is null.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
