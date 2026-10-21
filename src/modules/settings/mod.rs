//! Workspace settings (Laravel WorkspaceSetting/Setting core): one jsonb row per workspace,
//! Laravel's sections, defaults and rules. Kept providers' credentials are stored sealed in
//! their own table (`credentials`); the dropped providers' are refused.

pub mod credentials;
pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod schema;
pub mod service;
