//! Farm productive-use requests (RUAHA `pue_request`, `energy_estimate`, `v_village_energy`,
//! `v_village_pue_pipeline`): a farmer asks to rent or buy a machine, ops decides, and the
//! database keeps the Level 1 energy estimate.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod rules;
pub mod service;
