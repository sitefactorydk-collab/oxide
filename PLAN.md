# Oxide — Build Plan

Oxide is a cross-platform desktop AI assistant (Windows, macOS, Linux) written in Rust on Tauri v2. It recreates the features and look of the Claude desktop app, working from its **visible behavior and public specs only**. Its model backends are Anthropic, OpenAI, LM Studio and Strata.

This file is the single source of truth. Each phase below is sized to fit in **one chat**. Start a new chat in this project for every phase (see "How to run a phase").

---

## 0. Ground rules

### Clean-room boundary
- Reference material: the Claude desktop app's visible UI and behavior, the public MCP specification, and the public `claude_desktop_config.json` format and `.mcpb` (desktop extension) manifest format.
- Never unpack, decompile or copy Anthropic's app bundle, code, icons, illustrations, fonts or copy text.
- The UI should *look and feel* very close to Claude desktop: the same layout, information architecture, spacing rhythm, warm neutral palette, interactions and shortcuts. It uses **Oxide's own logo, name and open-licensed fonts**:
  - UI and body text: **Inter**.
  - Headings and assistant prose: **Source Serif 4**.
  - Code: **JetBrains Mono**.
  - All three are bundled locally under OFL.

### Decisions already made
| Area | Decision |
|---|---|
| Desktop shell | Tauri v2. Rust backend, React 18 + TypeScript + Vite frontend |
| Async | `tokio` |
| Providers | Hand-written clients on `reqwest` with one shared SSE parser. Anthropic Messages API (thinking, tool use, images, PDFs and prompt caching are first-class). OpenAI, LM Studio and Strata through one OpenAI-compatible Chat Completions client (changed from `async-openai` in Phase 3; see PROGRESS.md) |
| Strata default | `http://127.0.0.1:47831/v1` |
| LM Studio default | `http://127.0.0.1:1234/v1` |
| Models | **Only models with native function calling.** Capability is detected per provider; models without it are hidden, with a toggle to show them greyed out |
| MCP | Own client crate. JSON-RPC 2.0 over stdio (`tokio::process::Command`) plus Streamable HTTP. **Dual-era:** speaks the stateless 2026-07-28 protocol and falls back to the legacy `initialize` handshake (2025-11-25 / 2025-06-18 / 2025-03-26) for older servers |
| MCP config | Oxide owns `oxide_config.json` in the app config dir. Its `mcpServers` block uses **exactly** Claude desktop's schema (`command`, `args`, `env`), so entries paste 1:1. Remote servers add `"type": "http", "url", "headers"`. Oxide-only keys live under a separate top-level `"oxide"` object, never inside a server entry. Settings has a one-click **"Import from Claude desktop"** that reads `claude_desktop_config.json` read-only and merges after confirmation |
| Desktop extensions | `.mcpb` bundles unpack into `<app data>\extensions\<name>` with Oxide's own zip reader (`oxide-mcp/src/archive.rs`). Each has a managed entry under `oxide.extensions.<name>` in `oxide_config.json` (enabled, version, non-sensitive settings); sensitive settings live in the OS keychain. The command line is built from the manifest at start-up and run by the MCP manager like any server (Phase 10) |
| Skills | Folders with a `SKILL.md` in `<app data>\skills`; names and descriptions go into the system prompt and the built-in `load_skill` tool returns the rest (Phase 10) |
| Python | Oxide manages its own `uv` (a Tauri sidecar per target, pinned to one release in `scripts/uv-pins.json` and fetched by `npm run fetch-uv`). On first use it installs CPython 3.12 and a venv under `<app data>\python\` with the packages in the locked `python/uv.lock` (embedded in the app). `run_python` always asks for approval: the code is not sandboxed (Phase 12) |
| Docker sandbox | Own crate `oxide-sandbox` driving the `docker` CLI (works with Docker Desktop's context). A container per chat (`oxide-sandbox-<chat id>`), workspace volume or folder at `/workspace`, no network by default; the reference filesystem MCP server runs per chat from its image and is **not** an MCP-manager entry (manager servers are shared by all chats). Default image `python:3.12-bookworm` (Phase 12b) |
| Browser agent | Browser-Use (1.0) wrapped as a local MCP server (`python/oxide_browser_agent`, official `mcp` SDK 2.x, stdio), run **per chat** from the uv venv by `oxide-pyenv` (not an MCP-manager entry), with granular tools on Playwright sharing the browser over CDP. Its packages are the `browser` dependency group, installed on first use. It drives Chrome or Edge installed on the computer (own throwaway profile), else Playwright's Chromium, or the user's own Chrome over CDP (Phase 13) |
| Chrome link | Own MV3 extension (`extension/`, TypeScript built with `tsc`) that talks to Oxide over a localhost **WebSocket with token pairing** (`oxide-bridge`, its own small RFC 6455 implementation; JSON-RPC 2.0). Its tools reach models as `chrome__<tool>` while a paired Chrome is connected (Phase 14) |
| Storage | SQLite via `sqlx` (migrations checked in). API keys in the OS keychain via `keyring`. Project knowledge search uses SQLite FTS5 with BM25 ranking (Phase 7) |
| OS dialogs and links | `tauri-plugin-dialog` (file picker, save dialog) and `tauri-plugin-opener` (links open in the system browser), called from Rust commands so the frontend still only uses `@/bindings` (Phase 4) |
| IPC typing | `tauri-specta` generates TS bindings from Rust commands and events. No hand-written IPC types |
| Frontend state | Zustand for UI state, TanStack Query for backend data |
| Styling | Tailwind CSS v4 + CSS variables design tokens. Radix UI primitives for menus, dialogs and tooltips |
| Markdown | `react-markdown` + `remark-gfm` + `remark-math`/`rehype-katex` + Shiki for code (JavaScript regex engine, since the CSP blocks WASM; colours from a CSS-variables theme in `tokens.css`). Model output is pre-processed (`components/markdown/prepare.ts`) for LaTeX delimiters, prices vs `$math$`, and partial input while streaming |
| Desktop integration | Official Tauri plugins: single-instance (registered first), deep-link (`oxide://`), global-shortcut, autostart (`--hidden` starts in the tray), notification, window-state. Tray via Tauri's `tray-icon` feature. Quick entry is a second, hidden-at-startup webview window (`index.html?window=quick`); screenshots via `xcap` in `oxide-computer` (Phase 9) |
| Artifacts | Built-in `artifact` tool (create / update / rewrite). HTML and React run in an `<iframe>` sandboxed without `allow-same-origin`, loaded from Oxide's own `oxide-artifact` URI scheme with its own CSP (inline scripts yes, network requests no) — not `srcdoc`, which would inherit the app's CSP (Phase 8) |
| Computer use | `oxide-computer`: xcap screens, enigo input, Win32 + UI Automation for the app under the pointer and the focused field. Claude gets Anthropic's own computer tool (version per model); other models one `computer` function with the same input. Each chat asks once, then per app; a top bar with Stop and Ctrl+Alt+Esc stop it. **Windows only for now** (Phase 16) |
| Packaging & updates | `tauri build` per platform (`tauri.<os>.conf.json` pick the targets): NSIS + MSI (WebView2 download bootstrapper), a universal `.dmg` (ad hoc signed unless Apple secrets are set), AppImage + `.deb` + `.rpm`. The `uv` sidecar and the built Chrome extension (`bundle.resources`) ship in every installer. `tauri-plugin-updater` with minisign-signed bundles; the public key is committed, the update URL is compiled in by the release workflow (`OXIDE_UPDATE_URL`), so local builds don't update. GitHub Actions: `ci.yml` (checks on 3 OSes), `release.yml` (tauri-action → draft release + `latest.json` + extension zip). See `docs/RELEASING.md` (Phase 17) |
| IPC permissions | Oxide's commands are declared to Tauri (`AppManifest`, read from `collect_commands!` by `build.rs`), so each window may call only what its capability lists: the main window every command, quick entry five, the computer-use bar two; no `core:default`, no plugin calls from the frontend, no remote origins (artifact frames get no IPC) (Phase 17) |
| Secrets on Linux | The Secret Service (`keyring`'s `sync-secret-service` + `crypto-rust`, behind `oxide-store`'s `secret-service` feature, which `src-tauri` enables), with kernel keyutils as the fallback. Old keyutils keys move over when read (Phase 18) |
| MCP command trust | A local server's command line (command, args, env) must be trusted before it runs. A SHA-256 fingerprint per server name is kept in `<data>/mcp-trust.json`, outside the config. Saves in Settings, the import and extensions are trusted as they are saved; anything else waits in `needsTrust` for **Trust and start** (Phase 18) |
| Logging | `tracing` to stderr and `<logs>/oxide.log` through a redacting writer (`oxide_core::redact`); the file starts over past 5 MB (Phase 18) |
| Paths from the window | Commands that grant folders or install from disk accept only paths from Oxide's pickers or drops (`src-tauri/src/chosen.rs`); others need a native confirmation (Phase 18). See `docs/SECURITY.md` |
| i18n | Own small `src/i18n` (`useT`, flat keys, `en.ts`/`da.ts` with matching key sets), language in `appearance.language` (system/en/da). Danish covers the shell; other text is English until translated (Phase 18) |
| Dictation | Provider speech-to-text through an OpenAI-compatible `/audio/transcriptions` endpoint chosen in Settings → Voice (MediaRecorder in the web view, base64 to `transcribe_audio`), not local `whisper-rs` (Phase 18) |
| Media generation | OpenAI-compatible only: `oxide-llm::generation` (`/images/generations` and `/images/edits`, `/videos` with polling, `/music/generations` (synchronous; the shape of the user's comfy-bridge server, since OpenAI has no music API), `GET /models`). Settings → Media generation keeps an endpoint per kind (image, video, music; text comes from the chat models in Providers and Models): a provider or a custom URL, a model, and an optional key of its own in the keychain (`generation:<kind>`). Chats get `generate_image`, `generate_video` and `generate_music`, which ask first; results are stored with the attachments and played via `blob:` URLs (CSP `media-src 'self' blob:`) (Phase 19) |
| Long chats | A message window (newest 40 mounted, older ones on scrolling up) plus `content-visibility: auto`, not a virtualized list (Phase 18) |
| Utility Apps | Folders with `oxide-app.json` (built-in ones compiled in from `utility-apps/`, own ones in `<app data>\apps\<id>`), run in a frame on the `oxide-app` scheme (opaque origin, no IPC, CSP without any network), compiled by Babel with a module map (react, react-dom, lucide-react, react-markdown, remark-gfm, jszip, `oxide`). The `oxide` SDK (`src/appframe/sdk.ts`) talks to the main window by `postMessage`; the backend (`oxide-agent/src/apps`) checks each capability against the manifest and the per-app grant (asked once). Settings, grants and storage live in SQLite, keys as `app:<id>:generation:<kind>` (Phase 20) |
| Utility App builder | A draft per app in `<app data>\app-drafts\<id>` (Preview and Code show it; **Save to Utility Apps** copies it over the saved app), its builder chat an ordinary chat hidden from the chat list and search (`app_builder_chats`) with builder tools confined to the draft (`oxide-agent/src/apps/builder.rs`), checkpoints as whole copies in SQLite (`app_checkpoints`, 60 per app), the Preview calling the SDK as `draft:<id>` with the app's grants, screenshots drawn by the frame (DOM → SVG → PNG), the SDK's types for the model generated into `src/appframe/oxide-sdk.d.ts` (`npm run sdk-types`), Oxide's own Shiki-highlighted code editor (Phase 21) |
| Permission modes | **Manual** (ask, as before) and **Auto** per chat, picked in the composer. Auto lets reads, Allow-always grants and changes inside granted folders run, always asks for permanent deletes, computer use, paid generation and skills, and sends the rest to a safety-check model that sees only the user's messages and the tool calls (no assistant text, no tool results), two-stage and fail-closed; a stop shows the approval card with the reason (Phase 23) |
| Location | `C:\AI Apps\Oxide` |

### Repository layout (target)
```
Oxide/
├─ PLAN.md                  ← this file
├─ PROGRESS.md              ← phase log / hand-off notes (updated every phase)
├─ CLAUDE.md                ← conventions for any agent working in the repo
├─ Cargo.toml               ← workspace
├─ package.json, vite.config.ts, tsconfig.json, index.html
├─ src/                     ← React app
│  ├─ app/                  routes, layout, providers
│  ├─ components/           ui primitives + feature components
│  ├─ features/             chat, projects, artifacts, settings, quick-entry, …
│  ├─ bindings/             generated by tauri-specta (do not edit)
│  └─ styles/               tokens.css, fonts
├─ src-tauri/               ← Tauri app crate (thin: commands, events, windows, tray)
│  ├─ capabilities/         Tauri v2 permission files
│  ├─ binaries/             sidecars (uv-<target-triple>)
│  └─ tauri.conf.json
├─ crates/
│  ├─ oxide-core/           domain types: Conversation, Message, ContentBlock, ToolCall…
│  ├─ oxide-store/          SQLite repo layer + migrations
│  ├─ oxide-llm/            Provider trait + anthropic / openai_compat impls, streaming
│  ├─ oxide-mcp/            MCP client: transports, protocol, server manager, mcpb
│  ├─ oxide-agent/          reply generation, agent loop, tool registry, approvals, built-in tools
│  ├─ oxide-bridge/         Chrome WebSocket bridge server
│  ├─ oxide-pyenv/          uv bootstrap, venv management, python sidecar processes
│  ├─ oxide-sandbox/        Docker sandbox: workspace container per chat, exec, copies, file server
│  └─ oxide-computer/       screenshots + input (xcap, enigo) for computer use
├─ python/
│  └─ oxide_browser_agent/  Browser-Use MCP server (pyproject.toml, uv.lock)
├─ extension/               MV3 Chrome extension (TypeScript, built with tsc)
└─ .github/workflows/       CI + release matrix
```

---

## 1. How to run a phase (workflow)

**Start each phase chat with:**
> Run Phase N of PLAN.md for Oxide. Read PLAN.md and PROGRESS.md (in the project and in `C:\AI Apps\Oxide`) first.

**Every phase chat does this, in order:**
1. Read `PLAN.md` and `PROGRESS.md`. Check that the previous phase's exit criteria are met. If not, fix that first.
2. Create a task list from the phase's tasks.
3. Implement. Write files into `C:\AI Apps\Oxide` through the desktop app link.
4. **Verify** (see "Build & verify loop"). A phase is not done until its exit criteria pass on Windows.
5. Update `PROGRESS.md` with what was done, deviations from the plan, known issues, and notes for the next phase. Save it to the repo **and** to the project (`project_write`), so the next chat sees it without the folder.
6. If a decision in this plan changed, edit `PLAN.md` (in both places) and record why in `PROGRESS.md`.
7. Commit with message `phase N: <title>`.

### Build & verify loop
- **Editing files:** through the desktop link's file tools into `C:\AI Apps\Oxide`.
- **Running on Windows:** the Linux workspace on the PC cannot launch Windows GUI apps. `npm install`, `cargo tauri dev` and screenshot checks run in **PowerShell on Windows**, through computer use, or by the user when computer use isn't available.
- **Never share build outputs between Linux and Windows.** `node_modules/` and `target/` contain platform-specific binaries. Any quick Linux-side check uses `CARGO_TARGET_DIR=$HOME/oxide-target-linux` and a separate scratch copy for npm.
- **Checks per phase:** `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `npm run typecheck`, `npm run lint`, `npm test` (Vitest), then a manual run of `cargo tauri dev` against the exit criteria.

### Prerequisites (verify in Phase 0)
- **Windows:** Rust stable (MSVC), Visual Studio Build Tools ("Desktop development with C++"), WebView2 (preinstalled on Win 11), Node 20+, `cargo install tauri-cli --version "^2"`.
- **macOS:** Xcode Command Line Tools.
- **Linux:** `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libxdo-dev`, `libssl-dev`.
- Local test endpoints: LM Studio server running; Strata on `127.0.0.1:47831`.

---

## 2. Feature inventory (parity target)

This is what "all major features" means for Oxide. The phase that delivers each item is in brackets.

**Context management:** automatic chat compaction (older parts of a long conversation, and old tool results inside a long agentic reply, are replaced by a summary so the request fits the model's context window), a context meter, and recovery from "context full" errors [P13b].

**Chat:** streaming replies, stop, retry/regenerate, edit-and-resend with branch navigation (‹ 1/3 ›), copy, conversation title auto-generation, rename, star, delete, search, keyboard shortcuts [P3–P4]. Extended thinking toggle and collapsible thinking blocks [P3–P4]. Model picker in the composer [P3]. Attachments: images, PDFs, text and code files, drag-drop and paste [P4].

**Projects:** custom instructions, knowledge files, project-scoped chats, project memory [P7].

**Artifacts:** side panel, versions, code / Markdown / HTML / SVG / Mermaid / React renderers, copy and download, full-screen [P8].

**Connectors and tools:**
- Local MCP servers and remote (HTTP) MCP servers [P5].
- Tool approval prompts ("Allow once" / "Allow always" / "Deny") [P6].
- Desktop extensions (`.mcpb`) install and settings UI [P10].
- Skills (folders with `SKILL.md`) [P10]; models create and edit skills, with approval [P14c].
- Web search and web fetch tools [P6].

**Agentic work ("Cowork-style" tasks):**
- Folder access grants and a file tools sandbox [P11].
- Task list widget and a long-running task view [P11].
- Session side panel (like Claude desktop's): **Progress** (the task list) and **Used in this session** (tools, connectors, searches, files and folders the chat used) [P6 records usage, P11 panel].
- Code execution in the managed Python venv [P12].
- Docker sandbox workspace: a container per chat or task where the model can read, write, build and test code without touching the host, with files served by the official filesystem MCP server's Docker image [P12b].
- Scheduled tasks [P15].

**Browser:** Browser-Use agent [P13], Chrome extension control [P14].
**Computer use:** screenshot + mouse/keyboard with a visible control overlay and stop hotkey [P16].
**Desktop integration:**
- Tray icon, global quick-entry hotkey and floating window, screenshot-to-chat [P9].
- Notifications, single instance, `oxide://` deep links, start at login, window state restore [P9].

**Settings:** profile name, appearance (light / dark / system, chat font), providers and API keys, models, connectors, extensions, skills, permissions, data export and delete [P2, P5, P10, P17].
**Memory:** opt-in memory across chats, viewable and editable [P7].
**Voice dictation** [P18, stretch].
**Packaging:** installers for all three OSes, auto-update, CI [P17].
**Media generation:** images, video and music from chats (and Utility Apps) through OpenAI-compatible endpoints, with global default models per kind in Settings → Media generation [P19].
**Permission modes:** Manual and Auto (runs on its own, pauses to ask when an action looks risky), per chat, also for scheduled tasks [P23].
**Compact tool history:** a reply's tool calls fold into one quiet summary line that opens to a compact list, so long agentic replies don't fill the screen [P24].
**Utility Apps:** a sidebar page (after Projects) with a gallery of AI utility apps — small React apps in a sandboxed frame that use everything a chat can (each capability asked once per app), per-app model overrides, clone, export/import — and an AI Studio-style builder (chat + Preview/Code) to create and edit them [P20–P22].

---

## 3. Phases

Each phase lists **Goal → Tasks → Exit criteria**. Phases are ordered so the app runs and is testable after every one.

### Phase 0 — Scaffold & toolchain
**Goal:** an empty Oxide window runs with `cargo tauri dev` on Windows.
- Verify prerequisites (list above). Record versions in `PROGRESS.md`.
- `git init`. Add `.gitignore` (target, node_modules, dist, `src-tauri/binaries/*` except a README) and `.gitattributes` (LF for source).
- Create the Cargo workspace and empty crates from the layout. Use the Rust 2024 edition and pin the toolchain with `rust-toolchain.toml`.
- Scaffold the Tauri v2 app with React + TS + Vite. Set app id `dev.oxide.app`, product name `Oxide`, window 1200×800 with a minimum size of 760×520.
- Add Tailwind v4, ESLint, Prettier, Vitest, `tauri-specta` (with one `ping` command proving bindings generate), and a placeholder Oxide icon set.
- Write `CLAUDE.md` with repo conventions: crate boundaries, no `unwrap()` outside tests, `thiserror` in libraries and `anyhow` only in the app crate, `tracing` for logs, frontend folder rules, never edit `src/bindings`.
- Create `PROGRESS.md`.

**Exit:** `cargo tauri dev` opens a window that shows "Oxide" and the result of `ping`. Clippy, typecheck and lint pass.

### Phase 1 — Design system & app shell (static)
**Goal:** the UI looks like Claude desktop, running on mock data.
- `tokens.css`: warm off-white / near-black surfaces, a terracotta-style accent of Oxide's own choosing, borders, radii, shadows, and the type scale. Light and dark themes, system-follow.
- Load the bundled fonts.
- Custom title bar: Windows controls on the right, macOS traffic-light inset, Linux native decorations as a fallback.
- **Sidebar** (collapsible, resizable):
  - Oxide logo, "New chat" button.
  - Nav: Chats, Projects, Artifacts.
  - "Starred" and "Recents" lists.
  - User/profile menu at the bottom with Settings, Appearance and Help.
- **Home / new chat:** greeting with the user's name, a centered large composer, quick-action chips.
- **Conversation view:**
  - Centered column with a max width of about 768px.
  - User messages in subtle bubbles on the right. Assistant prose in serif with no bubble.
  - Hover actions (copy, retry, edit), and a "thinking" block style.
- **Composer:** auto-grow textarea, "+" menu (attach, add from project, connectors and tools toggles), model picker pill, extended-thinking toggle, send/stop button, drop-zone overlay.
- Static settings modal shell with left-nav sections.
- Routing: `/new`, `/chat/:id`, `/chats`, `/projects`, `/project/:id`, `/artifacts`, `/settings/*` (settings is a modal over the previous page).
- Keyboard shortcuts: Ctrl/Cmd+K (search), Ctrl/Cmd+Shift+O (new chat), Ctrl/Cmd+/ (shortcuts), Ctrl/Cmd+B (toggle sidebar).

**Exit:** side-by-side comparison screenshots (Oxide vs Claude desktop) are saved to `docs/ui-parity/`, layout differences are listed in `PROGRESS.md`, and both themes work.

### Phase 2 — Persistence, settings & secrets
**Goal:** real data replaces mocks.
- `oxide-core` types:
  - `Conversation { id, title, project_id?, model, starred, created_at, updated_at }`.
  - `Message { id, conversation_id, parent_id?, role, content: Vec<ContentBlock>, … }` (a parent pointer gives a tree for branches).
  - `ContentBlock` = Text | Image | Document | Thinking | ToolUse | ToolResult | ArtifactRef.
- `oxide-store`: sqlx + SQLite in the app data dir, migrations, repository functions, FTS5 table for search.
- Settings stored as typed JSON in SQLite. Provider API keys go through `keyring` and never appear in logs or IPC responses (masked).
- Providers settings page: add, edit and test endpoints for Anthropic, OpenAI, LM Studio and Strata, with prefilled defaults.
- Wire the sidebar lists, rename, star, delete and search to the store.

**Exit:** create, rename, star and delete persist across restarts. Search finds message text. Keys are stored in Windows Credential Manager.

### Phase 3 — LLM layer & streaming chat
**Goal:** real conversations with all four providers.
- `oxide-llm`:
  - `trait Provider { list_models(), stream(ChatRequest) -> Stream<ChatEvent> }`.
  - Unified `ChatEvent`: TextDelta, ThinkingDelta, ToolUseStart/Delta/End, Usage, Stop(reason), Error.
- **Anthropic client:** Messages API with SSE, tool use, extended thinking, images, PDFs, prompt-caching breakpoints, `anthropic-version` header, retry with backoff on 429/529.
- **OpenAI-compatible client** (hand-written on `reqwest`, sharing the SSE parser; custom base URL per provider): streaming, tool calls including parallel calls, images.
- **Capability detection:**
  - Anthropic and OpenAI: curated capability table, refreshed from the model list.
  - LM Studio: query its native models endpoint for the `tool_use` capability.
  - Strata: probe once with a tiny tool-call request and cache the result.
  - Models without native tools are hidden.
- Tauri commands: `send_message`, `stop_generation`, `regenerate`, `edit_and_resend`. Events stream to the frontend through a channel per conversation.
- Model picker lists models grouped by provider, with each conversation's model saved.
- Auto-title after the first exchange, using a cheap model chosen in settings.

**Exit:** streaming, stop, regenerate and edit-branching work against Anthropic, OpenAI, LM Studio and Strata. Thinking shows for Anthropic.

### Phase 4 — Chat UX completeness
- Markdown rendering: GFM tables, KaTeX, Shiki code blocks with language label, copy and wrap toggle. Streaming-safe rendering with no flicker or layout jump.
- Branch navigator ‹ n/m › on edited messages.
- Attachments: drag-drop, paste and file picker. Thumbnails and file chips. Images are resized and compressed in Rust (`image` crate; fit 1568 px, JPEG q85 or PNG when transparent). Files are copied into `<app data>\attachments` and messages store the copy's path.
  - PDFs go natively to Anthropic and as extracted text (`pdf-extract`) to others.
  - Text and code files are inlined with filename headers.
  - Size limits shown in the UI.
- Message actions: copy (Markdown), retry, edit, feedback (stored locally). Token and usage tooltip.
- **Thinking effort control** (requested after Phase 3; same idea as Claude desktop's Low / Medium / High / Extra / Max). Per model in the composer:
  - Anthropic: `output_config.effort`, with the levels from the Models API `capabilities.effort`.
  - OpenAI reasoning models: `reasoning_effort`.
  - LM Studio / Strata: whatever the server honors (tested first). At least thinking on/off.
    - Tested on 2026-11-05 (`scripts\win\local-thinking.ps1`): Strata honors `chat_template_kwargs.enable_thinking=false` and `reasoning_effort: "none"`; low/high made no measurable difference. So Strata gets an on/off switch (`ThinkingStyle::Switch`). LM Studio wasn't running and stays untested.
- Search modal (Ctrl+K) across titles and content.
- Export a conversation as Markdown or JSON.
- Scroll anchoring and a "jump to latest" button. Accessibility pass (focus rings, ARIA on composer and menus).

**Exit:** a 50-message conversation with code, math and images renders correctly and scrolls smoothly. Vitest covers markdown edge cases.

### Phase 5 — MCP client
- `oxide-mcp`:
  - JSON-RPC 2.0 framing over stdio: newline-delimited, stderr captured to a per-server log.
  - Streamable HTTP transport (POST with JSON or SSE response, `Mcp-Method` / `Mcp-Name` headers for 2026-07-28 servers, `Mcp-Session-Id` for legacy servers).
  - Version negotiation and dual-era fallback.
  - Supported: `tools/list|call`, `resources/list|read|templates`, `prompts/list|get`, `notifications/*_list_changed`, progress, cancellation, logging, `ping`, sampling (shown as a user-approved request), elicitation (rendered as a form), roots.
- **Server manager:** spawn, restart with backoff, stop on quit, status (starting / running / error), and env merge.
- **Windows specifics:**
  - Resolve `npx`/`node`/`uvx` through PATH plus `.cmd`/`.exe` suffixes.
  - Create child processes with `CREATE_NO_WINDOW`.
  - Kill the process tree on exit using a Job Object.
- `oxide_config.json` read and write with file-watch hot reload. "Import from Claude desktop" button (Windows `%APPDATA%\Claude\claude_desktop_config.json`, macOS `~/Library/Application Support/Claude/…`, Linux `~/.config/Claude/…`).
- **Settings → Developer / Connectors page:** server list with status dot, tool list, log viewer, "Edit config" button, add-server form.
- OAuth 2.1 + PKCE for remote servers that require it, via the system browser and a loopback redirect.
- Composer "+" menu shows connected servers with per-chat tool toggles. MCP prompts appear as attachable items and resources as attachable context.

**Exit:** the reference "everything" test server, the filesystem server (`npx`) and one `uvx` Python server all connect on Windows, list tools and run calls from a debug panel. A remote HTTP server connects.

### Phase 6 — Agent loop, approvals & built-in tools
- `oxide-agent`: the tool loop runs model → tool calls (in parallel where safe) → results → model, until a stop. Configurable max steps, cancellation, and partial results kept on stop.
- **Tool registry:** MCP tools (namespaced `server__tool`) plus built-in tools. JSON-schema normalization so the same tool definition works for both Anthropic and OpenAI formats.
- **Approval dialog:** inline card with tool name, server, arguments (pretty JSON) and "Allow once" / "Allow always for this chat" / "Deny". Read-only tool annotations can be set to auto-allow in Settings → Permissions.
- **Tool UI:** collapsible tool-call blocks that show input and result, with images rendered and errors styled.
- **Session usage log:** every tool call is recorded per chat (tool, server or built-in, a short label such as the search query, file or URL, time, and the message it belongs to), so the Phase 11 side panel can list what the chat used. Until then, the existing right-hand "In this chat" panel shows a simple "Used in this chat" list from this log.
- **Built-in tools:**
  - `web_search`, with selectable providers: Brave Search API, Tavily, SearXNG URL, or Anthropic's server-side web search when on Anthropic.
  - `web_fetch`: `reqwest` + Oxide's own HTML → Markdown extraction of the main content (`oxide-agent/src/tools/html.rs`; changed in Phase 6, see PROGRESS.md), with size caps.
- System prompt builder: date and time, user name, OS, enabled tools, project instructions slot, and memory slot.

**Exit:** a model chains several MCP and web tools with approvals in one turn on both Anthropic and an LM Studio model. Stop mid-loop is clean.

### Phase 7 — Projects & memory
- Projects: Phase 2 already stores projects (table, CRUD commands, "New project" dialog). This phase adds editing, archive, instructions editing, knowledge files (upload; text extracted and stored), and a project page listing its chats and knowledge.
- Context strategy: knowledge that fits the budget is included in full with cache breakpoints. Above the budget, a local BM25 index (SQLite FTS5's `bm25()` over passages of the files; changed from `tantivy` in Phase 7, see PROGRESS.md) and a `project_search` tool are used.
- Chats can be moved into and out of projects.
- **Memory (opt-in):**
  - After a conversation, a summarizer model proposes memory items.
  - Items are stored, then viewable, editable and deletable in Settings → Memory.
  - Memory is injected into the system prompt, per project or global.

**Exit:** project instructions and knowledge measurably change answers, large knowledge falls back to search, and memory can be viewed and deleted.

### Phase 8 — Artifacts
- Artifacts are created by a built-in `artifact` tool (`create` / `update` / `rewrite`, with id, type, title and content). This uses native tool calling and needs no tag parsing.
- Side panel opens beside the chat (resizable split), with a version switcher, copy, download, full-screen and a "Code ↔ Preview" toggle.
- **Renderers:**
  - Markdown and code (Shiki).
  - SVG (sanitized).
  - Mermaid (bundled).
  - HTML in a sandboxed iframe with its own CSP and no Tauri IPC.
  - React, transpiled with bundled `@babel/standalone` and rendered with bundled React, Tailwind and lucide. It works offline.
- Artifacts gallery page (`/artifacts`) across conversations.
- Inline artifact cards in the transcript.

**Exit:** each artifact type renders, updates create versions, and an HTML artifact cannot reach `window.__TAURI__`.

### Phase 9 — Desktop integration
- **Tray:** show/hide, new chat, quick entry, quit. The window close button hides to the tray (configurable).
- **Quick entry:**
  - Global shortcut (default Alt+Space on Windows/Linux, Option+Space on macOS, configurable) via `tauri-plugin-global-shortcut`.
  - A frameless always-on-top floating composer. Submitting opens the chat in the main window.
  - Optional screenshot or window capture attachment (`xcap`).
- Plugins: single-instance, deep-link (`oxide://chat/<id>`, `oxide://new?q=`), autostart, notifications (reply finished while hidden), window-state.
- Native menus: macOS app menu and Edit menu, plus standard accelerators.

**Exit:** the quick entry hotkey works system-wide on Windows, deep links open the right chat, and the app stays single-instance.

### Phase 10 — Extensions (.mcpb) & Skills
- **Desktop extensions:**
  - Install a `.mcpb` file (drag-drop or file picker). Unzip it and validate `manifest.json` (server type node/python/binary, entry point, `user_config` fields, compatibility).
  - Render the `user_config` form, with sensitive fields stored in the keychain.
  - Write a managed entry to the config and offer enable, disable, update and uninstall.
  - Node extensions use the system Node, or an error tells the user how to install it. Python extensions run through Oxide's uv.
- **Skills:** a folder containing `SKILL.md` with name and description frontmatter. The skills directory lives in app data. Skill names and descriptions go in the system prompt, and a built-in `load_skill` tool returns the body and files.
- Settings pages for Extensions and Skills (add, enable/disable, remove).

**Exit:** a sample `.mcpb` installs, configures and runs, and a sample skill is discovered and used by the model.

### Phase 11 — Agentic tasks (Cowork-style)
- **Folder grants:** the user adds folders to a chat (picker). Built-in file tools (`list_dir`, `read_file`, `search_files`, `write_file`, `edit_file`, `move_file`, `delete_file`; names changed in Phase 11, see PROGRESS.md) are confined to granted roots after symlink and `..` resolution. Reading never asks; changes ask. Delete moves files to a `_to_delete` folder by default, or deletes for good when the chat allows it.
- **Session side panel** (extends the right-hand "In this chat" panel; requested by the user after Phase 4, modelled on Claude desktop's visible behavior):
  - **Progress:** a `task_list` tool (create/update) renders the chat's steps with live status (pending / in progress / done) and a "Step n of m" pill in the title bar that opens the panel.
  - **Used in this session:** a list built from the Phase 6 usage log: web searches (with the query), web fetches, MCP connectors and their tools, files and granted folders, the linked computer or browser. Repeated uses of the same kind collapse into one row with a count or source icons; clicking a row scrolls to the tool call in the transcript.
  - The panel can be closed and reopened, and remembers its state per chat.
- Long-running mode: the agent keeps going while the window is hidden, sends a notification on completion or question, and shows a "needs your input" badge.
- **Shell tool (off by default):** commands run in a granted folder, with an approval per command or an allowlist, a timeout, output capture, and Windows PowerShell vs POSIX sh detection.

**Exit:** "organize this folder by file type and write a summary" runs end-to-end with approvals and a live task list in the session panel (Progress and Used in this session both fill in), and path escape attempts are blocked by tests.

### Phase 12 — Managed Python (uv) & code execution
- `oxide-pyenv`:
  - The `uv` sidecar per target triple is fetched by `scripts/fetch-uv.mjs` (`npm run fetch-uv`) into `src-tauri/binaries/` and checksum-pinned in `scripts/uv-pins.json` (a hash is recorded the first time a target is downloaded; release builds fail when the sidecar is missing).
  - First run: `uv python install` a pinned CPython, then `uv venv` in `<app_data>/python/venv` (changed from `.venv` in Phase 12), then `uv sync` from `python/` using a locked `uv.lock`.
  - Progress UI on first run, with repair and reset in Settings → Advanced.
- `run_python` built-in tool: executes in a per-chat working dir inside app data (or a granted folder) with a timeout and stdout/stderr/file capture. Generated files (charts, CSV) attach to the reply and can open in Artifacts.
- Pre-installed analysis packages: pandas, numpy, matplotlib, openpyxl.

**Exit:** a fresh Windows user profile goes from no Python to working `run_python` without manual steps, and a chart image comes back into the chat.

### Phase 12b — Docker sandbox workspace
Requested by the user after Phase 5: give Oxide's agent an isolated workspace like the cloud workspace the build agent used, inside a Docker container on the user's machine.
- **Detection and setup:** check that Docker (Docker Desktop on Windows/macOS, Docker Engine on Linux) is installed and running, with a readable message and an install link if not. Settings → Sandbox: on/off, image, CPU/memory limits, network on/off (off by default), and where workspaces live.
- **Workspace container per chat or task:** a named volume (or a folder under app data) mounted at `/workspace`, created on first use, kept until the chat is deleted or the user clears it. Containers run without privileges, with resource limits, no host mounts beyond the workspace, and stop when idle.
- **Files:** the official reference filesystem MCP server, run from its Docker image (`mcp/filesystem` at the time of writing; check the modelcontextprotocol/servers README), launched as a stdio server per chat (`docker run -i --rm --mount type=volume,src=<workspace>,dst=/workspace mcp/filesystem /workspace`) through `oxide-mcp`'s client, not as an MCP-manager entry (manager servers are shared by all chats; changed in Phase 12b, see PROGRESS.md). It shows in Connectors as a built-in server and its tools go through the sandbox's approvals.
- **Running code:** a built-in `sandbox_exec` tool runs commands inside the workspace container (`docker exec`), with a timeout, output capture and a size cap, so the model can install packages, build and run tests there. Default image: a small Linux image with common toolchains (configurable).
- **Moving files in and out:** "Copy into sandbox" from a granted folder (Phase 11), and "Save to folder" / download for results, both with approval. Nothing in the sandbox can reach the host otherwise.
- **UI:** the session side panel's "Used in this session" lists the sandbox, and a Sandbox tab shows its files, running commands and a Reset button.

**Exit:** with Docker running on Windows, a chat can create a project in its sandbox, build and test it there (including a failing test the model then fixes), and copy the result to a granted folder; with Docker stopped, the user gets a clear message; host paths outside the workspace are unreachable (tested).

### Phase 13 — Browser agent (Browser-Use)
(Built as one server per chat, preferring an installed Chrome or Edge; changed in Phase 13, see PROGRESS.md.)
- `python/oxide_browser_agent`: an MCP server (official `mcp` Python SDK, stdio) wrapping Browser-Use.
  - Tools: `browser_task(goal)` for autonomous mode, plus granular `navigate`, `read_page`, `click`, `type`, `screenshot` and `close`.
  - Browser-Use's own LLM calls go back through **Oxide's selected provider** via env-provided endpoint and key, so it uses the same models.
- Playwright Chromium is installed into the managed venv on first use (with progress), or an existing Chrome is reused via CDP when the user allows it.
- Registered as a built-in MCP server (shown in Connectors, can be toggled), with a live screenshot feed in the tool block.

**Exit:** "find the opening hours of X and summarize" completes with screenshots on Windows.

### Phase 13b — Context management & chat compaction
(Anthropic uses server-side clearing, not server-side compaction; see PROGRESS.md.)
Requested by the user after Phase 13: a long agentic reply on Strata `swift-iq2_xs` (128K context) made 35 tool calls in one reply (mostly `web_fetch` of 3–40k characters each, ≈ 410 KB of tool results) and filled the context to 100% before failing. Nothing in Oxide limits how much a conversation or a single reply sends.
- **Know the budget:** each model's context window (`ModelInfo.context_window` where the provider reports it; Strata and LM Studio from their model listing or a per-model override in Settings → Models; a safe default when unknown) minus the reply's output reserve. Token counts are estimated locally (characters/4, calibrated per chat from the provider's reported input tokens of the previous request).
- **Inside a running reply (the tool loop):** before every model request, if the estimated prompt is above a threshold (default 75% of the window):
  1. **Clear old tool results first:** results older than the last few steps are replaced by a short stub (`[cleared: web_fetch https://… · 40k characters — call it again if you need it]`), keeping the tool calls themselves, so the model sees what it already did.
  2. **Then compact:** if still above, the oldest part of the conversation is summarized by a model (the chat's own, or a configurable "compaction model") into a structured summary (the task, decisions, facts found with their sources, files made, what is left) that replaces it in the request.
- **Across turns:** the same compaction runs before a new reply when the history no longer fits. The summary is **stored** with the chat (it replaces the covered messages in requests; the transcript keeps everything), so it is made once, not every turn.
- **Tool output sizes follow the window:** `web_fetch`, `read_page`, file reads and MCP results get per-result caps scaled to the context window (e.g. at most ~10% of it), so a small-context model is never handed 40k characters at once.
- **Recovery:** a "context full" answer from the provider (`StopReason::ContextFull`, HTTP 400 "context length", Strata/LM Studio's equivalents) triggers one automatic compaction and a retry instead of failing the reply.
- **Anthropic:** use the API's own context-management features (server-side clearing of old tool results / compaction) when the model supports them, checked against the API docs at implementation time; Oxide's own compaction is the fallback everywhere else.
- **UI:** a context meter in the composer (fill of the current chat vs the model's window), a "Conversation compacted" divider in the transcript with the summary viewable, a **Compact now** action, and Settings → Models: compaction on/off, threshold, compaction model.
- **Tests:** a fake provider with a tiny window drives a long tool loop and a long multi-turn chat through clearing, compaction and the context-full retry; the stored summary is reused.

**Exit:** the reply that failed on 2026-11-12 (“can you make me a spread sheet with all the greatest male badminton players over the last 10 years?” on Strata `swift-iq2_xs`, 128K) finishes on Windows with the context staying below the window (Strata's monitor never reaches 100%), the transcript shows where it compacted, and a 50-turn chat keeps working past the window.

### Phase 14 — Chrome extension & WebSocket bridge
(WebSocket hand-written instead of `tokio-tungstenite`, extension built with `tsc` instead of Vite; see PROGRESS.md.)
- `oxide-bridge` (Rust, `tokio-tungstenite`):
  - Listens on `127.0.0.1` only, on a fixed default port that can be changed.
  - Accepts only `chrome-extension://<id>` origins and requires a **pairing token**: Oxide shows a 6-word code, the user pastes it into the extension popup, and the bridge exchanges it for a long-lived token stored in the keychain and in `chrome.storage.local`.
  - Messages are JSON-RPC 2.0 with heartbeat and reconnect.
- `extension/` (MV3, TypeScript, Vite):
  - Service worker plus content scripts and a popup (pairing, status, site permissions).
  - Tools: `tabs_list/create/select/close`, `navigate`, `read_page` (accessibility-tree-style text with element refs), `find`, `click`, `type`, `press_key`, `scroll`, `form_input`, `screenshot` (`captureVisibleTab`), `get_console`.
  - A per-site permission prompt in the extension, a visible "Oxide is controlling this tab" banner, and a Stop button.
  - Keep-alive is handled via the WebSocket and an alarms fallback for MV3 service-worker sleep.
- Oxide exposes these as a built-in tool group ("Chrome"), with pairing UI in Settings → Chrome.
- Load-unpacked instructions now; Chrome Web Store packaging notes go in `extension/README.md`.

**Exit:** pairing works, and the model can open a tab, read it, click, type and screenshot. Unpaired or foreign-origin connections are rejected (tested).

### Phase 14c — Models create and edit skills
Requested by the user after Phase 14: today models can only read skills (`load_skill`); creating, editing, switching off and removing them is done by hand in Settings → Skills (Phase 10). The skill store (`oxide-agent/src/skills.rs`: `Skills::create`, `update`, `detail`, `skill_md`) already does the work.
- **A built-in `save_skill` tool:** create a new skill or change an existing one (by name or folder) with `name`, `description` and the instructions (the body of `SKILL.md`), plus optional extra text files inside the skill's folder (e.g. `reference.md`, `scripts/x.py`), confined to that folder (no `..`, no absolute paths, size caps).
- **Always asks:** a skill shapes later chats, so every save shows an approval card with the whole `SKILL.md` (and, for an edit, what changes: a diff against the current version) and "Allow once" / "Deny" — no "Allow always". A skill installed from a folder or zip can only be changed after the card says so plainly.
- **Discovery:** `load_skill`'s listing (or a small `list_skills` tool) gives models the existing skills' names, descriptions and folders, so they edit instead of duplicating; names stay unique (`skill_slug`).
- **System prompt:** when the tool is offered, models may propose turning a repeated workflow into a skill ("turn what we just did into a skill") but save only when the user asks or agrees.
- **UI:** the saved skill appears in Settings → Skills like any other (edit, switch off, delete there); the tool line reads "Created skill …" / "Updated skill …" with a link to it; the chat's "+" menu can switch the tool off per chat; "Used in this session" lists it.
- **Tests:** create, edit, unique names, files confined to the skill's folder, the approval card's content and diff, a denied save writes nothing, the new skill is offered to the next reply.

**Exit:** in a chat, "turn what we just did into a skill" shows the skill in an approval card, saves it on Allow, and a new chat uses it; asking to change it later edits the same skill (diff shown) instead of making a second one.

### Phase 15 — Scheduled tasks
- Scheduler in Rust (cron expressions plus one-shot jobs) persisted in SQLite. Each run starts a fresh conversation with the saved prompt, model, tools and folder grants.
- The approval mode per task is chosen at creation, and "needs approval" runs pause and notify.
- UI: Scheduled tasks page (create, edit, pause, run now, history).
- Runs while Oxide is in the tray. Missed runs are handled on next launch (configurable).

**Exit:** a daily task runs on time from the tray, and its history shows outputs.

### Phase 16 — Computer use
- `oxide-computer`: screen capture (`xcap`, multi-monitor, scaled for the model), input (`enigo`: move, click, drag, type, keys, scroll), DPI handling on Windows.
- Exposed as the Anthropic computer-use tool type when on Anthropic, and as equivalent function tools for OpenAI-compatible models.
- **Safety:**
  - An explicit per-session enable, and a per-app allowlist prompt.
  - A visible always-on-top "Oxide is controlling your computer" bar with Stop.
  - A global panic hotkey (Ctrl+Alt+Esc).
  - Credential and financial fields are never typed into.

**Exit:** the model opens Notepad, types text and saves a file, and the panic hotkey stops it immediately.

### Phase 17 — Packaging, updates & CI
- Bundles:
  - Windows: NSIS + MSI with WebView2 bootstrapper.
  - macOS: `.dmg` with universal binary, signing and notarization notes.
  - Linux: AppImage + `.deb` + `.rpm`.
- Sidecars included per target.
- `tauri-plugin-updater` with signed update manifests.
- GitHub Actions: lint and test on 3 OSes, plus a release matrix producing artifacts. The extension zip is built in CI.
- Settings → Data: export all (zip of JSON and attachments), delete all.
- CSP and capabilities audit: least privilege per window (main, quick entry, artifact frames).

**Exit:** installers from CI install and run on clean Windows 11, macOS and Ubuntu VMs.

### Phase 18 — Hardening & polish (+ stretch)
- **Security review:**
  - Path sandbox tests, IPC permission review, bridge fuzzing.
  - MCP stdio command trust prompts on first launch of a new server.
  - Secret redaction in logs.
  - Linux keychain: `keyring`'s `linux-native` (kernel keyutils) loses keys at reboot; switch Linux to the Secret Service (`sync-secret-service`, needs D-Bus) with keyutils as the fallback (found in Phase 17).
- Performance: virtualized message list, startup time under 1.5 s target, memory profile.
- i18n scaffolding (English and Danish).
- **Stretch:** voice dictation (local `whisper-rs` or provider STT), a usage dashboard (tokens per provider), and conversation sharing via export to HTML.

**Exit:** every checklist in this plan is ticked, and known issues are triaged in `PROGRESS.md`.

*Done 2026-11-19* (see PROGRESS.md). Deviations: a message window instead of a virtualized list; provider speech-to-text instead of `whisper-rs`; start-up time to be measured on the next release build. Remaining `npm audit` findings (Vitest/tinypool, dev-only) are closed by upgrading Vitest to 5 before Phase 19.

### Phase 19 — Generation endpoints & media generation (OpenAI-compatible)
Requested by the user after Phase 17 (planning for Utility Apps): Oxide must generate images, video and audio, with global default models per kind of generation. **Decided with the user (2026-11-14): every generation endpoint speaks the OpenAI-compatible API** — OpenAI itself, Gemini's and Groq's compatible endpoints, and most local setups (LM Studio, Strata, LocalAI, vLLM, …). Anthropic's and Google's own APIs come later. Chats keep their existing providers.
- **Media generation** in `oxide-llm` on the OpenAI-compatible API only: images (`/images/generations`, and `/images/edits` with reference images; size, count, base64 or URL results), video (`/videos`: create, poll the job with progress, download the result) and music (`/music/generations`: style tags, optional lyrics, duration; one synchronous request; changed from speech `/audio/speech` by the user on 2026-11-19, see PROGRESS.md). Shapes are checked against OpenAI's API reference when the phase is built. A server without an endpoint gets a plain “this endpoint doesn't offer … generation” message; no provider-specific native calls.
- **Settings → Media generation** (global defaults; renamed from “Generation”, and the Text group dropped, by the user on 2026-11-19, since text comes from the chat models in Providers and Models): three groups — **Image, Video, Music generation endpoint** — each with a model (picked from an endpoint's model list, or typed), an optional **custom URL** (`https://…/v1`) and an optional **API key** (keychain) overriding the endpoint's key, the chosen model shown in the group's header (like AI Studio's “Generation endpoints”), and **Test** per group.
- **Chat tools** `generate_image`, `generate_video`, `generate_audio` (ask before running; costs can be high): results saved as attachments, shown in the transcript (image, video player, audio player) with download, and listed in Used in this chat.
- **Tests**: fake OpenAI-compatible servers for images, speech and video (incl. job polling, failures and missing endpoints), settings round trip, tool results.

**Exit:** in a chat on Windows, “make an image of …”, “make a 5-second video of …” and “make a 30-second song about …” work against OpenAI-compatible endpoints set in Settings → Media generation (a local server where one offers the endpoint, else OpenAI); changing a default changes which model and endpoint are used.

*Built 2026-11-19* (see PROGRESS.md). OpenAI shut down its Videos API (Sora 2) on 2026-09-24, so the video check uses a gateway that kept OpenAI's `/videos` API. Test in Settings → Media generation checks the model against the endpoint's model list instead of generating (no cost).

### Phase 20 — Utility Apps: runtime and gallery
Requested by the user after Phase 17: a **Utility Apps** page (sidebar item after **Projects**) with a collection of AI utility apps, modelled on the user's AI-Toolkit (`github.com/sitefactorydk-collab/AI-Toolkit`, built in Google AI Studio). Apps can be used, cloned, modified and created; sharing as a community platform may come later.
- **What an app is:** a small React + TypeScript web app (several files) with a manifest `oxide-app.json` (id, name, description, icon, category, version, entry file, the capabilities it uses, default models per kind of generation). Stored in `<app data>\apps\<id>\`. It runs in a sandboxed frame on its own URI scheme (`oxide-app`, like the artifact frame: own origin, no Tauri IPC), compiled in the frame with the bundled Babel and an import map of bundled libraries (React, Tailwind, lucide, react-markdown, jszip, …).
- **The `oxide` SDK in the frame** (postMessage to Oxide, typed, with a `.d.ts` for the builder): everything a chat can do — text generation with streaming, images and PDFs as input, structured JSON output; image, video and music generation (Phase 19); web search, web fetch (page → Markdown), site crawl that respects robots.txt; MCP connector tools; files (pick, drop, save, zip); managed Python, the Docker sandbox, the browser agent and Chrome; per-app storage. **Each capability asks once per app** (an approval card naming the app and the capability, “Allow for this app” / “Deny”), revocable in the app's settings.
- **Per-app settings:** the app's text model (a chat model from Providers and Models; default: the first available, or the one the app's manifest names) and overrides of the global media generation defaults (Image, Video, Music: model, custom URL, API key) for that app; the app's granted capabilities; its storage (clear).
- **Gallery** (`/apps`): cards with icon, name, description and category, search, filter by category; an app opens full-page with a slim header (name, **Settings**, **Clone**, **Edit** for own apps, **Export**). Built-in apps are read-only; **Clone** makes an editable copy under “My apps”.
- **Built-in apps** (ported from AI-Toolkit onto the SDK; the rest come in Phase 22): **AI Meta Tag Generator**, **Readability & Tone Analyzer**, **AI HTML Table from Image** and **AI SVG Creator** (text, page fetching, image input and code/SVG output between them). The user can change this pick.
- **Sharing groundwork:** export an app as one `.oxideapp` file (zip of the folder) and import one (with its manifest and capabilities shown before installing); apps are included in Settings → Data → Export all and removed by Delete all.
- **Tests**: manifest validation, SDK messages and capability approvals (once per app, revoke), the frame has no IPC (as in Phase 8), export/import round trip, the four built-in apps' happy paths against fake providers.

**Exit:** on Windows, the four built-in apps run from the Utility Apps page with the global models and with a per-app override; each capability asks once; a cloned app can be exported and imported again.

*Done 2026-11-20* (see PROGRESS.md). Deviations: files and storage don't ask; Deny lasts for the visit; Edit is a plain file editor until Phase 21; JSON output by instruction with one retry; the app frame's CSP allows no network at all.

### Phase 21 — Utility App builder (build mode)
Requested by the user: creating and editing apps “much like AI Studio's build mode”, with a switch between **Preview** and **Code**.
- **Builder view** for **New app**, **Clone** and **Edit**: a chat on the left with a chat model picked in the builder (from Providers and Models) that builds the app; on the right **Preview | Code**. Preview reloads live after each change; Code shows the file tree and an editor (syntax highlighting, save, find) so the user can read and edit any file.
- **Builder tools for the model** (scoped to the app's folder): list/read/write/edit/delete files, update the manifest (name, description, icon, category, capabilities, default models), read the preview's build errors and console, and take a screenshot of the preview. The system prompt carries the SDK's types and the app conventions. Errors in the preview offer **Fix with AI**.
- **Checkpoints:** every model turn and every manual save makes a version; **Restore** goes back. New apps start from a template.
- **Publish:** an app under construction is a draft until **Save to Utility Apps**; changing name, icon and category from the builder.
- **Tests**: builder tools confined to the app folder, checkpoints and restore, compile errors reported to the model, the code editor's save reaching the preview.

**Exit:** on Windows, “build an app that turns a pasted CSV into a chart and lets me download it as PNG” is built from scratch in the builder, its code is changed by hand in Code view, a built-in app is cloned and modified, and both are exported.

*Done 2026-11-20* (see PROGRESS.md). Deviations: the builder chat is an ordinary hidden chat; drafts are folders and the Preview runs them as `draft:<id>`; screenshots are drawn by the frame; the code editor is Oxide's own (no new dependency); builder tools don't ask (checkpoints undo); Clone stays a copy in My apps and Edit opens the builder.

### Phase 22 — More Utility Apps (rest of AI-Toolkit)
- Port three more AI-Toolkit tools onto the SDK, as picked by the user on 2026-11-20: **Multi Purpose AI Translator**, **AI Plugin Creator** and **llms.txt Generator**; each as a built-in app with its own test. The other AI-Toolkit tools (AI Web Auditor, AI SEO Recommendations, AI Schema Markup Generator, AI FAQ Questions Generator, Markdown for AI Agents testing tool, AI SFT Dataset Generator, SP Page Builder - AI) are not ported.
- Later (not planned yet): a community catalogue for sharing apps.

**Exit:** each of the three ported apps does on Windows what its AI-Toolkit original does, using Oxide's models and capabilities.

*Built 2026-11-20* (see PROGRESS.md). Deviations: llms.txt crawls with Oxide's `web.crawl` (≤ 50 pages per site, same origin, robots.txt); the Translator shows the model instead of token counts and translates text files like pasted text; translation styles live in the app's storage; no temperature option in the Plugin Creator.

### Phase 23 — Permission modes: Manual and Auto
Requested by the user on 2026-11-21: an **Auto** mode like the Claude desktop app's, where the model works on its own and pauses to ask only when something looks unsafe. The design follows Anthropic's public description of auto mode (anthropic.com/engineering/claude-code-auto-mode, read 2026-11-21) and the desktop app's visible picker; the wording in Oxide's UI is Oxide's own.
- **The picker:** a **Permission mode** menu in the composer, beside the model picker, with two entries, each with a one-line description: **Manual** (today's behaviour: Oxide asks before tools that change things, with Allow once / Allow always for this chat / Deny) and **Auto** (Oxide works on its own and stops to ask when an action looks risky). The button shows the current mode. The mode is kept per chat; new chats take the default from Settings → Permissions (Manual unless changed).
- **How Auto decides, in order (the first step that applies wins):**
  1. **Always ask**, even in Auto: deleting files for good (not `_to_delete`), computer use (its own per-app consent stays), `generate_image`/`generate_video`/`generate_music` (they can cost money), `save_skill`, and anything the user marked "always ask" in Settings → Permissions.
  2. **Runs without a check:** read-only tools (built-in reads and searches, web search and fetch, MCP tools annotated read-only, the task list), and what the user already allowed for this chat with Allow always — except Allow always for the shell and `run_python`, which in Auto still go to the check (they run arbitrary code).
  3. **Runs without a check:** changes inside the chat's granted folders (write, edit, move, delete to `_to_delete`), and `sandbox_exec` when the sandbox has no network.
  4. **Everything else goes to the safety check:** shell, `run_python`, MCP tools that change things, the browser agent and Chrome actions, the sandbox with network on, file actions outside granted folders.
- **The safety check:** a classifier model (Settings → Permissions → **Safety check model**: default the chat's own model; a local model is recommended to keep costs down). It sees the user's messages and the model's tool calls in this chat, including the one to judge, but **not** the model's own text or any tool results, so neither the model nor injected page or file content can argue it into allowing something. Two stages: a fast yes/no that leans toward asking, then, only when stage 1 flags the call, a second look with reasoning to cut false alarms. It blocks by default anything that destroys or sends away data (mass deletes, uploading files or secrets to outside services), weakens security (changing Oxide's own permissions or config, disabling protections, installing things that start themselves), crosses trust boundaries (running code downloaded from the internet, reading credential stores), or affects other people or shared things (sending messages or e-mail, publishing, pushing, deploying, payments) — unless the user's own messages clearly asked for exactly that. Anything the model decided on its own counts as not asked for. If the check fails, times out or gives an unreadable answer, Oxide asks (fail closed).
- **When the check stops an action:** the reply pauses on the usual approval card, now with the check's short reason (“Stopped because: it sends the folder's files to an outside server”); Allow lets it run, Deny tells the model it was refused and to find a safer way without working around the refusal. Hidden in the tray, Oxide sends a notification and shows the “needs your input” badge (as in Phase 11).
- **Visible afterwards:** tool lines run in Auto get a small **Auto** mark (with why: read-only, in your folder, checked), and **Used in this session** shows how many actions ran automatically and how many were stopped.
- **Scheduled tasks:** **Auto** becomes an extra approval mode beside the existing ones (Phase 15); a stopped action pauses the run as “needs approval” and notifies. Utility Apps and the app builder are unchanged.
- **Tests:** each step's routing (always-ask list, read-only, granted folders, Allow always kept or not), the check's input (no assistant text, no tool results), the two stages, fail closed on errors and timeouts, a stop shows the card with its reason and Deny reaches the model, the mode per chat and its default, scheduled tasks in Auto, the picker and the Auto marks in Vitest.

**Exit:** on Windows, a chat with a granted folder in **Auto**: “organize this folder by file type and write a summary” runs to the end with no approval cards and Auto marks on its tool lines; asking it to delete the files for good, to write outside the folder, or to run a shell command that downloads and runs a script stops with a card and a sensible reason; switching to **Manual** brings today's cards back; the same works with a local model (LM Studio) and with Claude; a scheduled task in Auto runs and pauses on a stopped action.

*Done 2026-11-22* (see PROGRESS.md). Deviations: generation runs in Auto once allowed always for the chat; file changes outside the shared folders always ask with a reason instead of going to the check (the file tools can't reach there in any mode); the shell allowlist and the browser, Chrome and sandbox "don't ask" switches don't skip the check in Auto; scheduled tasks keep "Run them without asking" as a third mode. Added after the manual checks: download-and-run shell commands always ask (Oxide's own pattern check, not the model), and cards Auto shows every time have no Allow always.

### Phase 24 — Compact tool history in the transcript
Requested by the user on 2026-11-21 (with side-by-side screenshots): in a long agentic reply every tool call is a full-width bordered card (icon, verb, target, Auto mark, chevron), each thinking step a line of its own, so twelve calls fill a whole screen before the answer. The Claude desktop app shows the same work as one quiet line. Oxide's version follows that visible behaviour; the wording is Oxide's own.
- **Groups:** consecutive tool calls in a reply (and the thinking between them) form one group, ended by the model's prose. A finished group is **one muted line** with no card around it: a summary of what was done, built from the calls' kinds and counts in plain words (“Listed a folder 4 times, ran 3 commands, wrote a file”; failures counted, e.g. “1 failed”), and a chevron. Prose stays as it is.
- **Opened:** the line opens a single bordered box with a compact row per step (no box per row, thin dividers, smaller text): thinking as a short muted line (its first sentence; click for all), each call as “Ran mkdir …”, “Listed C:\…\Downloads” with a chevron that opens its input and result as today. Auto marks shrink to a small ⚡ icon with the mark's text in a tooltip; **Failed** stays visible in red on its row and in the summary.
- **While a reply runs:** the current group is open and shows its steps live, ending with the running one (“Running mkdir …”, “Safety check …”); when the model starts writing prose the group collapses to its summary line. A setting (Settings → Appearance: **Show tool steps** collapsed / expanded) keeps groups open for users who want them.
- **Never hidden:** approval cards (also Auto's stops with their reason), computer-use cards, artifacts, generated images, video and music, and the browser's live screenshots stay outside the fold, in place, as now. A group with a waiting approval stays open.
- **Elsewhere:** “jump to the tool call” from Used in this session opens the group and scrolls to the row; search, export (Markdown, JSON, HTML) and Copy are unchanged (they use the messages, not the view).
- **Tests (Vitest):** grouping (prose splits groups; approvals, artifacts and media stay outside), the summary wording (kinds, counts, failures, singular/plural, Danish keys), open/close and the live group collapsing when prose starts, the setting, failures and Auto marks in compact rows, jump-to-call opens the group.

**Exit:** on Windows, the “organize this folder” chat from Phase 23's checks shows its tool work as one or two summary lines above the answer; opening one shows the compact rows with their marks and failures; a running reply shows its steps live and folds them when the answer starts; approval cards still appear in full; Settings → Appearance → Show tool steps: expanded brings back open groups.

---

## 4. Phase hand-off template (append to PROGRESS.md)
```
## Phase N — <title> — <date>
Status: done | partial
Done:
- …
Deviations from PLAN.md (and why):
- …
Known issues / TODO carried forward:
- …
Notes for Phase N+1:
- …
Verified: clippy ✅ tests ✅ typecheck ✅ lint ✅ tauri dev on Windows ✅
```
