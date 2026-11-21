//! WF-03 failed-sync detection and controlled recovery (contracts §11.3, §11.7): category map,
//! expectations of Flow-run writes, the due sweep, connection health, the verify read-back.

pub mod category;
pub mod expectations;
pub mod health;
pub mod sweep;
