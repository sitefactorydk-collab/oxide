# Oxide — Progress Log

The current phase and hand-off notes live here. Every phase chat reads this file first and appends to it last (template in PLAN.md §4).

## Status
- **Last completed:** Phase 3 — LLM layer & streaming chat (manual exit checks against all four providers: see Phase 3 below)
- **Next phase:** Phase 4 — Chat UX completeness
- **Repo location:** `C:\AI Apps\Oxide` (git, branch `main`)

## How to build and verify (read this first, every phase)
- **Where builds run:** the Linux VM behind the desktop link has no Rust, and its proxy blocks crates.io and npm. **All builds run on Windows.** The agent writes files through the link, the user double-clicks a script in `scripts\win\`, and the agent reads `logs\*.log` back.
- **Computer use doesn't help here.** It returned blank screenshots on this PC, and Explorer/PowerShell are click-only for the agent anyway. Don't spend time on it. Ask the user to double-click instead.
- **Scripts** (see `scripts\win\README.md`):
  - `dev.cmd`: app with hot reload. Logs to `logs\dev.log`.
  - `check.cmd` / `check.ps1`: cargo fmt, prettier, clippy `-D warnings`, cargo test, typecheck, lint and vitest. Logs to `logs\check-summary.log` (PASS/FAIL table) and `logs\check.log` (full output).
  - `commit.ps1`: `git add -A` + commit with the message in `logs\commit-msg.txt`.
- **Pattern for each phase:** write a `scripts\win\phaseN.cmd/.ps1` that runs any setup the phase needs (new npm/cargo deps), then `check.ps1`, then `commit.ps1` only if checks pass, then `dev.cmd`. Write `logs\commit-msg.txt` and the final PROGRESS.md *before* asking the user to run it. Then the user double-clicks once.
- **Linux-side quick checks** (added in Phase 1): once `node_modules` exists on Windows, the VM can run the pure-JS tools directly. Use `node node_modules/typescript/bin/tsc --noEmit -p tsconfig.json`, `node node_modules/eslint/bin/eslint.js src --max-warnings 0` and `node node_modules/prettier/bin/prettier.cjs --check src`. Vitest and Vite can't run there (they need Windows-native esbuild/rollup). Don't run them while `check.ps1` is in its prettier step, because files are being rewritten.
- **New npm packages:** add them to `package.json` from the agent side. A plain `npm install` in the phase script then picks them up, with no separate "deps" click needed.
- **Line endings:** write `.cmd` files with CRLF (`sed -i 's/$/\r/'`). Everything else uses LF. Since Phase 1, `.gitattributes` keeps `*.cmd`/`*.bat` as CRLF on checkout. To commit outside a phase script, write `logs\commit-msg.txt` and ask the user to double-click `scripts\win\commit.cmd`.
- **Linux-side Rust builds** (added in Phase 2): double-click `scripts\win\phase2-fetch.cmd` after adding crates, then unpack `logs\linux-vendor.tgz` in the cloud container. Edit its scratch manifest in `phase2-fetch.ps1` to add new crates. This builds and tests the library crates offline; `src-tauri` needs Windows.
- **Linux-side Rust builds, Phase 3 setup** (the cloud container has Rust 1.97; no crates.io):
  - Mirror the repo: on the device, `tar` it into `logs\` (exclude `node_modules`, `target`, `.git`, `logs`), stage the tarball and unpack it in the container. Stage `logs\linux-vendor.tgz` too and unpack it to `~/vendor-oxide`.
  - Harness `~/oxide-linux`: a symlink `crates -> <mirror>/crates`; `.cargo/config.toml` replacing crates-io with `~/vendor-oxide/vendor` (offline); a workspace `Cargo.toml` made from the repo's with `members` = the four library crates (core, store, llm, agent) plus `[patch.crates-io] specta-macros = { path = "stubs/specta-macros" }`. The stub is a no-op `#[proc_macro_derive(Type, attributes(specta, serde))]` plus a pass-through `#[specta]` attribute.
  - Builds use `CARGO_TARGET_DIR=$HOME/oxide-target-linux`. clippy and rustfmt work there.
  - New crates must already be in the vendor bundle. `futures-util` stays `default-features = false` because `futures-macro` isn't vendored.
  - Frontend files: copy them to the device and run tsc/eslint/prettier there (see above), then copy the formatted files back to the mirror.
- **Deleting files:** `rm` in the linked folder needs delete permission (`device_request_delete_permission`). Never run git from the Linux VM; use `commit.ps1` on Windows.

## Phase 3 — LLM layer & streaming chat — 2026-11-05
Status: code complete. The library crates (core, store, llm, agent: 111 tests) build, test and pass clippy on the Linux side; frontend typecheck, lint and prettier pass on the device. `scripts\win\phase3.cmd` runs every check on Windows, commits when green and starts the app.

