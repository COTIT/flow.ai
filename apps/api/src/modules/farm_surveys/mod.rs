//! Surveys and their answers (RUAHA `survey`, G14): admin authors draft -> live -> closed, the
//! content frozen once live; a farmer answers once per household and receives a voucher
//! (`farm_vouchers`).

pub mod dto;
pub mod eligibility;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
