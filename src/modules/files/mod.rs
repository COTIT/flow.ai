//! Stored files (Laravel System/File): upload, list, get, delete. Bytes live in the uploads
//! bucket (`storage`), metadata in `files`; downloads are short-lived signed URLs.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
pub mod sniff;
