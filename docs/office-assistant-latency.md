# Office assistant latency

How long the Office voice assistant takes between a spoken request and its answer, measured on the local dev stack. Times come from LangSmith project `genesy-office-voice-dev` (inputs and outputs hidden). The ElevenLabs side (speech-to-text, model, text-to-speech) is in its own section below.

## Office side

### What each span measures

Each voice route request is one root span with child spans:

| Span | What it covers |
| --- | --- |
| `genesy_assistant_tools` / `_confirm` / `_batch` | The whole Office route request |
| `session_lookup` | Rust `GET …/assistant/sessions/{id}?with=identity` (login, workspace, permissions, farm roles and session in one call) |
| `authorize` | Fallback only: the three `/me` checks for an API without the identity read |
| `propose` | Rust `POST …/assistant/actions` (the draft) |
| `review_target` | Rust lookup of the record's display name for the review card |
| `review_ticket` | Signing the review ticket (local, no network) |
| `genesy_assistant_review` | Checkpointing the approval pause in Postgres |
| `checkpoint_lock` | Taking the per-session advisory lock connection |
| `checkpoint_state` | Reading the checkpoint before a write to the graph |
| `approve` | Rust `POST …/actions/{id}/approve` |
| `genesy_assistant_execute` | Resuming the graph and running the business call |
| `execute_backend` | The Rust business call itself |

### Before and after

BEFORE = commit `c53025642` (spans only). AFTER = branch tip. Medians (p50) over the runs listed under Method.

| Span | Before | After |
| --- | --- | --- |
| _pending measurement_ | | |

### Changes

- Login + session: 4 sequential-then-parallel GETs became 1 (`lib/voice/identity.ts`). No caching across requests. Older APIs fall back to today's checks with no extra cost.
- Checkpoints: resume writes use LangGraph `durability: "async"`, so checkpoint writes overlap the Rust call. Each run still waits for every write before it returns (`workflow.test.ts`). The pool keeps one warm connection (`min: 1`, idle 60 s, max 4 unchanged), and readiness pre-connects it.
- Kept on purpose: the checkpoint read before resume. In LangGraph a failed resume leaves its value pending, so a stale ticket would block the real review for good. The read refuses mismatches before anything runs.

### Fewer model turns

Before, a read cost three tool calls (search, discover the exact operation, execute). Now the model may call a list/get operation directly. If its arguments are wrong, the reply carries the exact input schema; if the id is a guess, it carries up to 8 permitted candidates.

| Fixture | Tool calls before | Tool calls after |
| --- | --- | --- |
| _pending measurement_ | | |

### Method

_Pending: local dev stack, text-only conversations in the browser, spans read back through the LangSmith API._

### Not verified

_Pending._

## ElevenLabs side

Measured 2026-11-08 on the **Staging** agent (`agent_8401m6t5sahxf1wbbmsy01jkeqhw`, "Genesy Office · Staging") through https://staging-office.genesy.app/voice (AFM Shop, voice mode, microphone muted, typed requests). The Development agent was only read, never changed. Staging uses the Staging agent: the provider WebSocket opened with that `agent_id` on every conversation.

### What the dashboard exposes

- The agent has conversation retention 0 days with transcript and audio deletion on. The conversation history is empty after each call, so the dashboard's per-turn LLM time-to-first-byte, TTS time-to-first-byte and tool durations cannot be read. The only aggregate shown is the Spotlight "Response Time" (0.96 s, 7 days).
- The LLM picker lists latency ranges per model. Examples: GPT-6 Luna 1.01–5.04 s, Gemini 3.1 Flash Lite 656 ms–1.2 s, Gemini 3.5 Flash 951 ms–2.24 s, GPT-4.1 Mini ~660 ms. `gemini-2.5-flash` is no longer offered as a primary model; it only remains in the existing backup list.
- Baseline settings: LLM `gpt-6-luna` (temperature 0.1, reasoning effort default, service tier Standard, backups `gemini-2.5-flash` and `gpt-4.1-mini`, cascade timeout 4 s), TTS family V4 Turbo ("Fastest") with Expressive mode on, Turn V3 with speculative turn already on, eagerness Normal, take turn after 7 s silence, ASR Scribe v2 Realtime.
- There is no separate "optimize streaming latency" setting in this UI. TTS already runs on the fastest family, so the only TTS-side change left to test was Expressive mode.

### Method

Timings come from the browser: a wrapper around the page's WebSocket timestamps each provider message. Milliseconds are counted from the typed `user_message`:

