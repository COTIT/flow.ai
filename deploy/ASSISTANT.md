# Office assistant deployment

The assistant ships disabled in both environments. Production provisioning, provider configuration and deployment need their own review. The commands below are operator instructions; local deployment tests use stubs.

Use each environment's own ElevenLabs agent, credentials and database login. The API owns business rules, approvals and the action ledger. Office uses a restricted PostgreSQL login for task checkpoints. Neither service runs a Node worker or an internal resume endpoint.

## Secrets and role setup

Keep `deploy/<environment>/secrets/` at mode `700`. Place these files inside it:

| File | Consumer | Contents |
| --- | --- | --- |
| `elevenlabs_api_key` | API and Office | Same environment-specific provider key; API uses it for cleanup |
| `assistant_bridge_secret` | API and Office | Same random bridge signing secret |
| `assistant_checkpoint_password` | Operator helper only | Password for `assistant_checkpoint_<environment>` |
| `assistant_checkpoint_url` | Office only | PostgreSQL URL for that restricted login, host `db`, database `genesy` |
| `langsmith_api_key` | Office only | Optional tracing key; use an empty file while tracing is off |

Mounted files use mode `444`, matching existing secret files: containers run as different non-root users, and the mode `700` parent directory prevents other host users from reading them. Keep the unmounted `assistant_checkpoint_password` at mode `600`. Percent-encode the password when constructing the URL. Never use the API's administrator URL as the checkpoint URL.

With the database running and `ASSISTANT_ENABLED=false`, bootstrap the role before the release migration:

```bash
bash deploy/assistant-role.sh production bootstrap
```

Use `staging` for staging. The helper reads the password from its file and sends SQL through stdin. It creates `assistant_checkpoint_runtime` as a NOLOGIN group and grants it to one environment login; it never resets another environment's password. Repeating the command is safe. SQLx migrations own schema creation and DDL. The pinned checkpointer is `@langchain/langgraph-checkpoint-postgres@1.1.2`, schema `assistant_checkpoint`, migration version `5`; runtime code must never call `.setup()`.

After applying migrations through the normal reviewed release, verify grants:

```bash
bash deploy/assistant-role.sh production verify
```

Verification requires checkpoint DML, read-only checkpoint migration metadata, and denies business-table access and persistent DDL. A failure leaves the assistant disabled. Provisioning after migrations also works: `bootstrap` grants the existing checkpoint tables before verification.

Each environment login has `CONNECTION LIMIT 10`: two overlapping Office instances during a blue/green release, each with one process-wide pool of four connections and one separate readiness connection. At most two graph writers may hold advisory-lock connections per instance, leaving two pool slots for checkpoint reads and writes. Budget these ten connections per environment alongside the existing Rust pools and PostgreSQL administrator reserve. Additional Office replicas require a reviewed capacity change; do not create a pool per route bundle. Re-run `bootstrap` for an existing role before enabling this release; `verify` rejects the old four-connection limit.

Local disposable databases can run `assistant-role.sql` with psql variables `checkpoint_role=assistant_checkpoint_local` and `checkpoint_mode=identities|bootstrap|verify`. `identities` creates role identities and membership without a password or table grants. Set the local login password separately through a private connection before runtime use.

## Enable and verify

`assistant-agent.template.json` records a sanitized, read-only development agent snapshot. `assistant-validation.json` pins its observed provider version and SHA256 digest. Neither file is loaded by Office or grants access: the Rust API and generated operation policy remain authoritative, and readiness checks the actual provider configuration. The snapshot contains the shared prompt, native model/fallback chain, two client tool definitions, English/Dutch/Swahili settings, file limits, privacy settings and override/auth flags. It omits environment agent/tool/voice IDs, origins, private URLs, knowledge bases, webhooks and credentials.

Before provisioning another environment, review the snapshot against that environment's current agent. Through the normal reviewed operator procedure, create or update the two named client tools, then attach their environment-specific IDs to the agent's `prompt.tool_ids`; `clientTools` in the template is an expanded review section, not an agent PATCH field. Apply only the reviewed `configuration` diff and configure the environment's voice, origins and signed-session bindings separately. Keep the native planner and shared tools; do not replace them with a custom LLM or a webhook executor. Fetch the resulting configuration read-only, compare it, then record its new version and snapshot digest before acceptance testing. No script here mutates provider accounts.

