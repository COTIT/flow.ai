//! Native accounting core (Flow ERP M1): book setup, chart of accounts, fiscal calendar,
//! manual journals with server reviews, posting and manual-only reversal, workspace gate.
//! Read-only reports (trial balance, general ledger, P&L, balance sheet) in `reports`.
//! Bank statements and statement-line reconciliation in `banking`. Period/year close in `close`.
//! Opening conversion (staging, review, activation core, corrections) in `opening`.

pub mod banking;
pub mod close;
pub mod dto;
pub mod gate;
pub mod handlers;
pub mod journals;
pub mod opening;
pub mod repo;
pub mod reports;
pub mod revaluation;
pub mod routes;
pub mod service;
pub mod settlement;