- **first tool call**: time from the user message to the agent's first client tool call (first LLM turn).
- **LLM turn after tool**: time from each tool result to the next tool call (each later LLM turn).
- **final**: time from the last tool result to the first audio chunk (last LLM turn plus TTS first byte; the two cannot be split client-side).
- **total**: time from the user message to the first audio the user hears for the answer.

Each conversation ran three fixtures from `apps/office/scripts/eval/fixtures.json` in English: "Show the first five products", "Show the first five orders", and `native-product-create` up to the review card. No review was approved. Each card was cancelled or left to expire.

### Results (medians, ms)

| Variant | Settings | Convs | Requests | First tool call | LLM turn after tool | Final (LLM + TTS) | Total to first audio | Tool calls per request | Failed requests |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| a | `gpt-6-luna`, Expressive on (baseline) | 4 (1 void) | 10 | 1438 | 1330 | 2674 | **8034** | 3 (discover, discover, execute) | 0 |
| b | `gemini-3.1-flash-lite` (reasoning `minimal`), Expressive on | 2 | 6 | 960 | 869 | 987 | **3816** | 2 (discover, execute) | 2 of 2 orders requests |
| c | `gpt-6-luna`, Expressive **off** | 3 | 9 | 1703 | 1656 | 2643 | **9562** | 3 | 0 |

Office tool times (C's side; listed only to show the split): discover ~220 ms, read execute ~450 ms, write review preparation ~1.3–1.4 s.

For variant a, a typical read request breaks down to about 1.4 s for the first LLM turn, two later LLM turns of about 1.3 s each, about 0.9 s of tools, and 2.7 s for the final turn plus speech. **Four LLM turns account for about 6.5 s of the 8 s.** No pre-tool speech played in any variant (`pre_tool_speech: "auto"`), so the user hears silence for the full 6–10 s.

### Quality guard

- **a**: right operation every time. Product names, prices and order totals match the data. The review was explained correctly (name A, 220, 200, not created yet, read the phrase or use the button). Once, it made an extra `discover(assistant_task_status)` call before speaking, adding ~1.8 s.
- **b**: much faster, but skips the second discover (the operation's full schema). `orders_list` then failed on both runs ("unable to retrieve the list of orders"). Writes reached the review card, but it said "I have initiated the creation" before review. `[happy]`/`[sad]` expressive tags showed up in the transcript text. **Fails the quality guard.**
- **c**: same behaviour as a, no faster. Within noise, it was 1.5 s slower; this was the last block, run at a different time. In one create run it went straight into `assistant_task_status` `wait:true` without explaining the card and stayed silent until the review was cancelled; afterwards it reported the cancellation correctly. Expressive off gives no measurable latency gain.
- The Office review card for `products_create` showed currency-looking fields (Code USD, Decimal places 2) and not the product name or price. Forwarded to the Office side.

Conversations used: **9 of the 20 budgeted** (cap 12). Variant d (turn eagerness/timeouts) was not run: typed input bypasses turn detection, so it needs a person speaking.

### Recommendation

Keep `gpt-6-luna` and Expressive mode as they are. Instead, remove one LLM turn per request: let `discover_genesy_tools` return the full input schema of its top match, so the agent can call `execute_genesy_tool` right after the first discover. Expected gain is about 1.5 s per request (one ~1.3 s LLM turn plus ~0.2 s tool), from about 8.0 s to about 6.5 s, with no model change.

The trade-off is a larger discover result, meaning more input tokens on every turn. The schema could also be wrong when the top match is wrong. Variant b shows both sides: it skipped that second step and was 4 s faster, but without the schema it built a broken `orders_list` call. The `orders_list` fixture must pass before the change ships.

No model or TTS switch was applied. Untested option: the LLM "Service Tier: Fast" setting, listed at twice the price.

### Restore

The Staging agent was restored to its pre-test settings and published as version `agtvrsn_4701m6wp0cqpfbwbhpzmz4bsv75p`: `gpt-6-luna`, reasoning effort default, Expressive on, unchanged backups, turn, ASR and privacy settings. All three settings views (Agent, Agent voice, Settings) were re-read and matched the pre-test snapshot. Publish shows no pending draft. Intermediate published versions: variant b (`gemini-3.1-flash-lite`) and variant c (Expressive off). The Development agent (`agent_6501m6rwswxyfe1tca1t30b8k0ta`) and the Production agent were not modified.
