# Durable assistant conversations

Conversation history is a member's notebook. An execution session is one signed-in call: it keeps its existing one-hour lifetime, login ownership, action approvals and upload cleanup. Continuing a notebook starts a fresh execution session. Stored dialogue never restores tools, approvals, access tokens, files or executable state.

All member routes start at `/v1/workspace/{workspace}/assistant/conversations`. They require an ordinary bearer token, current workspace access and ownership by both workspace and user. Another member or workspace gets 404. The same user can read history after signing in again; only the original execution login can append provisional events to its source.

## Member routes

| Method | Suffix | Request | Success |
| --- | --- | --- | --- |
| POST | collection | `{title?: string}`; title at most 200 UTF-8 bytes | 201, flat conversation metadata |
| GET | collection | `cursor?`, `limit?` (50 default, 100 maximum), `q?` (200 bytes), `session_id?` | 200, `{conversations, cursor, has_more}` |
| GET | `/{conversation}` | `source_before?` (positive ordinal), `source_limit?` (50 default, 100 maximum) | 200, `{conversation, sources, sources_has_more, next_source_cursor}` |
| POST | `/{conversation}/end` | no body required | 200, flat metadata; closes linked execution sessions |
| DELETE | `/{conversation}` | no body | 200, `{id, state: "deleted"}`; erases message text and summaries |
| POST | `/{conversation}/sources` | signed `{session_id, provider_conversation_id, agent_id, text_only}` | 201, `{id, conversation_id, session_id, ordinal, revision, state}` |
| POST | `/{conversation}/events` | `{session_id, source_id, events}` | 200, flat metadata |
| GET | `/{conversation}/messages` | `cursor?`, `revision?`, `q?`, `source_id?`, `limit?`, `tail?` | 200, exact message page |

Metadata fields: `id`, `title`, `state` (`active`, `ended`), `revision`, `coverage` (`complete`, `provisional`, `partial`), `last_activity_at`, `expires_at`. List order is newest activity first, then id. Its opaque cursor preserves both values. A `session_id` list filter resolves at most one conversation and requires the source's original execution login.

Detail selects the newest source summaries before the supplied ordinal and returns that selection in ascending source order. The complete detail response stays within 64 KiB; a byte-limited selection sets `sources_has_more` and `next_source_cursor`, preserving each included summary in full. Each source exposes `id`, `conversation_id`, `ordinal`, `revision`, `state` (`provisional`, `final`, `partial`), `text_only`, `summary` and `provenance`. Provider, agent and execution identifiers stay out of history reads. Use `next_source_cursor` to read older sources.

## Immutable source attachment

Office first signs the existing `/assistant/sessions/{session}/attach` operation. Source creation requires the same authenticated login, active owned execution session and exact attached provider conversation id. Bridge signatures bind method, complete path, timestamp and raw body. A repeated identical attachment returns the existing source; replacing any binding returns 409. A provider conversation and execution session each belong to one source. A transaction-scoped member/workspace lock rejects any competing live history source with 409, across logins and logical conversations and for both text and voice. Closed, expired, inactive or revoked-login source sessions do not block a fresh source. Legacy execution sessions without a history source stay outside this guard.

A conversation can have several sources, including fresh sessions created after another sign-in. An ended conversation accepts final flushes and canonical snapshots while within retention, but cannot attach another source. Reads and duplicate submissions never extend retention.

## Provisional events

An event is `{key, ordinal, role, text, modality, status, interrupted?}`. Roles are `user|agent`, modalities `text|voice`, statuses `queued|sent_unconfirmed|complete`. `interrupted` defaults to false. Send 1–100 events per request, at most 128 KiB total. Each text is at most 16 KiB; keys are 1–256 bytes and ordinals 0–999999. Batch keys and ordinals must be unique. Unknown fields, including tool or approval data, are refused with 422.

Source, conversation, session and original login must all match. A closed session permits its last flush; a cleaned session cannot append. Exact retries leave revisions and activity unchanged. Delivery state never regresses, and existing event role, modality and ordinal are fixed. New user dialogue refreshes the 90-day inactivity window.

## Canonical ingestion

`POST /v1/assistant/conversations/ingest` requires the bridge signature and no user bearer token. Office verifies the ElevenLabs webhook HMAC over the original bytes before signing this request:

```json
{
  "provider_conversation_id": "conv_example",
  "agent_id": "agent_example",
  "turns": [
    {"key":"turn:0","role":"user","text":"Use blue","modality":"text","interrupted":false}
  ],
  "summary": "Header preference discussed."
}
```

The source is resolved from its stored provider and agent binding. At most 1000 unique turns and 2 MiB per snapshot; text and summary limits are 16 KiB each, with at most 48 KiB after JSON escaping so one record always fits a page. The API removes native six-digit confirmation phrases and recognizable Bearer or `sk-` credentials before hashing or storage. Provisional text uses the same redactor. Event keys containing recognizable credentials are refused. The API stores no raw provider payload.

A transaction locks the logical conversation, removes complete provisional rows, keeps queued and unconfirmed evidence separately, inserts canonical dialogue and updates source/conversation revisions. Retained pending ordinals move to 1000000 plus their original ordinal, after canonical turns. Text is never used to guess which pending message was delivered. Late complete provisional events cannot replace final text. The first canonical digest is immutable: the same normalized snapshot is idempotent; a different one returns 409. Deleted or expired conversations cannot be recreated by delayed events or webhooks.

## Exact message pages

```json
{
  "data": {
    "messages": [
      {"id":"01900000-0000-7000-8000-000000000001","source_id":"01900000-0000-7000-8000-000000000002","key":"turn:0","ordinal":0,"role":"user","text":"Use blue","modality":"text","status":"complete","interrupted":false,"provenance":"final"}
    ],
    "revision": 4,
    "cursor": null,
    "has_more": false,
    "through_sequence": "1:0",
    "gaps": []
  }
}
```

Messages follow source ordinal, turn ordinal and id. `limit` defaults to 50 and is at most 100. Every complete response, including metadata, stays within 64 KiB; a smaller page carries `has_more` and a cursor. Searches are literal, case-insensitive text matches; SQL wildcard characters in `q` stay literal. `tail=true` selects the latest complete messages, excludes delivery-uncertain evidence, then returns ascending order. Its cursor walks toward older messages.

The opaque cursor embeds the conversation and content revision. Supplying a stale cursor or `revision` returns 409; restart from a fresh page. `through_sequence` names the latest available source/turn independent of page, source filter or search. `gaps` lists up to 100 sources lacking canonical final text. Bootstrap callers must also preserve source-directory and byte-budget omissions as incomplete coverage.

## Retention and provider cleanup

History expires after 90 inactive days. Hourly cleanup tombstones up to 100 expired conversations, erases their turns and source summaries, and closes linked execution sessions. Expired history is already invisible before that sweep. Minimal conversation/source identities remain to refuse delayed resurrection and keep provider cleanup recoverable.

The existing minute worker first tries bounded [ElevenLabs conversation details](https://elevenlabs.io/docs/api-reference/conversations/get) for up to 20 ended or expired source sessions. It verifies conversation and agent identities and terminal `done|failed` state; whitelists dialogue, interruption and text/voice provenance; excludes contextual updates, platform events and tool payloads; then uses the same canonical ingest transaction. Missing, processing, malformed or oversized responses remain retryable. Source GET attempts are claimed before network I/O; never-attempted work runs first, then the oldest attempt. DELETE attempts use the same rotation on execution-session cleanup metadata, so failing early rows cannot starve later sources or legacy sessions.

A source-bound provider conversation becomes deletable when its execution control is due and a final snapshot exists, or after one additional hour of archival grace. Legacy sessions without a source retain their existing cleanup schedule. GET failure never confirms archive completion. DELETE failure keeps references for retry; only 200, 204 or 404 confirms provider deletion. A confirmed deletion without final text marks source coverage `partial`. A delayed valid final snapshot may still fill that gap while the logical conversation remains within retention. Tombstoned content remains erased.

## Errors

401 missing/invalid bearer on member routes; 403 invalid/missing bridge proof or denied workspace access; 404 unknown/foreign history or source; 409 competing live history call, ended attachment, changed immutable identity, stale content revision, conflicting final snapshot or deleted/expired ingestion; 413 request byte limit; 422 invalid fields, event/turn count, text size or cursor. Shared overload, timeout and internal error responses follow the API's normal envelope.
