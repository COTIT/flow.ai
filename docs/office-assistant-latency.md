# Office assistant latency

How long the Office voice assistant takes between a spoken request and its answer, measured on the local dev stack. Times come from LangSmith project `genesy-office-voice-dev` (inputs and outputs hidden). The ElevenLabs side (speech-to-text, model, text-to-speech) has its own section below, owned by session E.

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

## ElevenLabs

_Section owned by session E._
