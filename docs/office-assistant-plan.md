# Office assistant implementation plan

Approved by the user on 7 November 2026. Implementation and coordinated parallel agents authorized. This document records the approved scope and release gates; checkboxes describe verified progress only.

User update, 7 November 2026: complete remaining implementation and automated acceptance before pushing main and deploying to production. The user will perform physical Android/Chrome and iPhone/Safari microphone, speaker and camera checks manually after deployment. Those hardware checks are deferred to the user; they are not recorded as passed. Production has no users yet. This changes only the timing/ownership of physical-device acceptance; all other automated, safety, capability and release gates remain required.

Later scope correction, 7 November 2026: the user directed us to stop scope creep. Finish the existing implementation and the checks already queued; add no further features, test matrices or exhaustive model benchmark. The initial release remains voice and text, image/PDF references, existing backend tools, LangGraph/LangSmith and independent action approval. Existing Rust/web checks, current safety regressions, packaging and deployment readiness still must pass. Broader per-capability evaluation, model cost comparisons and extended load experiments are deferred beyond this initial release. Three existing backend stubs remain explicitly unavailable; implementing those missing backend features is outside this integration. Coverage audit gaps stay recorded without being presented as passing tests.

## Product decisions

- ElevenLabs handles both voice and text conversations and tool selection. No separate OpenAI integration or key.
- During live voice, users can type and share JPEG, PNG, WebP and PDF references without ending the call.
- Support every Office-authorized business and admin capability. Credential entry/reveal, PINs, login and device pairing use secure native UI handoffs.
- Genesy owns permissions, confirmation and execution. Direct operations call existing Rust routes; LangGraph coordinates durable tasks. LangSmith traces and evaluates execution.
- English, Dutch and Swahili must pass acceptance.
- Conversation persists across Office navigation. Workspace switch and logout end it.
- Reads execute directly. Effects require confirmation unless user explicitly authorizes one fully specified action or fixed batch. Missing details, changed scope, inferred defaults and ambiguous consent require review.
- Chat and reference content stay temporary. Resolved operational action audit lasts 90 days; unresolved actions remain until reconciled. Business records keep existing retention.
- Use minimum available provider retention and verify actual deletion behavior.
- Apply interface-kit and existing Office Frosted mist patterns/components. Caveman Ultra and Ponytail Full remain active.

## Architecture

Office voice/text/attachment UI connects to one native ElevenLabs agent. An authenticated Genesy bridge exposes a shared capability registry. Simple operations use existing Rust HTTP routes; LangGraph handles dependencies, review/resume and recovery. PostgreSQL stores graph checkpoints and an authoritative Rust action ledger. Existing Rust jobs handle postcommit work and provider cleanup. No separate Node worker or generic offline action executor.

Server-issued sessions bind authenticated user, source login, workspace, environment and configured agent. Model tools expose discovery, schema lookup, preparation, execution requests and status. A separate trusted confirmation endpoint approves immutable actions; it is never a model tool.

Stable task/step/action IDs bind operation, canonical arguments, attachment references and review version. Outcomes distinguish awaiting review, running, completed, queued, partial, failed, cancelled and unknown. Checkpoints contain references and safe progress; the Rust ledger proves approval and execution state.

## Phase 1: coverage and baseline

- [x] Classify all 431 existing OpenAPI operations, including 390 staff business operations, plus 25 internal assistant controls: direct read, confirmed effect, secure UI handoff, separate actor flow or infrastructure.
- [x] Record actual authorization, effects, media, sensitive fields, retry guarantees and reconciliation. Replace permission-prose regex, JSON-only filtering and method-based effect inference. Final catalog pins 205 reviewed source files.
- [x] Record and fix existing browser failures before release evidence. Updated-main full Office run: 840 cases, 832 passed, eight expected skips. Mobile glass-padding and CSP expectations are corrected.
- [x] Gate every new API operation on explicit classification; internal assistant endpoints cannot become self-approval tools.

## Phase 2: shared tools and transport

- [x] Generate contracts from backend OpenAPI plus reviewed authorization/effect metadata. Reject unknown arguments, authority overrides and executable URLs.
- [x] Support JSON, empty 204, multipart business uploads, artifacts, paging and async results.
- [x] Discover compact relevant operations, then exact schemas; use existing tenant-scoped search and clarify ambiguous entities.
- [x] Preserve exact money, decimal quantities, units and API-calculated totals.
- [x] Route secret-bearing tasks through native secure UI. CI rejects stale contracts or missing dispositions.
- [ ] Cover each eligible capability's transport, authorization, validation and result behavior.

