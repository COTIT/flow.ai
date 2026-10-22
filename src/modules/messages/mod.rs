//! Workspace mail (Laravel SendMessage / SendMail): `POST /send-message`, the email profile and
//! Office 365 connection tests, and the send logs (`email_send_logs`).

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
