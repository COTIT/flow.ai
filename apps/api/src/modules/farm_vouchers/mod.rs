//! Survey vouchers (RUAHA `survey_voucher`, `voucher_event`): a single-use claim on a fixed cash
//! incentive, paid face to face. The code is the household's; staff look it up (every scan
//! logged) and redeem it. Whoever registered or verified the household never redeems; a random
//! share is held for ops. The trail is append-only.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod rules;
pub mod service;
