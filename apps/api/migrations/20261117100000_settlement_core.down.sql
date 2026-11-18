-- Drops the settlement tables and every row in them.
DROP TABLE accounting_source_links;
DROP TABLE accounting_carrying_adjustments;
DROP TABLE accounting_allocations;
DROP TABLE accounting_open_items;
DROP TABLE bank_accounts;
DROP FUNCTION accounting_allocation_applied();
DROP FUNCTION accounting_open_item_opened();
DROP FUNCTION settlement_guard();
