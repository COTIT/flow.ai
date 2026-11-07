# Office assistant validation

Development evidence collected on 7 November 2026. This is not release approval. The approved scope and remaining acceptance gates are in [the implementation plan](office-assistant-plan.md).

## Workspace

Branch `codex/office-voice-agent` uses `origin/main` at `ffcf5c8b8`. A later fetch found five additional category/packing commits; these were integrated after pausing all child edits. All 213 saved task files were preserved, independent locale/changelog/navigation conflicts were resolved, and Office typecheck passed. All assistant changes are in the isolated worktree; the original checkout remains clean.

Live checks used synthetic staff and business records in the local `genesy_assistant_checks_5dd5c09` database. Tokens and provider credentials stayed in ignored private configuration or temporary files, never this document.

## Verified integration

| Check | Evidence |
| --- | --- |
| Native English tool execution | Repeated against the final backend/catalog. Real ElevenLabs conversation discovered and prepared `orders_create`. Exact review matched one synthetic product, quantity `1`, displayed price €1.25 and authoritative current facts. Independent confirmation created the draft through Rust; graph/audit recorded completion and the native agent reported the result through `assistant_task_status`. The private confirmation code stayed out of model results. |
| Native Dutch image/PDF inspection | Two authenticated temporary uploads reached the actual provider. The model identified a red PNG and the PDF's six-digit reference correctly. Business actions were disabled for this scenario. |
| Native Swahili audio | Provider produced Swahili speech. Its synthetic greeting, held only in memory and fed back as PCM, produced a matching finalized user transcript. No business actions ran. This checks the speech protocol; it does not validate a physical microphone or speaker. |
| Session identity | Signed URL included a provider-allocated conversation ID, bound server-side before connection. Native handshake IDs matched the Rust-owned session. |
| Provider cleanup | Rust marked the earlier seven smoke sessions and both temporary uploads cleaned. The final action smoke's session also reached `cleaned` with its provider reference cleared. Its completed action had no remaining proposal or material facts. The worker accepts cleanup only after successful provider deletion or confirmed absence. |
| Checkpoint storage | Restricted role persisted an approval interruption, read it through a new saver and resumed it. Business-table reads and runtime DDL were denied. Separate connections could not hold the same advisory lock. |
| Readiness | Actual provider primary/fallback image/PDF capabilities, file limits, restricted DB schema and local API health passed. |
| LangSmith | The actual development project displayed `products_get` with `verified_status: completed` and opaque action/session IDs. The trace showed “No inputs” and “No outputs”. Verified through the logged-in UI; the runtime key retained write-only access. |

The acoustic replay check also exposed an approval hazard: a model given the complete confirmation phrase could speak it into the microphone path. Office keeps the challenge on the native card and in its browser matcher, excluding it from model tools and contextual updates. When the user personally speaks approval, ElevenLabs ASR necessarily processes that utterance and the agent may receive its transcript. The browser latches the first approval before awaiting the server; subsequent echoes and replayed events cannot dispatch the same action again. Model tool results contain only status, action ID and review instructions.

## Automated checks

