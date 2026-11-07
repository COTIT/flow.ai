# Assistant acceptance evidence

The inventory, report and Rust-audit commands collect and audit capability evidence without calling ElevenLabs, changing provider limits, sending mail or executing business writes. The separate `native.mjs` driver can start metered provider conversations and request its two reviewed synthetic effects; it remains inactive. The user's later scope correction defers exhaustive capability/model benchmarks and extended experiments beyond the initial release. These detailed matrix reports remain diagnostic and must not be presented as completed acceptance. Existing repository checks, queued safety regressions, packaging and deployment readiness remain required. Physical-device checks belong to the user after deployment and remain untested.

Run from the repository root:

```sh
pnpm --filter @genesy/office exec vitest run scripts/eval
node apps/office/scripts/eval/acceptance.mjs inventory --output /private/tmp/assistant-matrix.json
node apps/office/scripts/eval/acceptance.mjs budget
```

The matrix includes every current catalog operation. Executable and secure UI capabilities require real native transport, authorization, validation and result evidence, plus applicable tenant, restricted-role, farm-role, four-eyes and confirmation checks. Executable tools additionally need unmocked provider/bridge evidence; secure operations need native UI handoff evidence. Internal, external-actor and identity routes retain their explicit dispositions.

Declared unavailable native capabilities retain their eligible rows and required gates. The report lists their reviewed reasons in `unavailableCapabilities` and blocks `releaseReady`. Placeholder transport responses cannot establish real business results.

`fixtures.json` contains 22 read-pilot command templates in English, Dutch and Swahili, one malformed-identifier case, and two explicit product/cash-invoice native recipes awaiting execution approval. Existing Rust list/setup recipes provide source references. Other native fixtures remain missing. Native fixtures must use known synthetic records, exact intended arguments and independently checked outcomes; a generated schema example is insufficient. List shape/size checks establish transport only; the local pilot claims result evidence only for exact known resource IDs. Reviewed semantic list assertions come from the Rust fixtures.

`safety-corpus.json` contains 14 multilingual adversarial scenarios and eight failure-injection specifications. Its `not_run` marker is deliberate. Expected outcomes require actual ledger, response and delivery checks. A model saying it acted safely is insufficient evidence.

## Read-only local pilot

Run only after the operator verifies the API process uses the isolated synthetic database. The database argument records that binding; the HTTP API cannot independently prove its connection string.

```sh
node apps/office/scripts/eval/local.mjs \
  --base-url http://127.0.0.1:3280/v1 \
  --auth-file /private/tmp/genesy-voice-local-auth.json \
  --database genesy_assistant_checks_5dd5c09 \
  --output /private/tmp/assistant-read-pilot.jsonl
```

The private auth file supplies `token`, `workspaceId` and `productId`. Credentials, prompts, raw responses and concrete resource paths never enter output. Only literal loopback HTTP origins are accepted; redirects are refused. Effects and secure UI operations are refused, including effectful GET routes. Each happy read also tests no-bearer denial. This pilot does not establish cross-workspace, restricted/farm-role, native provider or complete capability coverage.

## Existing Rust/Postgres fixtures

The optional `tests/common/contract.rs` hook records requests only after the existing strict status/schema check succeeds. Set `ASSISTANT_HTTP_COVERAGE_FILE` to a new private JSONL path for one coordinated API run. Set `ASSISTANT_HTTP_COVERAGE_CATALOG_SHA256` to the current catalog SHA-256 to detect mixed catalog versions. Leave both unset for ordinary test runs. Serialize Cargo with other agents.

Records contain operation ID, method, status, test-thread identity and contract-validation marker, optionally catalog hash. They contain no URI, request/response, bearer, transcript or business identity. Test output must include standard `test module::function ... ok` lines. Failed and unidentified tests cannot establish passing evidence.

Freeze the pinned sources before a fresh Cargo build and evidence run. Earlier direct-binary results cannot prove assertions added after that binary was compiled.

`assistant_capability_auth::every_effectful_capability_refuses_its_exact_unapproved_action` checks all 218 effectful native routes with their own matching, owned but unapproved action. The API-local effect fixture stays synchronized with the catalog. All217 ordinary claims require exact approval409; only secure key creation requires its exact policy422 and field error. Unchanged proposed state proves refusal only. Seeded drafts bypass proposal/material capture; these checks establish no approval-success, default UI preview, conversational bridge, secure handoff or business result. Multipart bytes, MIME boundary and the three effectful GET routes are included. A passing fresh case and observed per-operation refusal are required before its confirmation-refusal mapping counts.

```sh
node apps/office/scripts/eval/rust-audit.mjs \
  --observations /private/tmp/assistant-http.jsonl \
  --test-log /private/tmp/assistant-api-results.log \
  --output /private/tmp/assistant-rust-audit.json \
  --evidence-output /private/tmp/assistant-rust-evidence.jsonl
```

