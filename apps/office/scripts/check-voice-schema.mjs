import assert from "node:assert/strict"
import { readFile, writeFile } from "node:fs/promises"
import { createRequire } from "node:module"
import { dirname, join } from "node:path"
import { fileURLToPath, pathToFileURL } from "node:url"

const require = createRequire(import.meta.url)
const distribution = dirname(require.resolve("@langchain/langgraph-checkpoint-postgres"))
const { version } = JSON.parse(await readFile(join(distribution, "../package.json"), "utf8"))
assert.equal(version, "1.1.2", "Review checkpoint SQL before changing the pinned saver version")
const { getMigrations } = await import(pathToFileURL(join(distribution, "migrations.js")).href)
const migrations = getMigrations("assistant_checkpoint")
assert.equal(migrations.length, 6)
const source = `-- Generated from @langchain/langgraph-checkpoint-postgres ${version} (MIT).\n-- SQLx owns DDL; restricted runtime must never call saver.setup().\nCREATE SCHEMA assistant_checkpoint;\n\n${migrations.map((sql, version) => `${sql}\nINSERT INTO assistant_checkpoint.checkpoint_migrations(v) VALUES (${version});`).join("\n\n")}\n\nDO $$ BEGIN\n  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'assistant_checkpoint_runtime') THEN\n    GRANT USAGE ON SCHEMA assistant_checkpoint TO assistant_checkpoint_runtime;\n    GRANT SELECT ON assistant_checkpoint.checkpoint_migrations TO assistant_checkpoint_runtime;\n    GRANT SELECT, INSERT, UPDATE, DELETE ON assistant_checkpoint.checkpoints, assistant_checkpoint.checkpoint_blobs, assistant_checkpoint.checkpoint_writes TO assistant_checkpoint_runtime;\n  END IF;\nEND $$;\n`
const migration = fileURLToPath(new URL("../../api/migrations/20261106184342_assistant_checkpoints.up.sql", import.meta.url))
if (process.argv.includes("--write")) await writeFile(migration, source)
else assert.equal(await readFile(migration, "utf8"), source, "Pinned checkpointer SQL differs from SQLx migration")
console.log(`Checkpoint schema matches saver ${version}, migration ${migrations.length - 1}`)
