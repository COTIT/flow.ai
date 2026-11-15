//! Native accounting core (Flow ERP M1): book setup, chart of accounts, fiscal calendar,
//! manual journals with server reviews, posting and manual-only reversal, workspace gate.

pub mod dto;
pub mod gate;
pub mod handlers;
pub mod journals;
pub mod repo;
pub mod routes;
pub mod service;