## Phase 3: durable action safety

- [x] Replace filesystem journal with PostgreSQL action ledger and durable graph checkpoints.
- [x] Bind approval to actor, login, workspace, operation, exact request, material facts, version and expiry. Default approval TTL: 120 seconds.
- [x] Rust guard atomically claims approval before forwarding to existing handler. Normal endpoint authorization remains active.
- [ ] Verify state-dependent approved facts inside service transactions/locks; a generic preflight GET cannot provide that guarantee. Earlier order, stock, mail and invoice regressions passed, including drift beyond a large invoice's visible preview. Review then reproduced missing product/variant currency/VAT defaults, invoice-create defaults and invoice-update selected recipient/billing/line defaults. Shared native resolver fixes and expanded regression validation are in progress.
- [ ] Extend existing transactional idempotency/correlation where duplicate-producing actions lack recovery evidence.
- [ ] Fixed batches contain ordered approved actions; failure or unknown pauses remainder. Every later dispatch rechecks live authorization.
- [ ] Already admitted action may finish after disconnect; future steps pause. Lost acknowledgement remains unknown until evidence resolves it; no blind replay.
- [x] Repair mail status/action IDs, invoice partial outcomes, durable postcommit delivery and omitted-settings credential preservation.

Guard guarantees once-only dispatch, not exactly-once external delivery. Request/ledger finalization failure after commit can leave unknown. Preserve that distinction in UI and traces.

## Phase 4: image/PDF references

- [x] Allow JPEG, PNG, WebP and PDF only. Explain unsupported HEIC, Office files, archives and video.
- [x] Enforce 10 MiB/file, 10 uploads/session, two simultaneous uploads, and any stricter provider limit with shared atomic reservations.
- [x] Upload through authenticated Office proxy; inspect byte signatures and actual byte count. Bounded transient buffers only; no permanent Genesy file, S3 object, OCR stack or temporary disk copy for analysis.
- [x] Persist ownership/quota/provider-reference/cleanup metadata only. Never persist attachment bytes, names, transcript or extracted text in audit/checkpoints.
- [ ] Configure primary and explicit backup models with verified image and PDF input plus tool support; incapable fallback cannot silently ignore attachments.
- [x] Upload first, then send native multimodal event over active voice/text connection. Separate uploaded, submitted and inspected evidence; SDK send returning void is not receipt. Actual Dutch PNG/PDF inspection passed.
- [ ] Report unreadable/corrupt/encrypted files. Clarify uncertain numbers, identities and amounts; reference exact attachment/page where supported.
- [ ] Treat attachment text as untrusted data. Sharing for inspection does not authorize business save, email, publication or filing.
- [ ] Separate reviewed save/email actions use existing business-file and destination permissions. Browser can reuse original Blob during session; loss of memory requires re-upload.
- [ ] Remove unused uploads when supported. Used-file chip removal cannot erase model context. End/clear conversation initiates provider cleanup; retries survive restart and abandoned-session expiry.
- [ ] Verify provider whole-conversation deletion and retention scope before making deletion claims.

## Phase 5: Office-native UI

- [x] Stable signed-in shell provider owns conversation across navigation; /voice is a view. Persistent status, mute, stop speaking, end and open-conversation controls.
- [ ] Live voice accepts typing and attachments. Text-only starts without microphone. Transport change when enabling voice preserves verified task progress and visible context.
- [x] Reuse PageHead, Bento, Tile, SubTile, Notice, Table, Field, responsive Dialog and current palette/type/theme tokens.
- [ ] Attach/Send composer, native desktop/mobile picker, optional device camera. Preview tray shows stable ordinals, thumbnails, names, sizes, actual status and individual retry.
- [ ] Camera/audio interruption reflects real microphone state. Mobile controls remain reachable with keyboard open and safe-area spacing.
- [ ] Canonical review shows workspace, target, before/after, quantity, money, recipient and consequences. Edits invalidate prior approval.
- [x] Spoken approval uses finalized user events through trusted browser handler and the current native card's exact localized six-digit challenge. The app never proactively includes its code or phrase in model tools/context. A phrase spoken by the user passes through provider ASR and may appear in the user transcript; the trusted finalized-user-event latch and once-only Rust claim reject agent echoes and replays. Generic yes never approves. Explicit UI approval remains available.
- [ ] Refresh existing query caches after verified mutations. All labels and errors translated en/nl/sw; keyboard, screen reader, 44px targets and reduced-motion behavior covered.
- [ ] Strands: unchanged vendor source wrapped in existing pattern; retain license; small navy/gold/teal emblem, three strands, extra glass pass disabled. Lazy/static fallback; stop when hidden/idle/ended; measure mobile audio performance.

