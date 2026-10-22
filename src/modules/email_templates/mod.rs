//! Email templates (Laravel WorkspaceSetting/EmailTemplate): a subject and body with
//! `{variable}` placeholders per domain; one default per domain.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
