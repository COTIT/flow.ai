//! API integration tests: accounting, invoices, payments, sales, tills and taxes.
//! The suite is split over the `tests/api_*.rs` crates so no single compile outgrows CI's memory
//! (one ~97k-line crate peaked at 15.3 GB). Feature files stay in `tests/api/`; files that call
//! each other's helpers (`crate::other::…`) share a crate. New file: declare it in one crate below;
//! `every_file_is_a_module` in `api_core.rs` fails otherwise.

#[path = "common/mod.rs"]
mod common;

#[path = "api/acceptance.rs"]
mod acceptance;
#[path = "api/accounting.rs"]
mod accounting;
#[path = "api/accounting_aging.rs"]
mod accounting_aging;
#[path = "api/accounting_reports.rs"]
mod accounting_reports;
#[path = "api/accounting_schema.rs"]
mod accounting_schema;
#[path = "api/activation.rs"]
mod activation;
#[path = "api/analytics.rs"]
mod analytics;
#[path = "api/app_keys.rs"]
mod app_keys;
#[path = "api/assistant_order_invoice_quotes.rs"]
mod assistant_order_invoice_quotes;
#[path = "api/assistant_preconditions.rs"]
mod assistant_preconditions;
#[path = "api/assistant_product_quotes.rs"]
mod assistant_product_quotes;
#[path = "api/assistant_review_rows.rs"]
mod assistant_review_rows;
#[path = "api/assistant_review_safety.rs"]
mod assistant_review_safety;
#[path = "api/assistant_storefront_design.rs"]
mod assistant_storefront_design;
#[path = "api/attention.rs"]
mod attention;
#[path = "api/attention_priorities.rs"]
mod attention_priorities;
#[path = "api/bank_reconciliation.rs"]
mod bank_reconciliation;
#[path = "api/cash_places.rs"]
mod cash_places;
#[path = "api/fx_revaluation.rs"]
mod fx_revaluation;
#[path = "api/gateway_accounts.rs"]
mod gateway_accounts;
#[path = "api/gateway_recovery.rs"]
mod gateway_recovery;
#[path = "api/inventory_valuation.rs"]
mod inventory_valuation;
#[path = "api/invoice_actions.rs"]
mod invoice_actions;
#[path = "api/invoice_credit.rs"]
mod invoice_credit;
#[path = "api/invoice_gateway.rs"]
mod invoice_gateway;
#[path = "api/invoice_money.rs"]
mod invoice_money;
#[path = "api/invoice_payments.rs"]
mod invoice_payments;
#[path = "api/invoice_send.rs"]
mod invoice_send;
#[path = "api/invoices.rs"]
mod invoices;
#[path = "api/login.rs"]
mod login;
#[path = "api/money_sql.rs"]
mod money_sql;
#[path = "api/numbering.rs"]
mod numbering;
#[path = "api/opening.rs"]
mod opening;
#[path = "api/payables.rs"]
mod payables;
#[path = "api/payables_candidates.rs"]
mod payables_candidates;
#[path = "api/payments.rs"]
mod payments;
#[path = "api/payments_stripe.rs"]
mod payments_stripe;
#[path = "api/period_close.rs"]
mod period_close;
#[path = "api/pricing_sql.rs"]
mod pricing_sql;
#[path = "api/provider_observations.rs"]
mod provider_observations;
#[path = "api/purchasing.rs"]
mod purchasing;
#[path = "api/sale_tax.rs"]
mod sale_tax;
#[path = "api/sales_billing.rs"]
mod sales_billing;
#[path = "api/sales_groups.rs"]
mod sales_groups;
#[path = "api/sales_recognition.rs"]
mod sales_recognition;
#[path = "api/sales_returns.rs"]
mod sales_returns;
#[path = "api/sales_returns_candidates.rs"]
mod sales_returns_candidates;
#[path = "api/sales_returns_schema.rs"]
mod sales_returns_schema;
#[path = "api/settlement.rs"]
mod settlement;
#[path = "api/settlement_schema.rs"]
mod settlement_schema;
#[path = "api/shipment_methods.rs"]
mod shipment_methods;
#[path = "api/shipments.rs"]
mod shipments;
#[path = "api/stock_transfer.rs"]
mod stock_transfer;
#[path = "api/stocktakes.rs"]
mod stocktakes;
#[path = "api/tax_line.rs"]
mod tax_line;
#[path = "api/tax_profiles.rs"]
mod tax_profiles;
#[path = "api/till_cash.rs"]
mod till_cash;
#[path = "api/till_catalog.rs"]
mod till_catalog;
#[path = "api/till_login.rs"]
mod till_login;
#[path = "api/till_pairing.rs"]
mod till_pairing;
#[path = "api/till_privileged.rs"]
mod till_privileged;
#[path = "api/till_quote.rs"]
mod till_quote;
#[path = "api/till_refunds.rs"]
mod till_refunds;
#[path = "api/till_sales.rs"]
mod till_sales;
#[path = "api/vat_groups.rs"]
mod vat_groups;
