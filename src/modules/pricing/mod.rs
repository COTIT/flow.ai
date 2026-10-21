//! Item price calculator behind vimdesk-web and POS live totals (Laravel Shared/Item
//! `calculate-items`, `calculate-marge`). Computes only; stores nothing.

pub mod body;
pub mod dto;
pub mod engine;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
