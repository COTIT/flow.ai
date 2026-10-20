//! Hourly rates (Laravel WorkspaceSetting/HourlyRate): CRUD with an optional VAT group.
//! The `/users/{user}` rate links wait for the hour-registration port.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
