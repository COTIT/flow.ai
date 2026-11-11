//! Your Flow scenes: a member's own display settings per Flow scene (order, hidden, which
//! components). Display config only, never business data; Rust owns the vocabulary and the
//! defaults. Contract: `docs/api/flow-scenes.md`.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
