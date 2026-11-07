-- Bounded reviewed facts; legacy critical proposals fail closed at claim without DML.
ALTER TABLE assistant_actions ADD COLUMN review_facts jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (octet_length(review_facts::text) <= 32768);