## Phase 6: privacy, observability and cost

- [ ] Separate dev/staging/prod credentials and LangSmith projects; secrets runtime-only through secret files.
- [x] Trace opaque IDs, operation, stage, latency and verified outcome. Exclude credentials, transcript, files, filenames, extraction and raw business responses from exported telemetry. Actual LangSmith trace showed hidden inputs/outputs and a verified completion.
- [ ] Keep only required canonical approval evidence during pending execution; audit retains hashes/references/projected outcomes. Purge resolved audit after 90 days.
- [x] Disable recordings, configure minimum provider retention and verify actual deletion scope. Trace failure does not block business actions.
- [ ] Measure minutes, billed messages, uploads, model tokens and cost/completed task. Pin lowest-cost native model passing full tool/image/PDF/language suite; lower measured latency breaks ties.

## Phase 7: validation and release

- [ ] Strict TDD: observe meaningful failure first, then smallest passing change. Run required Rust/API/client and web checks; update contracts/changelog/docs.
- [ ] Real Rust/PostgreSQL coverage for every eligible capability/handoff, cross-workspace denial and restricted/farm role/four-eyes checks.
- [ ] Test exact money, stock races/recalled lots, changed permission, expired/stale/replayed approvals and ambiguous consent.
- [ ] Inject crash around admission/commit/acknowledgement, simultaneous confirms, restart/checkpoint loss and uncertain external delivery.
- [ ] Attachment tests: spoofed MIME/length, size/type boundaries, encrypted/corrupt files, duplicate names, concurrent caps, failed/uncertain sends, stale handles, upload/end races and restart cleanup.
- [ ] Desktop/375px Playwright across locales/themes: navigation, live typing/upload without reconnect, camera return, keyboard/screen reader and text-only without microphone. No unexpected console/network failures.
- [ ] Prove unmocked ElevenLabs to bridge/graph to Rust/PostgreSQL and truthful spoken/UI result in isolated staging.
- [ ] Deferred by the later scope correction: three text repetitions of canonical commands per eligible capability/language and exhaustive model comparison. Keep the existing native checks and queued safety regressions; do not claim the deferred benchmark passed.
- [ ] Additive reversible migrations, restricted checkpointer role, explicit readiness, immutable tested images, pinned agent/model/catalog/graph/dataset evidence.
- [ ] Real-stack and voice acceptance failures block release. Rollout limits users/workspaces while retaining full capability set.
- [ ] Rollback disables admissions, reconciles executing actions and restores compatible app/agent while preserving business data.

## Readiness refinements

- Request signed URL with provider-allocated conversation ID; verify actual response contract, bind server-side before exposing URL, then match SDK handshake. Fail closed on missing/mismatched ID. Never trust arbitrary browser conversation ID for upload/delete.
- Sensitive bridge calls use shared server HMAC secret plus normal user authentication; model cannot forge approval or bind another provider conversation.
- SQLx owns production DDL, including pinned checkpoint-saver tables/version rows and grants. Runtime restricted role never calls setup. CI compares schema to exact saver version in disposable DB.
- Disaster recovery bootstraps restricted role identities before restoring ACLs; test restored grants and denied business-table access.
- Rollback compatibility accounts for business migrations and saver/graph versions. Routine assistant rollback never restores entire business database.
- Clean stack/image tests verify all three audio worklet assets and standalone runtime dependencies.

## Approved dependencies and remaining evidence

Approved additions: PostgreSQL LangGraph checkpointer for persistence and compatible JSON Schema validation, including required driver/types and format support. The approved OGL dependency was removed because the unchanged official Strands source uses native WebGL. Reuse existing SDKs, PostgreSQL, Rust jobs and UI.

Unchecked at approval: live multimodal delivery, provider/fallback capabilities, attachment deletion/retention, multilingual accuracy, mobile Strands performance, production capacity and full baseline. These remain release gates.
