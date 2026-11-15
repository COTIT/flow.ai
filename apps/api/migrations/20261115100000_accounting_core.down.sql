DELETE FROM document_counters WHERE series = 'journal';
ALTER TABLE document_counters DROP CONSTRAINT document_counters_series_check,
    ADD CONSTRAINT document_counters_series_check CHECK (series IN ('invoice', 'order'));
DROP TABLE journal_lines;
DROP TABLE journal_entries;
DROP TABLE accounting_effects;
DROP TABLE accounting_commands;
DROP TABLE accounting_periods;
DROP TABLE accounting_fiscal_years;
DROP TABLE account_mappings;
DROP TABLE accounts;
DROP TRIGGER workspaces_accounting_settings ON workspaces;
DROP TABLE accounting_settings;
DROP FUNCTION journal_entries_balanced();
DROP FUNCTION accounting_posted_fixed();
DROP FUNCTION accounting_calendar_check();
DROP FUNCTION accounting_settings_seed();
