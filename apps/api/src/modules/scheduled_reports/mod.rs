//! Scheduled reports: a member's own read-only reports (sales, to-do, overdue invoices, low
//! stock) mailed to their own login on a daily, weekly or monthly slot in the workspace time
//! zone. Never a write. Contract: `docs/api/scheduled_reports.md`.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod report;
pub mod routes;
pub mod runner;
pub mod service;
