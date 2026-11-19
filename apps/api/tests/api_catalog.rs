//! API integration tests: products, pricing, storefront, assistant, Flow, mail, jobs,
//! integrations and workflows.
//! The suite is split over the `tests/api_*.rs` crates so no single compile outgrows CI's memory
//! (one ~97k-line crate peaked at 15.3 GB). Feature files stay in `tests/api/`; files that call
//! each other's helpers (`crate::other::…`) share a crate. New file: declare it in one crate below;
//! `every_file_is_a_module` in `api_core.rs` fails otherwise.

#[path = "common/mod.rs"]
mod common;

#[path = "api/approvals.rs"]
mod approvals;
#[path = "api/approvals_schema.rs"]
mod approvals_schema;
#[path = "api/assistant.rs"]
mod assistant;
#[path = "api/assistant_batches.rs"]
mod assistant_batches;
#[path = "api/assistant_capability_auth.rs"]
mod assistant_capability_auth;
#[path = "api/assistant_gaps.rs"]
mod assistant_gaps;
#[path = "api/assistant_identity.rs"]
mod assistant_identity;
#[path = "api/assistant_mail_recipients.rs"]
mod assistant_mail_recipients;
#[path = "api/assistant_price_import.rs"]
mod assistant_price_import;
#[path = "api/assistant_retention.rs"]
mod assistant_retention;
#[path = "api/discounts.rs"]
mod discounts;
#[path = "api/email_profiles.rs"]
mod email_profiles;
#[path = "api/email_templates.rs"]
mod email_templates;
#[path = "api/files.rs"]
mod files;
#[path = "api/flow_conversations.rs"]
mod flow_conversations;
#[path = "api/flow_scenes.rs"]
mod flow_scenes;
#[path = "api/imports.rs"]
mod imports;
#[path = "api/integrations.rs"]
mod integrations;
#[path = "api/integrations_schema.rs"]
mod integrations_schema;
#[path = "api/jobs.rs"]
mod jobs;
#[path = "api/load_deals.rs"]
mod load_deals;
#[path = "api/load_jobs.rs"]
mod load_jobs;
#[path = "api/netsuite.rs"]
mod netsuite;
#[path = "api/pdf_templates.rs"]
mod pdf_templates;
#[path = "api/price_corrections.rs"]
mod price_corrections;
#[path = "api/pricing.rs"]
mod pricing;
#[path = "api/product_brands.rs"]
mod product_brands;
#[path = "api/product_bulk_runs.rs"]
mod product_bulk_runs;
#[path = "api/product_category_link.rs"]
mod product_category_link;
#[path = "api/product_fields.rs"]
mod product_fields;
#[path = "api/product_images.rs"]
mod product_images;
#[path = "api/product_master_guards.rs"]
mod product_master_guards;
#[path = "api/product_variants.rs"]
mod product_variants;
#[path = "api/products.rs"]
mod products;
#[path = "api/quick_search.rs"]
mod quick_search;
#[path = "api/scheduled_reports.rs"]
mod scheduled_reports;
#[path = "api/searches.rs"]
mod searches;
#[path = "api/send_message.rs"]
mod send_message;
#[path = "api/special_offers.rs"]
mod special_offers;
#[path = "api/storefront.rs"]
mod storefront;
#[path = "api/storefront_connections.rs"]
mod storefront_connections;
#[path = "api/storefront_design.rs"]
mod storefront_design;
#[path = "api/storefront_design_edits.rs"]
mod storefront_design_edits;
#[path = "api/storefront_preview_links.rs"]
mod storefront_preview_links;
#[path = "api/webhook_endpoints.rs"]
mod webhook_endpoints;
#[path = "api/webhook_events.rs"]
mod webhook_events;
#[path = "api/workflows.rs"]
mod workflows;
