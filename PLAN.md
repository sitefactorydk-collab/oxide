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
| Chrome link | Own MV3 extension that talks to Oxide over a localhost **WebSocket with token pairing** |
| Storage | SQLite via `sqlx` (migrations checked in). API keys in the OS keychain via `keyring`. Project knowledge search uses SQLite FTS5 with BM25 ranking (Phase 7) |
| OS dialogs and links | `tauri-plugin-dialog` (file picker, save dialog) and `tauri-plugin-opener` (links open in the system browser), called from Rust commands so the frontend still only uses `@/bindings` (Phase 4) |
| IPC typing | `tauri-specta` generates TS bindings from Rust commands and events. No hand-written IPC types |
| Frontend state | Zustand for UI state, TanStack Query for backend data |
| Styling | Tailwind CSS v4 + CSS variables design tokens. Radix UI primitives for menus, dialogs and tooltips |
| Markdown | `react-markdown` + `remark-gfm` + `remark-math`/`rehype-katex` + Shiki for code (JavaScript regex engine, since the CSP blocks WASM; colours from a CSS-variables theme in `tokens.css`). Model output is pre-processed (`components/markdown/prepare.ts`) for LaTeX delimiters, prices vs `$math$`, and partial input while streaming |
| Desktop integration | Official Tauri plugins: single-instance (registered first), deep-link (`oxide://`), global-shortcut, autostart (`--hidden` starts in the tray), notification, window-state. Tray via Tauri's `tray-icon` feature. Quick entry is a second, hidden-at-startup webview window (`index.html?window=quick`); screenshots via `xcap` in `oxide-computer` (Phase 9) |
| Artifacts | Built-in `artifact` tool (create / update / rewrite). HTML and React run in an `<iframe>` sandboxed without `allow-same-origin`, loaded from Oxide's own `oxide-artifact` URI scheme with its own CSP (inline scripts yes, network requests no) — not `srcdoc`, which would inherit the app's CSP (Phase 8) |
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
├─ extension/               MV3 Chrome extension (TypeScript, built with Vite)
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
- Skills (folders with `SKILL.md`) [P10].
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
- Performance: virtualized message list, startup time under 1.5 s target, memory profile.
- i18n scaffolding (English and Danish).
- **Stretch:** voice dictation (local `whisper-rs` or provider STT), a usage dashboard (tokens per provider), and conversation sharing via export to HTML.

**Exit:** every checklist in this plan is ticked, and known issues are triaged in `PROGRESS.md`.

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
