//! Farm Control Tower reads (RUAHA `v_village_production`, `v_village_data_quality`): one
//! village at a time, for its staff.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