The observed agent disables voice recording and sets retention to zero days with audio/transcript deletion. ElevenLabs documents zero days as immediate deletion; the separate `zero_retention_mode` flag is false, so this snapshot makes no Enterprise Zero Retention Mode claim. See [ElevenLabs privacy settings](https://elevenlabs.io/docs/eleven-agents/customization/privacy). Authentication is enabled; origin requirements and allowlists still need environment review. Verify the offline snapshot with `bash deploy/assistant-agent.test.sh`.

Talk-back and idle settings: the agent's prompt has the four-beat script and the silence check-in, and its turn settings take the turn after 20 s of silence and end the call after 60 s (`turn_timeout` 20, `silence_end_call_timeout` 60). The template also records `force_pre_tool_speech: true` on `execute_genesy_tool`, so the agent always speaks before running a business tool. That flag lives on the client tool itself, and one `execute_genesy_tool` tool is shared by the Production, Staging and Development agents. Flipping it changes all three at once. The Development agent uses its own copy of `execute_genesy_tool`, with the same name, description and schema and the flag on. The shared tool used by Production and Staging still reads false. Flip it on the shared tool only at release time and only with the owner's explicit yes, then re-fetch the tool read-only and confirm the flag. On Staging and Production, also set Agent settings → Conversational behavior → "Take turn after silence" 20 s and "End conversation after silence" 60 s, publish, and compare the prompt with this template.

The validation record remains **pending**. Local standalone packaging and disposable PostgreSQL restore checks passed. Public image fetching recovered; a preliminary Linux build and two-instance 384/512 MiB upload/read/admission-restart checks passed. That image predates final batch and money-review changes. Final source, checkpoint-pool saturation and postcommit acknowledgement fault checks remain required. Final acceptance must pin the release commit, image digest and provider version together and complete container load, real-stack and paid multilingual checks before enablement.

Set `ELEVENLABS_AGENT_ID` and `ASSISTANT_WORKSPACE_ALLOWLIST` in the environment file. The allowlist contains one through 100 comma-separated workspace UUIDs; an empty, malformed or wildcard list fails closed. Every new session, upload, tool preparation, secure handoff and confirmation checks the authenticated workspace. Batch controllers recheck admission before each later step. Removing a workspace preserves authenticated status, cancellation and cleanup. This limits initial rollout without dropping any capability inside an admitted workspace. Keep `LANGSMITH_TRACING=false` unless tracing is required; inputs and outputs remain hidden, and callback work is awaited. After the existing repository checks, queued safety regressions, packaging and runtime readiness pass, use the normal reviewed deploy command for the admitted workspace. The user's later scope correction defers exhaustive model/capability benchmarking and extended load experiments; those results must remain unverified. Physical-device checks belong to the user after deployment.

The assistant overlay supplies runtime secret files; credentials never enter Docker build args. Once `elevenlabs_api_key` exists, the overlay remains selected while paused so Rust cleanup can finish. An installation that has never run the assistant release can still omit the overlay while disabled. Enabled deployments verify role privileges after migrations, then require Office's `/api/voice/readiness` to return exactly `200` from inside its container. This check makes no paid vendor request. Office's Docker `/healthz` probe also requires exactly `200`; a `404` fails.

Stage with the same 384 MiB Office limit used in staging and the 512 MiB production ceiling. Before wider enablement, check concurrent bounded uploads, task requests and shutdown during an active request; confirm no OOM restart, no lost checkpoint acknowledgment and no duplicate business action. Readiness alone does not prove these load properties.

## Rollback and recovery

First set `ASSISTANT_ENABLED=false`. Restart the affected services through the reviewed deployment procedure to stop new sessions. Keep provider cleanup credentials until all outstanding remote deletion attempts finish. Preserve ledgers and checkpoints. Existing native invoice delivery jobs continue while assistant admission is disabled, so retain the Rust invoice worker and its mail/PDF configuration too.

For a schema change consisting entirely of the reviewed additive assistant migrations, use:

```bash
bash deploy/deploy.sh production rollback --compatible-assistant
```

The first assistant release freezes nine migration checksums in `assistant-compatibility.json`: `20261106184342` checkpoints, `20261106184359` actions, `20261106185501` upload limits, `20261106190652` upload metadata, `20261106192623` approval events, `20261106200256` review facts, `20261106204413` invoice delivery, `20261106210133` delivery indexes and `20261107000925` reviewed batches. Never edit these applied files. A new migration needs its own reviewed checksum entry; no filename pattern automatically approves it.

This explicit path requires the assistant to be disabled. It compares every newer applied SQLx migration with the committed SHA384 allowlist, verifies checkpointer schema version `5`, and checks graph `office-assistant-v1` / workflow `2` / saver `1.1.2` / invoice delivery protocol `1` / batch protocol `1`. Both the running release and rollback target must declare the same fingerprint and contain the invoice delivery module and both worker dispatch paths, batch controller admission and batch cleanup. These checks also run when the schema version is unchanged. It records that fingerprint in deployment state. Unlisted migrations, checksum changes, a different fingerprint, missing worker support or incomplete evidence stop rollback before changing the running version. No business database restore occurs on this path.

The first assistant release cannot use compatible rollback to a pre-assistant or unversioned release: those APIs cannot finish the new durable invoice jobs. Pause admission and deploy a reviewed fix that retains worker support. Keep ordinary unchanged-schema rollback limited to reviewed worker-capable revisions as well.

Assistant releases require `--compatible-assistant` even when the schema is unchanged. Ordinary rollback remains available for releases without the assistant fingerprint. `--restore-db` remains disaster recovery: it replaces the whole database and loses writes since the selected dump. Do not use it for routine assistant rollback.

Reinstall the ops kit after deploying these scripts so restore drills use its root-owned copy of the role SQL. Both restore paths create role identities before restoring ACLs, then verify checkpoint privileges when that schema exists. Fresh disaster-recovery clusters still need private login passwords and runtime secret files; `pg_dump` contains neither role passwords nor cluster-wide role definitions. Reauthorize users before resuming any retained task.