Exit criteria, to check by hand in `cargo tauri dev` (needs a key or a running server for each):
- [ ] Anthropic: streaming, Stop, Retry (regenerate) and edit-and-resend; with extended thinking on, a "Thinking" line streams and becomes "Thought for Ns".
- [ ] OpenAI: same, with a model such as gpt-4.1-mini.
- [ ] LM Studio: same, with a tool-capable model loaded (e.g. qwen3).
- [ ] Strata: models appear after the one-time tool probe; streaming and Stop work.
- [ ] A new chat gets an automatic title after the first reply.

Done:
- **`oxide-core`:**
  - `model.rs`: `ModelInfo` (`id` = `<providerId>/<model>`), `ModelCapabilities { tools, thinking: none|budget|adaptive, images, pdfs }`, `ProviderModels` (models or a readable `error`), `model_id` / `split_model_id` (splits at the first `/`, since LM Studio names contain `/`).
  - `chat.rs`: `GenerateOptions { thinking }`, `StopReason` (endTurn, maxTokens, stopSequence, toolUse, refusal, contextFull, stopped, error, other), `Usage` (input, output, cache read/write), `StreamEvent` (started, textDelta, thinkingDelta, toolUse, usage, done, failed), `ChatStreamEvent { conversationId, event }`, `ConversationChanged`.
  - `Message` gains `stopReason` and `usage`. `AppSettings.models`: `showUnsupported`, `autoTitle` (on), `titleModel` (null = automatic), `thinkingBudget` (8000, clamped to 1024–64000).
- **`oxide-store`:** migration `0002_replies.sql` (`messages.stop_reason`, `messages.usage`; table `capability_probes` keyed by base URL + model). `add_reply(NewReply)` saves an assistant reply under a user message with an id chosen when it started. `probe_result` / `save_probe_result` / `clear_probe_results`. `new_id()` and `now_ms()` are public.
- **`oxide-llm`** (hand-written on reqwest 0.13):
  - `Provider` trait (object-safe, boxed futures): `list_models() -> ModelListing { models, unverified }`, `stream(ChatRequest) -> ChatStream`, `probe_tools(model)`. `connect(Endpoint)` picks the client. `Endpoint` redacts its key in `Debug`.
  - `ChatRequest { model, system, messages, tools, max_tokens, thinking: Default|Off|Budget(n)|Adaptive, cache }`. `ChatEvent`: TextDelta, ThinkingDelta, ThinkingSignature, ToolUseStart/Delta/End, Usage (cumulative), Stop(reason). Streams yield `Result<ChatEvent, LlmError>`; an `Err` ends them.
  - `sse.rs`: incremental SSE parser, tested with every possible chunk split (CRLF, multi-byte UTF-8, comments, unterminated last event).
  - **Anthropic:** `Authorization: Bearer` + `anthropic-version: 2026-10-21`. Thinking is sent as `adaptive` or `enabled` + `budget_tokens`, both with `display: "summarized"`; adaptive models get `{"type":"disabled"}` when the toggle is off. Images and PDFs go as base64 blocks; text documents go as text documents. Only signed thinking is sent back. Cache breakpoints sit on the system prompt, the last tool and the last two user turns. 429/503/529 are retried up to 3 times (1, 2, 4 s, or `retry-after` up to 30 s). Mid-stream `error` events and cut connections fail the stream.
  - **OpenAI-compatible:** Chat Completions with `stream_options.include_usage` (OpenAI and LM Studio only), `max_completion_tokens` for OpenAI and `max_tokens` for local servers (omitted unless set). Parallel tool calls are tracked by index, images go as data URLs, tool results become `tool` messages, and `reasoning_content`/`reasoning` deltas count as thinking. A stream that ends after a finish reason without `[DONE]` still counts as complete.
  - **Capabilities** (`capabilities.rs`): Anthropic models come from the Models API's `capabilities` (thinking types, image/PDF input) over a curated fallback, with tools for every Claude 3+ model. OpenAI uses a curated table: non-chat models are dropped; chat models without tools (chatgpt-*, o1-mini/preview, search, chat-latest) are kept but unusable. LM Studio uses `GET {root}/api/v0/models` (`capabilities: ["tool_use"]`, `type: vlm`). Strata models are all `unverified`, and each is probed once with a tiny non-streaming tool-call request (400/422 means no tools).
  - `probe.rs` (Settings → Providers test) now uses the same Anthropic auth headers.
- **`oxide-agent`:**
  - `ModelRegistry`: lists enabled endpoints in parallel. Listings are cached for 10 minutes; failed listings aren't cached. Endpoints that need a key and have none report it without making a request. Unverified models are probed one at a time (each probe may load a local model), with results kept in SQLite. `list(refresh)` clears both caches. `find(modelId)` looks up a model's capabilities, and `invalidate()` is called by the provider commands.
  - `ChatService` (one reply per chat at a time; `Busy` otherwise):
    - `send`, `edit_and_resend`, `regenerate` (on an assistant message → sibling reply; on a user message → answer it, i.e. retry) and `stop`.
    - `reply` builds the context from the path to the user message, emits `started` at once, then streams in a task. Text is batched every 40 ms. Stop and errors keep what was written (stopReason stopped/error). A stop before any output saves nothing. The chat is marked idle *before* the final `done`/`failed` event, so the UI can send again straight away.
  - `context.rs`:
    - `build_messages` drops earlier thinking and tool blocks, skips empty turns, merges same-role turns and starts with a user turn.
    - `system_prompt` is minimal: Oxide, the UTC date and the user's name.
    - `plan_request`: for Anthropic, it caps output at min(model max, 32k), raises it to fit a thinking budget plus 4k for the answer, shrinks the budget for small models and turns caching on. For OpenAI-compatible servers, it uses the server defaults.
  - Auto-title after the first exchange, if it's enabled and the chat still has its derived title. It uses the configured title model, or else a haiku (Anthropic) or gpt-4.1-mini/4o-mini (OpenAI) model; local servers use the chat model. The answer is cleaned (first line, no quotes or "Title:" prefix, at most 60 characters), there's a 30 s timeout, and the title is checked again before it's saved.
