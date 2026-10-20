//! Custom statuses (Laravel WorkspaceSetting/CustomStatus): per-domain status names
//! (deal pipelines). Archive/unarchive aren't ported: no client calls them.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
