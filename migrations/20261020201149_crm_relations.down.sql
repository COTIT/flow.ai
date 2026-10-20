-- pg_trgm stays: later modules may index with it, and dropping it needs no rollback.
DROP TABLE contacts;
DROP TABLE relations;