- **`src-tauri`:**
  - Commands: `start_conversation(input, options)` (now also starts the reply), `send_message`, `edit_and_resend`, `regenerate`, `stop_generation` and `list_models(refresh)`. These replace `add_user_message` and `edit_user_message`.
  - Typed events `ChatStream` (`events.chatStream`) and `ConversationUpdated` (`events.conversationUpdated`) are emitted by `TauriSink`, which implements `oxide_agent::EventSink`.
  - `AgentError` maps to `CommandError`.
- **Frontend:**
  - `features/chat/streams.ts`: a Zustand store of live replies and failures per chat, with a pure, tested `reduceStream`. `useChatEvents` (mounted in `AppLayout`) applies events, puts saved replies in the query cache and applies automatic titles.
  - Chat page:
    - The live reply shows a streaming "Thinking Ns" block, the text so far, tool-call chips and a "Working Ns" line before any output. The composer's Stop button calls `stopGeneration`.
    - Retry on any reply regenerates it; the thread ends at the answered message while it streams, so the new reply replaces the old one on screen.
    - A failed reply shows an alert with Retry. An unanswered message older than 1.5 s offers "Answer this message".
    - Stop notes appear under replies: Stopped, cut off, output limit, declined, context full, tool requested, no response.
    - The view follows the bottom unless the user scrolls up.
  - Model picker (`ModelPicker value/onChange`) groups models by endpoint, shows endpoint errors, hides models without tools (or greys them out), tags thinking models, and refetches when opened. In a chat it sets the chat's model. New chats use `useNewChatModel`, which falls back to a listed Sonnet, then any usable model.
  - The thinking toggle is disabled for models that can't think.
  - **Settings → Models:** show models without tool calling, automatic titles, title model, thinking budget, and each endpoint's models with Tools / Thinking / Images / PDFs badges. **Refresh** re-lists and re-probes.
  - `features/models/catalog.ts` is deleted, replaced by `features/models/{models,queries}.ts`.
- **Tests:**
  - Rust library crates: core 23, store 30, llm 38, agent 20.
  - The llm tests run against a local HTTP test server: SSE in 37-byte pieces, retries, stream errors, request bodies and headers, model lists.
  - The agent tests use a scripted fake provider: streaming and saving, stop with and without output, failures, busy, branching, titles, probes and caching.
  - A compile-time check makes sure every command future is `Send`.
  - Vitest: the fake backend streams replies through `fakeEvents`, with `hold`, `fail` and `auto` modes. New tests cover streams, models and chat streaming (live reply, Stop, failure and Retry, regenerate, thinking option, automatic titles, model picker, unsupported models, thinking toggle).

Deviations from PLAN.md (and why):
- **No `async-openai`.** OpenAI, LM Studio and Strata go through a hand-written Chat Completions client on reqwest, sharing the SSE parser with the Anthropic client. `async-openai` deserializes chunks into strict types that break on local servers' extra fields (`reasoning_content`, missing `id`s, error chunks), and it would pull in a second reqwest version. PLAN.md updated.
- **Anthropic API version 2026-10-21 with Bearer auth** (checked against the docs today). This version rejects `temperature`, `top_p` and `top_k`, so Oxide never sends them.
- **Events instead of a Tauri `Channel` per conversation:** global typed events carry the conversation id. One app-wide listener keeps a reply streaming into the UI while the user is on another page; a channel would die with the page that started it.
- **`regenerate` covers retry after a failure** (it accepts a user message id), so there is no separate `generate_reply` command. `start_conversation` takes `GenerateOptions` and starts the reply.
- **Reply generation lives in `oxide-agent`** (`ChatService`), not `src-tauri`, which stays glue only. Phase 6 builds the tool loop on it.
- **Stream items are `Result<ChatEvent, LlmError>`** instead of an `Error` variant of `ChatEvent`.
- **LM Studio without `/api/v0`** (older versions) lists its models as "no tools" rather than probing, because probing would load every model.
- **The system prompt is a minimal placeholder** (Phase 6 builds the real one). Its date is UTC, because there's no time zone crate yet.

