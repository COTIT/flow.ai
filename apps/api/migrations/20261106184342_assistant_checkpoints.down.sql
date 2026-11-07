-- Reverting destroys assistant workflow checkpoints; application rollback preserves this schema.
DROP SCHEMA assistant_checkpoint CASCADE;
