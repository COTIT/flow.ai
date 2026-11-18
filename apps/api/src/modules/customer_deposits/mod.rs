//! Customer deposits (Flow ERP M4, masterplan §12 functional-currency prepayment): receive
//! money ahead of a sale (DR cash|bank|provider_clearing / CR customer_deposits + deposit open
//! item) and apply it to a recognised, unbilled sale (allocation + DR customer_deposits / CR
//! unbilled_receivables). Active books only.

pub mod dto;
pub mod handlers;
pub mod routes;
pub mod service;
