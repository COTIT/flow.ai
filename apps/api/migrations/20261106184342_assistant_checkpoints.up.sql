-- Generated from @langchain/langgraph-checkpoint-postgres 1.1.2 (MIT).
-- SQLx owns DDL; restricted runtime must never call saver.setup().
CREATE SCHEMA assistant_checkpoint;

CREATE TABLE IF NOT EXISTS "assistant_checkpoint".checkpoint_migrations (
    v INTEGER PRIMARY KEY
  );
INSERT INTO assistant_checkpoint.checkpoint_migrations(v) VALUES (0);

CREATE TABLE IF NOT EXISTS "assistant_checkpoint".checkpoints (
    thread_id TEXT NOT NULL,
    checkpoint_ns TEXT NOT NULL DEFAULT '',
    checkpoint_id TEXT NOT NULL,
    parent_checkpoint_id TEXT,
    type TEXT,
    checkpoint JSONB NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    PRIMARY KEY (thread_id, checkpoint_ns, checkpoint_id)
  );
INSERT INTO assistant_checkpoint.checkpoint_migrations(v) VALUES (1);

CREATE TABLE IF NOT EXISTS "assistant_checkpoint".checkpoint_blobs (
    thread_id TEXT NOT NULL,
    checkpoint_ns TEXT NOT NULL DEFAULT '',
    channel TEXT NOT NULL,
    version TEXT NOT NULL,
    type TEXT NOT NULL,
    blob BYTEA,
    PRIMARY KEY (thread_id, checkpoint_ns, channel, version)
  );
INSERT INTO assistant_checkpoint.checkpoint_migrations(v) VALUES (2);

CREATE TABLE IF NOT EXISTS "assistant_checkpoint".checkpoint_writes (
    thread_id TEXT NOT NULL,
    checkpoint_ns TEXT NOT NULL DEFAULT '',
    checkpoint_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    idx INTEGER NOT NULL,
    channel TEXT NOT NULL,
    type TEXT,
    blob BYTEA NOT NULL,
    PRIMARY KEY (thread_id, checkpoint_ns, checkpoint_id, task_id, idx)
  );
INSERT INTO assistant_checkpoint.checkpoint_migrations(v) VALUES (3);

ALTER TABLE "assistant_checkpoint".checkpoint_blobs ALTER COLUMN blob DROP not null;
INSERT INTO assistant_checkpoint.checkpoint_migrations(v) VALUES (4);

ALTER TABLE "assistant_checkpoint".checkpoint_writes ADD COLUMN IF NOT EXISTS task_path TEXT NOT NULL DEFAULT '';
INSERT INTO assistant_checkpoint.checkpoint_migrations(v) VALUES (5);

DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'assistant_checkpoint_runtime') THEN
    GRANT USAGE ON SCHEMA assistant_checkpoint TO assistant_checkpoint_runtime;
    GRANT SELECT ON assistant_checkpoint.checkpoint_migrations TO assistant_checkpoint_runtime;
    GRANT SELECT, INSERT, UPDATE, DELETE ON assistant_checkpoint.checkpoints, assistant_checkpoint.checkpoint_blobs, assistant_checkpoint.checkpoint_writes TO assistant_checkpoint_runtime;
  END IF;
END $$;
