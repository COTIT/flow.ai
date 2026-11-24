//! Local endpoint tests share the same app, database helpers and feature files as the full suite.
#[path = "../common/mod.rs"]
mod common;

include!(env!("VERZO_ENDPOINT_MODULES"));
