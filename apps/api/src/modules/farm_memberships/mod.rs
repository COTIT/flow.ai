//! Farm memberships: a farm role (farmer, field officer, ops, admin) plus an optional village,
//! per user and workspace. The guard that reads them is `http::farm::FarmAccess`.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
