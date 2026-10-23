//! Deals (Laravel CRM/Deal): cards on a sales board, kanban drags as gap ranks. Archive,
//! un-archive, the V2 list and exports aren't ported (no vimdesk-web caller, or left out by
//! user decision 2026-10-23).

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