- Full Rust API integration run: 1,341 passed, six ignored, zero failures. This includes durable invoice delivery, direct-mail recipient checks, material preconditions and retention. The later 1,000-line invoice HTTP case also passed: a changed hidden tail refused dispatch, and a fresh review succeeded.
- All 445 Rust unit/bin tests and all 59 OpenAPI tests passed, including full-material invoice digest binding, bounded preview, private projection and secret masking.
- Final strict clippy, SQLx preparation/freshness, Rust formatting and documentation checks passed. The six ignored API tests are existing load benchmarks. Independent final safety review found no material defects in approval binding, public fact projection, cleanup cadence, parallel preparation or trace redaction.
- Typed mail receipts: five Rust regressions passed for accepted, blocked, manual, recipient-free and approved GET email testing. Office projection tests cover every invoice receipt status for both send and resend, and mail receipts for send and profile testing.
- Voice browser suite: production build and 32 desktop/375px cases passed, including all three locales, text/voice startup, PDF sharing, multipart business save, navigation, reload recovery, secure handoff, session revocation and spoken confirmation. Provider/API connections are mocked in this suite.
- Full Office browser baseline before the later five main commits: 812 cases yielded 801 passes, eight viewport skips and three failures. Exact CSP test expectations and mobile table padding were corrected. Rebuilt affected suites passed 77 cases with one viewport skip; all observed failures are covered by those reruns. The new category changes require their final affected browser checks.
- After keeping approval codes private, production build and all 32 voice browser cases passed again. Tests reject nonce leakage in every client-tool result/contextual update while retaining actual-user spoken approval.
- Updated-main full Office browser run: 840 cases, 832 passed, eight expected skips and zero unexpected failures. Typecheck and ESLint passed. Later native invoice receipt changes have their own affected rerun.
- Final Office unit suite: all 160 tests in 24 files passed. Catalog generation and freshness passed for 448 operations: 375 executable, 15 secure UI, 17 internal, 33 external actor and eight identity operations. All 180 referenced authority-source hashes match the reviewed code.
- Final production build and 34 desktop/375px voice cases passed. Parallel native preparations cannot replace the active review, while independent task-status reads still work. Office/e2e typechecks and lint passed.
- Tracing tests use the actual SDK with mocked exporter failures: hidden inputs/outputs, bounded calls, shared initialization across route bundles and once-only business callbacks passed. A failing graph-node test exposed the SDK's original-error fallback when the anonymizer returned `{}`. Returning a generic error string fixed it; all ten tracing/workflow cases passed, including error/stack redaction without replay.
- Updated-main order/stock preconditions: five HTTP cases and three context cases passed, including stale-price/stock/order refusal and read-protected write-only roles. Six retention SQLx cases passed, including the exact deadline, unresolved actions and pending remote references.
- Native invoice caller: 14 helper tests and 102 affected desktop/375px browser cases passed. Queued, manual, failed and uncertain delivery no longer display a false sent confirmation.
- Generated API client and the repository's exact freshness command passed against the final native OpenAPI. All web packages typechecked again; client tests passed 24/24, shared security tests 28/28, and Office ESLint passed.
- Deployment compatibility, role, compose, health and backup/restore mock suites passed. The exact eight migration checksums are frozen. Compatible rollback rejects pre-assistant or unversioned invoice workers, including targets with an unchanged schema.
- Actual PostgreSQL 18 dump/restore in disposable containers passed. Restored checkpoint CRUD worked; business-table reads, persistent DDL and migration-metadata writes were denied. Ten role connections succeeded and the eleventh was refused. Existing development databases were untouched.
- A standalone Office build from a secret-free temporary source context ran after relocation, with no dependency fallback to the workspace. Health/login/readiness, disabled write endpoints, all three byte-identical SDK worklets and 27 client chunks passed. This was a macOS artifact check, not a Linux memory-limit test.

The sanitized [agent template](../deploy/assistant-agent.template.json) records the observed development prompt, tools, model/fallback configuration and privacy controls. The [machine-readable validation record](../deploy/assistant-validation.json) pins its observed provider version and template hash. Staging/production bindings and final release image/commit remain unset.

## Implementation boundaries

One immutable review is active at a time. Fixed batches now prepare two through ten resolved actions under one independent review; typed queued/partial/unknown/failure stops later dispatch. Final Rust and native release validation remains pending. Text-to-voice and camera return establish a fresh verified provider binding while preserving visible browser history and owned task references. Previous attachment receipts are cleared and original browser-held files require explicit re-sharing. Automated continuity tests passed; physical microphone, speaker and camera checks are user-owned after deployment.

The real native checks used synthetic local records. They do not establish complete capability/language coverage, the cheapest passing model, or staging acceptance.

## Pending release evidence

- Run the complete per-capability multilingual acceptance matrix, failure injection and cost/latency measurements. The development agent currently retains its daily/concurrency limits.
- Physical Android/Chrome and iPhone/Safari microphone, speaker and camera checks are user-owned after production deployment. Automated browser/accessibility and animation checks remain predeployment gates.
- Final packaged container and 384/512 MiB capacity evidence remains required. The public image fetch recovered. A preliminary secret-free Linux build and two-container-per-limit upload/read/admission-restart tests passed without OOM. That source predates final batch and money-review changes; stronger pool saturation and postcommit acknowledgement faults still need execution. Actual restored grants and focused rollback checks passed. No staging or production deployment has occurred.

