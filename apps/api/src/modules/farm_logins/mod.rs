//! Farmer logins (G11): an officer issues a temporary password for a registered person's phone;
//! the farmer signs in with phone + password and must choose their own before anything else
//! counts (surveys refuse a temporary password). No email, no SMS, no third party.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod rules;
pub mod service;