Known issues / TODO carried forward:
- **Tool calls aren't run (Phase 6).** They stream and are saved, and the UI says Oxide doesn't run tools yet. Tool blocks are dropped from the history sent to models until Phase 6.
- **Phase 4 work still open:**
  - Markdown is still the Phase 1 paragraph/code splitter.
  - Branch navigation (‹ n/m ›) is missing. Edited and regenerated branches exist, and the newest is shown.
  - Usage is stored but not shown.
  - Attachments aren't sent. PDFs for OpenAI-compatible models get a "can't be read" placeholder until `pdf-extract`.
- **Window reloads:** a reply that is streaming when the window reloads (e.g. Vite HMR) isn't shown live again, only once it's saved.
- **Stuck chats:** if the reply task ever panicked, its chat would stay busy until restart. Nothing in it panics (no unwraps).
- **Not supported yet:**
  - `anthropic-workspace-id` (only needed for keys that span several workspaces).
  - Effort control for OpenAI reasoning models.
- **Vitest output:** the fake backend emits events from timers, so React may log "not wrapped in act(...)" warnings in test output. They're harmless.

Notes for Phase 4:
- **Markdown:** the live reply re-renders `Prose` about 25 times a second (the backend batches deltas every 40 ms). The Markdown renderer must handle partial input (an open code fence, half a table) without layout jumps.
- **Branch navigator:** `activeThread(messages, leafId)` already takes a leaf. Keep the chosen leaf per chat in the UI, and pass it when sending (`parentId`).
- **Attachments:**
  - `ChatService::send` already takes `Vec<ContentBlock>`; widen `send_message`.
  - `MediaSource::Path` is read and base64-encoded at request time (`oxide-llm/src/media.rs`).
  - Replace the PDF placeholder in `openai.rs` (`user_messages`) with extracted text.
- **Usage tooltip:** read `Message.usage`; the live value is in `LiveReply.usage`.

Notes for Phase 6:
- `ChatService::stream_reply` is one model turn. The tool loop wraps it, and `build_messages` must keep tool blocks once results exist.
- Tool plumbing that already exists:
  - `ChatRequest.tools` / `ToolSpec` map to both APIs.
  - OpenAI `tool` messages and Anthropic `tool_result` blocks are built.
  - `ChatEvent::ToolUse*` streams the arguments.
- Approvals can pause inside `ChatService` and resume through a new command. The `cancelled` watch channel already gives clean cancellation.

Verified: clippy ✅ tests ✅ (library crates on Linux; full workspace pending on Windows) typecheck ✅ lint ✅ prettier ✅ — Windows run and manual exit checks pending

## Phase 2 — Persistence, settings & secrets — 2026-11-05
Status: done. All checks pass on Windows via `scripts\win\phase2.cmd` (fmt, prettier, clippy, cargo test 58/58 including the Credential Manager round trip, typecheck, lint, vitest 39/39). Committed as `phase 2: persistence, settings & secrets`.

Exit criteria, checked by hand in `cargo tauri dev`:
- Create, rename, star and delete persist across restarts.
- Ctrl+K search finds message text.
- The Anthropic key is stored as `provider:anthropic.dev.oxide.app` under Windows Credential Manager → Generic Credentials. The UI shows only `••••-QAA`.
- Test connection reached the real API: 14 models in 518 ms.

Done:
- **`oxide-core` types** (serde + specta, camelCase over IPC, timestamps as `i64` ms exported as `number`):
  - `Conversation { id, title, projectId, model, starred, createdAt, updatedAt }` and `StartConversation`.
  - `Message { id, conversationId, parentId, role, content, model, createdAt }`. `parentId` makes a tree.
  - `ContentBlock` = text | image | document | thinking | toolUse | toolResult | artifactRef (tag `type`), plus `MediaSource` (base64 | path) and `plain_text()` for search.
  - `Project` / `NewProject` / `ProjectPatch`, `SearchHit` (snippet terms marked with U+0002/U+0003).
  - `AppSettings { profile { name }, appearance { theme, chatFont } }`, all `#[serde(default)]` so new fields never break old documents.
  - `ProviderKind` (anthropic | openai | lmstudio | strata) with labels, default URLs (all end at `/v1`) and `requires_key`; `ProviderConfig` (masked `keyHint`, never the key), `ProviderInput`, `TestProvider`, `ProviderTestResult`.
