-- Refuse downgrade while new pending states exist; cancel/drain through supported code first.
ALTER TABLE assistant_actions DROP CONSTRAINT assistant_actions_proposal_lifetime_check;
ALTER TABLE assistant_actions ADD CONSTRAINT assistant_actions_check CHECK(state IN ('proposed','approved') OR proposal IS NULL);
ALTER TABLE assistant_actions DROP CONSTRAINT assistant_actions_state_check;
ALTER TABLE assistant_actions ADD CONSTRAINT assistant_actions_state_check CHECK(state IN ('proposed','approved','dispatching','completed','unknown','cancelled','expired','failed','queued','partial'));
DROP INDEX assistant_actions_batch_idx;
ALTER TABLE assistant_actions DROP CONSTRAINT assistant_actions_batch_fk;
ALTER TABLE assistant_actions DROP CONSTRAINT assistant_actions_batch_position_check;
ALTER TABLE assistant_actions DROP CONSTRAINT assistant_actions_batch_verdict_scope_check;
ALTER TABLE assistant_actions DROP COLUMN batch_id;
ALTER TABLE assistant_actions DROP COLUMN batch_index;
ALTER TABLE assistant_actions DROP COLUMN batch_verdict;
DROP TABLE assistant_batches;
