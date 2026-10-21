//! Workspace settings (Laravel WorkspaceSetting/Setting core): one jsonb row per workspace,
//! Laravel's sections, defaults and rules. Integration credentials are refused until F2b.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod schema;
pub mod service;
