//! Deal pipelines (Laravel CRM/DealPipeline): sales boards and their status columns. Pipeline
//! delete, duplicate and the single-status GET aren't ported: vimdesk-web never reaches them.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
