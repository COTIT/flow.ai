//! Shelf bins (Laravel ERP/WarehouseSectionRowBin): the leaf of the bin hierarchy, plus the
//! flat bin list of a whole warehouse (`GET /warehouses/{id}/bins`).

pub mod dto;
pub mod handlers;
pub mod repo;
pub mod routes;
pub mod service;