An observed successful HTTP response in a passed test proves contracted native transport. Result, authorization, validation and isolation gates additionally require a matching review in `rust-reviews.json` or an owner-specific `rust-reviews.*.json`: exact operation, expected status, passed named test and pinned assertion sources. Review notes explain the checked behavior. Source references alone do not pass anything. The shared foreign-row helper does not test collection creation; those create-isolation rows remain open.

When a pinned test/helper changes, re-read its assertions before updating its hash. Do not regenerate hashes merely to silence a stale-review error. A runtime-worker observation without a passing test identity stays unproven. Requests using raw helpers outside `send()` may not appear and need explicit reviewed instrumentation or evidence.

## Native evidence contract

The provider executor must use synthetic records and verify backend outcomes. Read input hashes using `loadInputs()` from `acceptance.mjs`; the dataset hash includes reviewed Rust mappings. Each JSONL record contains:

```json
{
  "catalogHash": "exact current catalog SHA-256",
  "datasetHash": "exact current dataset SHA-256",
  "operationId": "products_get",
  "caseId": "product-get-owned-synthetic",
  "gate": "language",
  "kind": "real_native",
  "model": "actual native model from transcript[].producing_llm",
  "modelSource": "transcript.producing_llm",
  "agentVersion": "conversation.version_id used for this run",
  "agentVersionSource": "conversation.version_id",
  "graphVersion": "immutable graph/application version",
  "runId": "opaque unique conversation-run ID",
  "language": "en",
  "repetition": 1,
  "passed": true,
  "latencyMs": 1234,
  "costUsdMicros": 3000,
  "costSource": "provider_metadata_verified_units",
  "usageSnapshot": "post_end_observed",
  "conversationStatus": "done"
}
```

Example numbers illustrate fields only. Never turn unknown usage into zero. Extract numeric usage and actual model identifiers transiently before provider cleanup; discard transcript and raw conversation details. Read actual models from `transcript[].producing_llm` and the version used from `conversation.version_id`. Configuration `llm_override` does not prove the model used; absent producing-model fields remain unknown.

The provider's `metadata.cost_fiat` is USD, as documented in its [6 July 2026 changelog](https://elevenlabs.io/docs/changelog/2026/7/6). Cost coverage requires an observation after the conversation ends with API status `done`; this establishes observed post-call cost only. The API documents no billing-finality flag or finality deadline. Reports therefore retain `billingFinality: "unverified"` and `costBasis: "observed_post_end"`. Active `usageSnapshot: "before_conversation_end"` values can be zero or incomplete and cannot satisfy this coverage. Independent billing evidence may establish stronger conclusions separately. Provider `cost`/credit fields have no assumed USD conversion. Reusing the same run across bridge and language gates must preserve its identity and measured cost; the report deduplicates it.

```sh
node apps/office/scripts/eval/acceptance.mjs report \
  --evidence /private/tmp/assistant-combined-evidence.jsonl \
  --model exact-model-id \
  --output /private/tmp/assistant-acceptance-report.json
```

The report exits nonzero while capability gates or language coverage remain incomplete. It rejects stale hashes, unknown cases, mixed native models without explicit selection, missing/mixed agent or graph versions, and malformed repetition/language fields. Mocks and schema-only checks cannot satisfy real evidence. Three distinct repetition slots are required per capability/language; duplicate records do not add repetitions, and a failed attempt cannot be overwritten by retrying its slot. Every capability must pass at least once, with success at least 95% overall and per language. Safety acceptance remains a separate 100% gate.

A native session's selected locale and correct product name or amount do not prove language quality. A passing language record needs an independent review of the actual answer or audio in that language, performed transiently without retaining chat, or a separately authorized trusted native evaluation. Do not derive a language pass from the input's `language` or the driver's `completed` flag.

`releaseReady` covers this capability matrix only; `releaseApproval: not_assessed` prevents interpreting it as whole-plan release approval. Cost completeness is reported separately. Select the lowest observed cost among models that pass the full capability, safety, language and image/PDF suites; lower measured latency breaks ties. An incomplete model trial cannot establish the cheapest passing model.

## Cost and scheduling

390 capabilities × three languages × three repetitions = 3,510 scenarios per model. At the dedicated development agent's current 50 conversations/day, that needs at least 71 days, excluding other sessions. Concurrency remains two. The scripts do not increase either limit or run paid bulk traffic.

As checked on 7 November 2026, [official ElevenAgents pricing](https://elevenlabs.io/pricing/agents) lists $0.003 per text message, with LLM usage separate. Exactly one billed message per scenario would therefore cost $10.53 in platform messages alone. Multi-turn/tool conversations, voice, uploads and LLM charges add cost; this is not a complete estimate. First review a fixed synthetic dataset and a small measured pilot, then approve a concrete full-run budget and schedule. The [conversation API](https://elevenlabs.io/docs/eleven-agents/api-reference/conversations/get) exposes `metadata.cost_fiat`; even a post-end `done` observation does not establish final billing.

Compare p50/p95 and observed post-call cost per completed task on the same fixed dataset and environment, retaining unverified billing finality. Establish measured baseline before choosing absolute latency targets. No production SLO or lowest-cost model has been proved by these tools alone.
