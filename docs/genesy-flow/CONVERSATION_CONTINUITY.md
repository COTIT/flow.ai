# Flow conversation continuity

Release preparation, 2026-11-13. Staging and production deployment are authorized. Flow history is always available; there is no history rollout flag.

## Storage and authority

Logical conversations belong to a workspace and user. Each connection gets a new provider source and login-bound execution session. A fresh authenticated login can read its user's history; historical dialogue cannot approve or replay an action. Rust approval and dispatch checks remain authoritative.

Client dialogue is provisional. Signed native transcription or fixed-origin provider reconciliation replaces complete source snapshots transactionally. Queued and sent/unconfirmed text remain distinct. Tombstones reject late ingestion after deletion. Message cursors carry a content revision and reject stale pagination.

Only dialogue and native summaries are archived. Approval codes and transport credentials are redacted. Tool payloads, reasoning, attachments and audio are excluded. Native summaries are lossy indexes; exact originals remain available through the session-bound `assistant_conversation_history` tool. Initialization includes up to 20 recent complete turns, with a 32 KiB context budget and a 12 KiB summary budget. These are application limits.

History expires after 90 inactive days. Reads and ingestion retries do not extend expiry. The existing purge worker erases expired dialogue and summaries while retaining tombstones. Business audit retention remains separate.

Provider archival waits until the source is finalized or its cleanup deadline reaches one hour. Cleanup attempts rotate past failures; failed remote deletion remains pending. A worker outage or provider failure can delay physical erasure. Only provider 200, 204 or 404 acknowledges deletion.

## Controls

`/history` reuses Flow's shell and transcript renderer. Browsing preserves the active call, draft and review. End Call stops transport and preserves history. End Conversation closes the logical conversation. New Conversation creates a new logical ID. Delete requires confirmation, stops an active connection before purge and clears the saved pointer.

Reload authorizes the saved logical ID before restoring text. Microphone use requires explicit activation. Same-origin Web Locks coordinate capture; Rust rejects competing active sources. Saved pointers contain only workspace, user and logical conversation IDs.

## Deployment and recovery

Apply the additive API migration and deploy matched API, Office and Flow artifacts through the normal backup, migration and health checks. Flow always requests persistent history. Existing Office sessions retain their legacy behavior unless they request history. History starts validate the actual HMAC receiver and provider transcript configuration before allocating transport. The agent's empty `flow_history_context` default permits legacy starts.

The public callback is Office's `/api/voice/history-webhook`; Flow does not proxy this route. Configure the development agent for the transcription event only, JSON format, `exclude_transcript: false`, HMAC authentication and retries. Keep audio delivery disabled and preserve existing recording and retention settings. Supply the matching server-only `ELEVENLABS_WEBHOOK_SECRET` or `ELEVENLABS_WEBHOOK_SECRET_FILE`, with at least 16 characters. Read back provider settings and selected workspace webhook configuration.

Release verification requires observed complete native transcription, committed local receipt, duplicate/retry handling, provider/config readback and correct resumed recall. Readiness rejects missing history binding, excluded transcripts, recording, general prompt override or missing HMAC webhook configuration. Passing synthetic client tests alone does not satisfy native delivery.

If the release fails, pause new assistant sessions with the existing `ASSISTANT_ENABLED=false` admission control on the new binaries. Keep the new Office callback and Rust ingestion, retention and deletion workers running, with credentials, and fix forward. Older binaries cannot maintain this history. Do not reverse the migration or restore the business database for routine rollback.

## Executed evidence

Development provider tests used synthetic text sessions and refused business operations. Both restored-context recall and older exact-history retrieval returned the later corrected preference; zero business operations executed. The test observer asserted initialization bytes sent to the provider. Created provider sessions were deleted afterward.

Only the development history prompt section and empty dynamic-variable default were applied and read back. Existing prompt content outside that section, platform settings, tools, recording and zero-day retention were preserved. Full repository template publication remains unverified against live drift.

Production browser history checks passed 36 cases across desktop, 375px and tablet. They include actual SDK transport with synthetic audio tracks, pending text, reconnects, auth expiry, multiple tabs, active deletion teardown and stale callbacks. Mocked transport and synthetic tracks do not establish physical microphone behavior.

Combined Office/Flow production checks passed 282 cases with six existing skips. The relevant real-stack browser checks passed 18 cases. A separate synthetic HMAC smoke exercised Flow's proxy, Office's webhook/BFF and Rust/PostgreSQL together: committed replacement, duplicate receipt, redaction, stale revision refusal, workspace isolation and deletion tombstones all passed with provider calls disabled. This verifies the local receipt pipeline; native provider delivery remains unverified.

Final unit suites passed: voice 541, Flow 394, Office 1,237 and generated client 24. Rust checks passed 15 continuity HTTP tests, 119 assistant HTTP tests, 528 library tests and 72 OpenAPI tests. Types, lint, production builds, generated-client freshness, strict Clippy, SQLx metadata freshness and reversible migration checks passed. All 527 existing catalog operations remain unchanged; the nine new conversation controls are internal.

Native webhook delivery and complete transcript capture under the current privacy settings await the authorized staging deployment and provider configuration. Real audio capture is also unverified. No alternate persistence architecture is enabled automatically. Release verification is recorded below when executed.

Development readback at `2026-11-13T13:17:46.276Z` confirmed the history prompt binding, recording disabled and zero-day retention. Transcripts remain excluded and no workspace webhook is registered. Those settings describe Development; Staging and Production use separate environment callbacks and secrets.

Reproducible checks:

```sh
pnpm --filter @genesy/voice test
pnpm --filter @genesy/office test
pnpm --filter @genesy/flow test
pnpm --filter @genesy/office voice:check
pnpm --filter @genesy/api-client generate:check
pnpm --filter @genesy/e2e exec playwright test history.spec.ts --project=flow-desktop --project=flow-mobile --project=flow-tablet
```

The browser command requires current production builds and the configured mock servers. The provider smoke command below needs an explicitly selected development environment file and incurs provider usage; it validates the agent name before starting synthetic text sessions:

```sh
ASSISTANT_HISTORY_ENV_FILE=/absolute/path/to/development.env node apps/office/scripts/eval/continuity.mjs
```

Provider smoke evidence and pending delivery gates are recorded in `deploy/assistant-validation.json`.