- **`oxide-store`** (sqlx 0.8, bundled SQLite):
  - `Store::open(path)`: WAL, foreign keys, busy timeout, embedded migrations (`crates/oxide-store/migrations/0001_init.sql`). `open_in_memory()` for tests.
  - The app opens `%APPDATA%\dev.oxide.app\oxide.db` at startup (`src-tauri/src/state.rs`).
  - Tables: `projects`, `conversations`, `messages`, `settings`, `providers`, plus FTS5 tables `conversations_fts` and `messages_fts`. These are external-content tables, kept current by triggers, with `unicode61 remove_diacritics 2`.
  - Searchable tables have an explicit `seq INTEGER PRIMARY KEY`, so FTS rowids survive VACUUM.
  - Migration seeds the four default endpoints.
  - Repositories:
    - Conversations: start (chat + first message in one transaction), rename, star (doesn't bump `updatedAt`), set model, delete (cascades to messages and the index).
    - Messages: list (all branches, insertion order), add (checks the parent is in the same chat, bumps the chat), edit = add a sibling.
    - Projects: CRUD; deleting one detaches its chats.
    - Settings: typed JSON per key; an unreadable document falls back to defaults.
    - Providers: CRUD, plus key set/clear/read.
  - Search: each word is quoted and prefix-matched (FTS syntax in the query is harmless). Title hits rank first, then chats by their best message match, with a snippet.
  - **Secrets:** `SecretStore` trait with `KeyringSecrets` (keyring 3: `windows-native` → Credential Manager, `apple-native`, `linux-native`) and `MemorySecrets` for tests.
    - Keys are stored as `provider:<id>` under service `dev.oxide.app`.
    - Calls run on `spawn_blocking`.
    - `Secret`'s `Debug` output is redacted.
    - Only a masked hint (`••••abcd`) goes into SQLite.
- **`oxide-llm::probe::list_models`:** connection test (GET `{base}/models`).
  - Anthropic uses `x-api-key` and `anthropic-version: 2023-06-01`; the other kinds use bearer auth.
  - Readable errors (unreachable, 401/403, API error message) that never contain the key.
  - reqwest 0.13 with native TLS.
- **`src-tauri`:**
  - 24 new commands in `commands/{chats,projects,settings,providers}.rs`, registered in `specta_builder`.
  - Fallible commands return `CommandError { kind: notFound | invalid | keychain | internal, message }`.
  - Bindings are exported with `BigIntExportBehavior::Number`.
  - Keys only travel frontend → backend (`setProviderKey`, `testProvider`).
- **Frontend:**
  - TanStack Query over `@/bindings` (`lib/api.ts` `unwrap` / `ApiError`; query hooks in `features/{chats,projects,settings,providers}/queries.ts`).
  - Rename, star and delete are optimistic, with rollback.
  - The sidebar, title bar breadcrumb, chat page, home and project composers, Chats page, Projects page (new **New project** dialog) and the "+" menu all read real data.
  - The chat page shows the active branch (`features/chat/thread.ts`, `activeThread`). Editing a message creates a branch. Sending saves the message and shows "Saved. Oxide can't reply yet" until Phase 3.
  - Search (Ctrl+K and the Chats page filter) uses FTS with highlighted snippets.
  - Settings: name, theme and chat font are stored in SQLite. `useUi` keeps a persisted first-paint copy, so the theme shows without a flash, and Phase 1 localStorage preferences are imported once.
  - **Settings → Providers:**
    - Lists the endpoints; edit name, URL and key.
    - Add an endpoint (type picker with defaults that follow the type; "Use default" link).
    - Remove a saved key; delete an endpoint.
    - "Test connection" works with a typed or saved key, and shows the model count, a sample and latency.
  - Artifacts page: empty state until Phase 8.
  - The model list is a static catalog (`features/models/catalog.ts`) until Phase 3.
  - `features/mock/` is deleted.
- **Tests:**
  - 51 Rust tests in core/store/llm, including persistence across reopening the file, FTS (snippets, renames, diacritics, deletes, hostile queries), cascades, key hint vs key, and probe headers against a local TCP server.
  - A Windows-only test round-trips a real Credential Manager entry (service `dev.oxide.app.test`, cleaned up afterwards).
  - Vitest: an in-memory fake backend (`src/test/fakeBackend.ts`, mocked into `@/bindings` by `setup.ts`) and fixtures. App tests cover chats, edits, search, rename/star/delete, projects, settings and providers.
  - `renderApp(path, { ui, backend })` now seeds the fake backend.

Deviations from PLAN.md (and why):
- **`ContentBlock::ToolUse.inputJson`** is a JSON string, not a `serde_json::Value`, which would need specta's `serde_json` feature. Phase 3 can switch if useful.
- **Projects got a table and basic CRUD now**, not in Phase 7: conversations reference them and the UI already shows them. Knowledge files, instructions editing and memory stay in Phase 7.
- **Edits already branch** (sibling message under the same parent); branch navigation is still Phase 4.
- **Provider endpoints live in a `providers` table**, not the settings JSON, because they're CRUD-shaped and need stable ids for keychain entries.
- **Linux keys** use the kernel keyring (`linux-native`): no D-Bus build dependency, but keys don't survive a reboot. Revisit with Secret Service in Phase 17.
- **specta's `derive` feature is now enabled explicitly** in the workspace. Before, it only came through tauri-specta's feature unification, so library crates didn't build on their own.
- **New workflow, Linux-side Rust builds:**
  - `scripts\win\phase2-fetch.cmd` runs `cargo vendor` on a scratch manifest and writes `logs\linux-vendor.tgz`.
  - The agent unpacks it in its cloud container and builds/tests core/store/llm offline (`CARGO_TARGET_DIR=$HOME/oxide-target-linux`). `src-tauri` still only builds on Windows.
  - specta-macros isn't in that bundle, so the Linux side patches in a no-op stub; type export only runs on Windows.

Known issues / TODO carried forward:
- **No replies yet:** sending saves the message only. Phase 3 adds streaming and assistant messages via `Store::add_message`.
- **Model picker:**
  - It's static, and the composer's model applies to new chats only.
  - `set_conversation_model` exists but isn't wired to the chat page yet (Phase 3).
- **Endpoints and models:** the `enabled` flag is stored but unused until Phase 3 lists models.
- **First run:** a key set before Phase 2's first run doesn't exist; keys only come from Settings → Providers.
- **Search quirk:** words are prefix-matched, so "pal" finds "palette" but not "napalm". Mid-word matching would need a trigram index.
- **`src/bindings/index.ts`** is regenerated by `cargo test` (`export_bindings`). If a type change looks wrong in TS, regenerate it before editing the frontend.

Notes for Phase 3:
- **Anthropic auth changed** (checked in the docs on 2026-11-05):
  - The current docs send `Authorization: Bearer <key>` with `anthropic-version: 2026-10-21`. `x-api-key` is now "legacy" but still works; the probe uses it with `2023-06-01`.
  - Switch to Bearer in the real client and check the version header.
  - Keys not scoped to a workspace need `anthropic-workspace-id` on every request, or the API returns 400. The user's key is scoped to the Default workspace. Consider an optional workspace field on Anthropic endpoints.
- The user's Console plan is "Evaluation access" ($0). Listing models works; sending messages may need credits.
- Persist assistant replies with `store.add_message(conversation_id, Some(parent), Role::Assistant, blocks, Some(model))`. The thread's leaf is the last message from `list_messages` (see `activeThread`).
- Get keys with `store.provider_key(&state.secrets, id)`. Never put them in events or errors.
- `probe::ANTHROPIC_VERSION` and the URL conventions (base ends at `/v1`) are in `oxide-llm/src/probe.rs`. Reuse them for the real clients.
- The chat page has a `NoReplyYet` note and `WorkingLine` (unused) ready for streaming.
- The fake backend in `src/test/fakeBackend.ts` must gain every new command, or tests calling it fail at type-check.

Verified: clippy ✅ tests ✅ typecheck ✅ lint ✅ tauri dev on Windows ✅

## Phase 1 — Design system & app shell (static) — 2026-11-05
Status: done. All checks pass on Windows (fmt, prettier, clippy, cargo test, typecheck, lint, vitest 25/25). `cargo tauri dev` shows the shell on mock data in both themes.

Screenshots in `docs/ui-parity/`:
- `claude-desktop-light-chat.png`: the reference.
- `oxide-light-home.png`, `oxide-light-chat.png` and `oxide-dark-chat.png`: Oxide.

The two chat screenshots predate one fix: the "New chat" item's icon sits above its label because of the Tooltip/NavLink bug noted below. It's fixed and covered by a test.

Done:
- **Design tokens** (`src/styles/tokens.css`, `--ox-*`):
  - Warm off-white / warm near-black surfaces and Oxide's own iron-oxide accent (`#b4532d` light, `#d7754d` dark).
  - Lines, radii, shadows, type scale and motion.
  - Light, dark and system themes. The system theme applies from the first frame through the CSS media query. Any element can scope a theme with `data-theme`, which the Settings previews use.
  - Tailwind names are mapped in `index.css`: `bg-app/sidebar/raised/sunken/hover/active/bubble`, `text-fg/-muted/-subtle`, `border-line/-strong`, `bg-accent/-soft`, plus `font-chat` and `max-w-thread`.
- **Fonts:** Inter, Source Serif 4 and JetBrains Mono, bundled from `@fontsource-variable/*` so nothing loads from the network. The chat font (serif / sans / system) is switchable.
- **Oxide mark:** an open ring with one atom slipping out of the gap (`components/OxideMark.tsx`). It's also `app-icon.svg` → `npx tauri icon`, and `public/oxide-mark.svg` (favicon).
- **Title bar** (`app/shell/TitleBar.tsx`):
  - Full width, with a left segment aligned to the sidebar: Oxide menu, sidebar toggle, back/forward.
  - Breadcrumb `Project / Chat ⌄` (star / rename / delete), a chat-details panel toggle, and Windows caption buttons.
  - Per-OS config: `tauri.windows.conf.json` sets `decorations: false`; `tauri.macos.conf.json` uses an overlay title bar with a traffic-light inset; Linux keeps native decorations.
  - The capability file adds start-dragging, internal-toggle-maximize, minimize, toggle-maximize, close and set-theme.
- **Sidebar:**
  - Resizable (220–420 px, double-click resets) and fully hideable with Ctrl+B.
  - New chat, Search, Chats, Projects and Artifacts; collapsible Starred and Recents; a "…" menu per chat (star, inline rename, delete).
  - Profile menu at the bottom with Settings, an Appearance submenu and Help.
- **Pages:**
  - `/new`: greeting by time of day with the profile name, large composer, five chips.
  - `/chat/:id`: centered 768 px column, user bubbles on the right, serif assistant prose, hover copy/retry/edit, a "Thought for Ns" collapsible block, a live "Thinking" status line, and the right-hand "In this chat" panel.
  - `/chats`, `/projects`, `/project/:id` (instructions + knowledge cards) and `/artifacts` (gallery).
- **Composer:**
  - Auto-grows up to 320 px. Enter sends; Shift+Enter adds a new line.
  - "+" menu: attach files, add from a project, web search toggle, connectors submenu.
  - Extended-thinking toggle, model picker grouped by provider, send ↔ stop.
  - Window-wide drop overlay. It uses Tauri's native drag-drop (real paths for Phase 4) and falls back to HTML5 in a browser.
- **Overlays:**
  - Settings modal at `/settings/:section` with 13 sections: General (name), Appearance (theme cards, chat font) and About (version via `commands.ping`) work; the others name their phase.
  - Ctrl+K search over titles and message text.
  - Ctrl+/ shortcuts dialog.
- **Shortcuts** (`app/shortcuts.ts`): Ctrl/⌘ + K, Shift+O, B, `,` and `/`. `/` is also matched as Shift+7 on Nordic layouts.
- **State:**
  - Zustand `useUi` holds preferences, persisted through `lib/storage.ts`, a guarded localStorage with an in-memory fallback.
  - `features/mock/store.ts` is an in-memory backend stand-in with seed chats, projects and artifacts, plus a fake 1.8 s reply so stop and the status line are visible.
- **Tests:** 25 Vitest tests (shell, navigation, active nav styling, composer send/stop, search, settings, theme switch, shortcut matching, helpers).

Deviations from PLAN.md (and why):
- **Layout follows the current desktop app** (from the user's screenshot), not the older layout described in the plan. The title bar spans the full width with back/forward buttons; a collapsed sidebar hides completely instead of shrinking to an icon rail; the reply composer's toolbar sits under the input.
- **The Oxide logo sits in the title bar** (as the app menu button), not at the top of the sidebar, which in the reference starts directly with "New".
- **Added a `/chats` page** for the sidebar's "Chats" item. PLAN.md routing updated.
- **Settings is a modal over the previous page.** Implementation: a single splat route renders `AppLayout`, which matches pages with `useRoutes` against a background location.
- **Hash router** (`createHashRouter`): it works from Tauri's asset protocol without a server-side fallback.
- **Window/webview APIs** are used directly from `@tauri-apps/api`, through `src/lib/window.ts` only. CLAUDE.md updated.
- **Versions:** react-router 7.18, zustand 5.0, radix-ui 1.8 (unified package), lucide-react 1.69 (ISC).

Known issues / TODO carried forward:
- **Mock data.** Chats, projects, artifacts and models are mock data (`features/mock/*`). Phase 2 replaces the store; components read through it, so the swap stays local.
- **Placeholder controls.** The model list, connectors and web search toggles are UI only. Attachments show chips but aren't read.
- **Node 26 quirks in tests:**
  - Node's own `localStorage` stub shadows jsdom's, which `src/test/setup.ts` patches.
  - Data routers fail under jsdom (AbortSignal mismatch), so tests use `MemoryRouter`.
  - The ExperimentalWarning about `--localstorage-file` in test output is harmless.
- **Back/forward.** Back is disabled at the first history entry. Forward is always enabled, because history length isn't observable.
- **macOS.** The traffic-light position (16, 22) isn't verified on a Mac yet.
- **Gotcha.** Never wrap a `NavLink` that uses a function `className` in a Radix `asChild` component (Tooltip, Menu trigger): the Slot stringifies the function. Resolve the active state with `useMatch` instead (see `Sidebar.tsx` `NavItem`).
- **Obsolete scripts.** `scripts\win\phase1-deps.*` is superseded by `phase1.cmd` (agent-side delete isn't allowed). Delete by hand.

Parity (Oxide vs reference, `docs/ui-parity/`):
- Same layout: full-width title bar with nav buttons on the left and caption buttons on the right; sidebar with nav, sections and profile at the bottom; centered thread; grey right-aligned user bubbles; rounded reply composer with the toolbar underneath; status line with two accent dots.
- Intentional differences:
  - Oxide mark menu instead of the hamburger menu.
  - No Chat/Code mode switch, Share button, voice input or effort/"Auto" controls. Export comes in Phase 4; voice is a Phase 18 stretch goal.
  - Sidebar has Chats instead of Scheduled/Customize; Scheduled arrives in Phase 15.
  - Starred + Recents instead of Pinned with project grouping.
  - "Local profile" instead of a plan badge.
  - Own fonts, accent colour and wording.
- Small visual differences to revisit: sidebar text runs slightly larger than the reference; the reference title bar has no border under the sidebar segment.

Notes for Phase 2:
- Replace `useMock` with TanStack Query hooks over the generated bindings. The shapes in `features/mock/types.ts` show what the UI needs (`Conversation.starred/projectId/modelId/updatedAt`, message blocks, `Thinking { seconds, text }`).
- Move `userName`, theme and chat font into stored settings. `useUi` can keep layout state (sidebar width/open, sections).
- Search (`features/search/SearchDialog.tsx`) and the Chats page filter in memory. Swap in the FTS5 query.
- The Providers settings section is a placeholder in `features/settings/SettingsDialog.tsx` (`SectionBody`).

Verified: clippy ✅ tests ✅ typecheck ✅ lint ✅ tauri dev on Windows ✅

## Phase 0 — Scaffold & toolchain — 2026-11-05
Status: done. The app window opened and showed the `ping` reply. Final checks and the commit run through `phase0-finish.cmd`; see `logs\check-summary.log` and `logs\commit-summary.log`.

Toolchain (from `logs\toolchain.log`):
- Windows 11 Pro (10.0.26300)
- rustc/cargo 1.99.1, MSVC target; rustup 1.29.2
- tauri-cli 2.12.4
- node 26.13.0, npm 12.0.2
- git 2.54.0, uv 0.12.10
- Visual Studio Community 2022 17.14 (C++ tools)
- WebView2 156.0.4314.58

Done:
- **Cargo workspace:**
  - Edition 2024, resolver 3, `rust-version = "1.90"`.
  - Shared deps; lints deny `unwrap_used`, `dbg_macro` and `unsafe_code`.
  - `rust-toolchain.toml` (stable + clippy/rustfmt).
- **8 stub crates** under `crates/`:
  - `oxide-core` has `APP_NAME` and `version()` plus tests.
  - The others hold module docs only, naming the phase that fills them.
- **`src-tauri` app crate** (`oxide`, lib `oxide_lib`):
  - `ping` command registered through tauri-specta.
  - Bindings are exported to `src/bindings/index.ts` on every debug run, and by the test `export_bindings` (`npm run bindings`).
  - Logging via `tracing`.
  - `tauri.conf.json`: id `dev.oxide.app`, window 1200×800 with a 760×520 minimum, strict CSP.
  - `capabilities/default.json` holds `core:default`.
- **Windows manifest fix:**
  - `src-tauri/windows-app-manifest.xml` is embedded into *every* binary by `build.rs` (`/MANIFEST:EMBED`).
  - `tauri-build` is told not to add its own (`WindowsAttributes::new_without_app_manifest()`).
  - Without this, Rust test binaries crash on start with `STATUS_ENTRYPOINT_NOT_FOUND` (0xc0000139).
- **Frontend:**
  - Vite 7, React 19, TS strict with `noUncheckedIndexedAccess`, and the `@/` alias.
  - Tailwind v4 via `@tailwindcss/vite`, with tokens in `src/styles/tokens.css` mapped through `@theme inline`.
  - ESLint 9 flat config, Prettier, Vitest + Testing Library (jsdom).
  - `App.tsx` calls `commands.ping`.
- **Icons:** placeholder `app-icon.svg` (rust tile + ring); `npx tauri icon` generated `src-tauri/icons/*`.
- **Docs:** `CLAUDE.md` (conventions), `PLAN.md` and `PROGRESS.md` in the repo root.

Deviations from PLAN.md:
- React 19 instead of 18: it's the current stable release, and nothing in the plan depends on 18.
- `rust-version` is 1.90, not 1.85. Cargo's MSRV-aware resolver had held Tauri at 2.11 to match 1.85, which mismatched `@tauri-apps/api` 2.12. Tauri 2.12 needs Rust 1.90.
- tauri-specta stays pinned at `=2.0.0-rc.21` with `specta =2.0.0-rc.22` and `specta-typescript 0.0.9`. Newer RCs exist (tauri-specta rc.26, specta rc.28, specta-typescript 0.0.15), but rc.21 works and its export API is the one in use. Upgrade deliberately in a later phase if something needs it.
- Windows dev scripts were added (not in the plan) because builds can't run from the agent side.

Known issues / TODO carried forward:
- npm 12 blocked esbuild's postinstall script (`allowScripts`). Vite works because esbuild ships its binary as an optional package. Revisit if esbuild ever fails to start.
- `eslint@9` reports as no longer supported (ESLint 10 is out). Upgrade together with the plugins in a later phase.
- Default git identity: `commit.ps1` sets a repo-local placeholder (`Oxide Developer <oxide-dev@localhost>`) only if none is configured.
- The placeholder icon is temporary. Phase 1 designs the real Oxide mark.

Notes for Phase 1:
- Build the design system in `src/styles/tokens.css`, using the `--ox-*` variable prefix and Tailwind mapping in `src/styles/index.css`.
- Add these npm deps in `scripts\win\phase1.ps1`: react-router, zustand, the Radix primitives, and lucide-react for icons (ISC licence; no Anthropic assets).
- Bundle fonts locally (Inter, Source Serif 4, JetBrains Mono, all OFL) in `src/assets/fonts/` via `@fontsource-variable/*` packages, so there's no network at runtime.
- For parity screenshots, ask the user to screenshot Claude desktop and Oxide side by side. Computer use can't capture this PC's screen.

Verified:
- Window opens with the `ping` reply ✅.
- clippy, cargo test, typecheck, lint and vitest pass via `phase0-finish.cmd`; see `logs\check-summary.log`.

## Planning — 2026-11-05
- Agreed scope and decisions (see PLAN.md §0). App name: Oxide.
- PLAN.md written and saved to the project.
