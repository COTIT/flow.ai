//! Farm bug reports (RUAHA `bug_report`): the in-app bug button files a screenshot or photo,
//! the last 30 actions and what went wrong. Anyone with a farm role files under their own name;
//! only farm admin reads; no route changes or deletes a report.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
