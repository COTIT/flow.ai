//! Plot title documents (RUAHA `plot_document`): photos or PDFs of a plot's land papers, filed
//! by staff of the plot's village, never changed or deleted. Stored with the files module's
//! bucket and type sniffing, but in their own table: the workspace file list and file delete
//! never see them.

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
