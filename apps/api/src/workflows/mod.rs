//! Workflow engine contract (WF-01, contracts.md §3-§5.5): execution state helpers with
//! claim/takeover/fencing, append-only events, incidents, job dispatch seam, HTTP routes.

pub mod dispatch;
mod dto;
pub mod events;
pub mod executions;
mod handlers;
pub mod incidents;
mod repo;
pub mod routes;
mod service;

#[cfg(test)]
pub(crate) mod testkit;
