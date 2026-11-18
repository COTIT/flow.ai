//! API integration tests: farms, lots and supply links.
//! The suite is split over the `tests/api_*.rs` crates so no single compile outgrows CI's memory
//! (one ~97k-line crate peaked at 15.3 GB). Feature files stay in `tests/api/`; files that call
//! each other's helpers (`crate::other::…`) share a crate. New file: declare it in one crate below;
//! `every_file_is_a_module` in `api_core.rs` fails otherwise.

#[path = "common/mod.rs"]
mod common;

#[path = "api/assistant_farm_facts.rs"]
mod assistant_farm_facts;
#[path = "api/farm_bug_reports.rs"]
mod farm_bug_reports;
#[path = "api/farm_catalogue.rs"]
mod farm_catalogue;
#[path = "api/farm_demo.rs"]
mod farm_demo;
#[path = "api/farm_documents.rs"]
mod farm_documents;
#[path = "api/farm_energy.rs"]
mod farm_energy;
#[path = "api/farm_loans.rs"]
mod farm_loans;
#[path = "api/farm_logins.rs"]
mod farm_logins;
#[path = "api/farm_mapping.rs"]
mod farm_mapping;
#[path = "api/farm_market.rs"]
mod farm_market;
#[path = "api/farm_packing.rs"]
mod farm_packing;
#[path = "api/farm_passbook.rs"]
mod farm_passbook;
#[path = "api/farm_payouts.rs"]
mod farm_payouts;
#[path = "api/farm_pue.rs"]
mod farm_pue;
#[path = "api/farm_records.rs"]
mod farm_records;
#[path = "api/farm_registration.rs"]
mod farm_registration;
#[path = "api/farm_sales.rs"]
mod farm_sales;
#[path = "api/farm_supply.rs"]
mod farm_supply;
#[path = "api/farm_supply_corrections.rs"]
mod farm_supply_corrections;
#[path = "api/farm_surveys.rs"]
mod farm_surveys;
#[path = "api/farm_tenancy.rs"]
mod farm_tenancy;
#[path = "api/farm_tower.rs"]
mod farm_tower;
#[path = "api/farm_vouchers.rs"]
mod farm_vouchers;
#[path = "api/lot_invoices.rs"]
mod lot_invoices;
#[path = "api/lot_orders.rs"]
mod lot_orders;
#[path = "api/lot_passports.rs"]
mod lot_passports;
#[path = "api/lot_public.rs"]
mod lot_public;
#[path = "api/lot_stock.rs"]
mod lot_stock;
#[path = "api/supply_links.rs"]
mod supply_links;