Latest automated UI: the full Office baseline passed 878 cases with eight expected skips (886 total). The latest isolated production build and all 62 desktop/375px voice cases passed, including translated task outcomes, Dutch unavailable guidance, navigation recovery and private spoken consent. Office typecheck and affected-source ESLint passed. Final native contracts and generated metadata still require their combined source validation.

Live synthetic product/payment fixture execution is pending explicit user authorization after automatic approval review rejected provider-backed write infrastructure. Neither rejected helper patch was applied; no paid sessions or synthetic writes ran through it.

Rollout admission: six meaningful failing cases reproduced unrestricted or malformed workspace scope. The explicit workspace allowlist, native read-only status alias, cancellation preservation and per-step batch refresh passed 33 focused tests; combined related evaluator/controller tests passed 75. Office and E2E TypeScript checks plus affected-source lint passed. A final production build after all native contract changes remains required.

Independent review found an invoice-update gap involving selected contact, relation billing and default line values. The shared native resolver/lock fix passed source re-review; fresh expanded runtime checks remain required. A later order-to-invoice gap reproduced four meaningful failures and its native shared-builder fix is awaiting fresh execution. Refund acceptance, atomic receipt correlation and settlement reconciliation are also under final validation. These source reviews do not establish runtime safety.

Capability evidence review now contains 513 operation-specific assertion rows: all 390 eligible operations have domain reviews, and 387 have semantic result mappings. Three existing product stubs remain explicitly unavailable. New assertion rows are source reviews only until a fresh, matching Rust run and HTTP observation audit passes.

Development agent prompt updated for fixed batches: prepare two through ten fully resolved effects, obtain independent native-card approval, then wait through the correct action or batch status operation. The new provider version was read back and all other conversation/platform settings matched. Snapshot integrity regression first failed on the missing batch contract, then passed. Earlier live native checks belong to the previous version; no new paid session has run.

Refund display: two regressions reproduced exposing native minor-unit amount 1250 as an ordinary amount for a 12.50 request. The BFF now projects refund ID/status only; the exact amount remains on the independent review card. All 35 BFF projection tests passed. All six English/Dutch/Swahili desktop/375px refund browser cases passed, including queued status, private exact-money review and no ambiguous raw minor-unit display. Provider traffic was mocked.

Scope frozen by user direction: no further features, test matrices or exhaustive model benchmark. Existing native/web checks, current safety regressions, packaging and runtime readiness remain required. Detailed coverage gaps are diagnostics; they are not passing evidence. Existing backend stubs remain explicitly unavailable. Earlier proposed metered fixture experiments stay inactive and are not part of the initial release.

Final focused native repairs passed: app-key revocation now supports exact reviewed DELETE while creation stays in the secure form; unapproved and replayed requests refuse. Exact recursive PIN rejection prevents PIN-bearing proposals from being stored, while ordinary pipeline/shipping/fingerprint fields remain valid. Native library tests passed458 and OpenAPI tests passed64; the full current API suite subsequently passed 1,393 tests with six existing ignored cases and no failures.

Secure-form development regression: both desktop/375px cases reproduced a submitted result being lost under Next Strict Mode. Resetting the mount guard on effect setup passed both cases. The same secure-handoff result assertion remains in the production browser suite, and CI now runs the focused development check.

Frozen release checks: 1,393 API tests passed, six existing benchmarks ignored; 458 native unit tests and 64 OpenAPI tests passed. Strict clippy, SQLx freshness and formatting passed. The current Office suite passed 300 tests, the generated client passed 24 tests and exact freshness, all web packages typechecked, and final production voice/refund browser checks passed 68 cases. Development Strict Mode checks passed 2 cases.

The final sanitized HTTP audit observed all 390 eligible operations in passing native tests and matched 387 semantic results; three pre-existing backend stubs remain explicitly unavailable. These figures do not claim that every detailed audit gate or a live model benchmark passed.

The frozen Linux Office image built without private environment files. Its 384 MiB packaging smoke passed health/login/readiness, six disabled admission routes and three byte-identical SDK worklets, with no OOM. This was a packaging check without credentials, provider conversations or business writes; extended load experiments remain deferred.

Production preparation: a separate authenticated agent now matches the verified development conversation/platform settings. Private bridge/checkpoint credentials and a single-workspace allowlist remain on the production server, with admission disabled. The initially authorized external integration keys/tracing project are temporary and reused for first deployment; rotation and environment-specific key separation follow the user’s requested post-verification rotation. No production conversations have run.
