# Oxide

A desktop AI assistant for Windows, macOS and Linux, written in Rust on [Tauri v2](https://tauri.app) with a React + TypeScript front end.

Oxide works like the Claude desktop app: the same layout, chats and projects, artifacts, MCP connectors, agentic tasks in your folders, and browser and computer use. It talks to several model providers instead of one:

- **Anthropic**
- **OpenAI**
- **LM Studio** (local, `http://127.0.0.1:1234/v1`)
- **Strata** (local, `http://127.0.0.1:47831/v1`)
- any other **OpenAI-compatible** endpoint

Oxide offers only models with native function calling, so tools work the same with every model it lists.

Two things go beyond what the Claude desktop app offers:

- **[Utility Apps](#utility-apps):** small AI apps that run inside Oxide, with a builder for making your own.
- **[Local models](#local-models-first-class):** everything also works with models running on your own computer.

> Oxide is an independent project. It is not made by, affiliated with or endorsed by Anthropic. It was built clean-room from the Claude desktop app's visible behaviour and public specifications (MCP, `claude_desktop_config.json`, `.mcpb`). It uses its own name, logo and open-licensed fonts (Inter, Source Serif 4, JetBrains Mono) and contains no Anthropic code, assets or text.

## Utility Apps

Utility Apps are small, focused AI tools — a translator, an SEO helper, a plugin generator — that sit on their own page in the sidebar, next to Projects. Each one is a little React + TypeScript app with a simple form-and-result interface. It does one job well without you writing a prompt each time.

**What an app can use.** Apps call Oxide through a typed SDK (`oxide`). With it they can use most of what a chat can:
- text generation with streaming, images and PDFs as input, and JSON output that follows a schema;
- image, video and music generation;
- web search, reading web pages as Markdown, and crawling a site (respecting `robots.txt`);
- MCP connector tools;
- picking, dropping, zipping and saving files, and reading and writing in one folder you pick for the app;
- storage that belongs to the app.

**Safe by design.**
- Every app runs in a sandboxed frame on its own origin, with no access to Oxide's internals and no network of its own. All requests go through Oxide.
- Each capability (AI text, reading web pages, …) asks once per app.
- You can revoke a capability in the app's settings.

**Your models, per app.**
- An app uses your global defaults, or a text model and image, video and music endpoints of its own.
- A translator can run on a local model while an image tool uses a cloud one.

**Built in:**

| App | Category | What it does |
|---|---|---|
| Multi Purpose AI Translator | Content | Translates text, HTML, images and PDFs into 15 languages in a style you choose, with optional translator comments; saves as Rich Text |
| Readability & Tone Analyzer | Content | Scores a page or text for readability, finds its tone and audience, and suggests improvements |
| AI Meta Tag Generator | SEO | Three optimized meta titles and descriptions for a page, from its URL or pasted HTML |
| llms.txt Generator | SEO | Crawls your site, lets AI pick the pages worth reading, and bundles `llms.txt` and `llms-full.txt` |
| AI Plugin Creator | Code | Generates installable WordPress plugins, Joomla plugins and modules, and Drupal modules, or modifies one you upload as a ZIP |
| SP Page Builder Addon Creator | Code | Makes and edits SP Page Builder 6 addons as installable Joomla 5/6 plugins from a description and screenshots, in a folder you choose: a browser of your addons (edit, clone, delete, import a ZIP), a file tree, a code editor, and **Download installable zip** |
| Elementor Widget Creator | Code | The same for Elementor widgets as installable WordPress plugins (classic widgets for Elementor 3 and 4) |
| AI HTML Table from Image | Code | Turns a screenshot or photo of a table into clean HTML table code |
| AI SVG Creator | Design | Creates and refines SVG graphics and icons from a description, a reference image or an SVG, optionally animated |

**Make your own.** **New app** opens a builder in the style of Google AI Studio's build mode:
- **Chat and preview.** You describe the app in a chat on the left, and a model writes it while **Preview** reloads live on the right.
- **Code.** **Code** shows every file in a highlighted editor, so you can change anything by hand.
- **Self-checks.** The model can check the preview for errors and take screenshots of it to fix its own mistakes. **Fix with AI** sends an error back to it.
- **Checkpoints.** Every model turn and every save is a checkpoint you can restore.
- **Drafts.** An app stays a draft until **Save to Utility Apps**.

**Share.**
- Clone any app (built-in ones too) to change it.
- Export an app as a single `.oxideapp` file and import it on another computer. Its capabilities are shown before it installs.

## Local models, first-class

Oxide treats a model on your own computer as an equal to a cloud model, not a fallback:

- **LM Studio and Strata are built in**, and any other OpenAI-compatible server (Ollama, LocalAI, vLLM, …) can be added with its URL.
- **Tool calling is detected per model.** LM Studio reports it, and Strata is probed once. Every listed model can use tools, MCP connectors and agentic work.
- **Thinking controls** work where the server supports them. Strata, for example, gets an on/off switch.
- **Small context windows are handled:**
  - tool results are sized to the model's window;
  - old tool results are cleared, then long chats are compacted;
  - a "context full" answer triggers one automatic compaction and a retry.
- **Use local models where they fit:**
  - Auto mode's safety check (recommended, to keep costs down);
  - chat titles, memory and compaction;
  - a Utility App's own model;
  - image, video and music generation on a local OpenAI-compatible server.
- **No account needed.** Oxide stores everything in SQLite on your computer, and API keys in the system keychain. With only local models, nothing leaves your machine.

## Features

**Chat**
- Streaming replies, stop, retry, edit and resend with branches (‹ 1/3 ›), copy, star, rename, search, export to Markdown, JSON or HTML.
- Extended thinking with an effort control, where the model supports it.
- Attachments: images, PDFs, text and code files, by drag and drop or paste.
- Markdown with tables, maths (KaTeX) and highlighted code.
- Voice dictation through an OpenAI-compatible speech-to-text endpoint.
- Context management: a context meter, and automatic compaction of long chats and long agentic replies.

**Projects and memory**
- Projects with their own instructions, knowledge files and chats. Large knowledge is searched (SQLite FTS5) instead of sent in full.
- Optional memory across chats, which you can view and edit.

**Artifacts**
- A side panel with versions for Markdown, code, HTML, SVG, Mermaid and React.
- Artifacts run in a sandboxed frame that has no access to the app.

**Tools and connectors**
- MCP client for local (stdio) and remote (Streamable HTTP, OAuth) servers. It speaks both the 2026 stateless protocol and the older handshake.
- `oxide_config.json` uses Claude desktop's `mcpServers` format, so entries can be pasted in as they are, or imported from Claude desktop in one click.
- Desktop extensions (`.mcpb`) and Skills (folders with a `SKILL.md`). Models can create and edit skills, with your approval.
- Built-in web search (Brave, Tavily, SearXNG, or Anthropic's own) and web fetch.

**Agentic work**
- Folders you share with a chat, with file tools confined to them.
- A task list, and a session panel showing progress and what the chat used.
- Managed Python (`uv`, installed on first use), shell commands (off by default), and a Docker sandbox per chat.
- A browser agent (Browser-Use) and a Chrome extension that lets Oxide act in your own browser.
- Computer use: screenshots, mouse and keyboard, with a visible stop bar and Ctrl+Alt+Esc. Windows only for now.
- Scheduled tasks that run while Oxide sits in the tray.

**Permission modes**
- **Manual:** Oxide asks before tools that change things.
- **Auto:** Oxide works on its own and stops to ask when an action looks risky. A two-stage safety check decides; it sees your messages and the tool calls, never the model's own text or tool results. It fails closed.

**Compact tool history**
- A reply's tool calls fold into one quiet summary line, which opens to compact rows.
- While a reply runs, a single status line shows the current step and the elapsed time.

**Media generation**
- Images, video and music from chats, through OpenAI-compatible endpoints, with a default model for each kind.

**Utility Apps:** see [above](#utility-apps).

**Desktop**
- Tray icon, a global quick-entry hotkey (Alt+Space), `oxide://` links, start at login, notifications.
- Light and dark themes. English and Danish.
- Updates install from within the app.

## Install

Download the installer for your system from [Releases](https://github.com/sitefactorydk-collab/oxide/releases/latest):

| System | File |
|---|---|
| Windows 10/11 (x64) | `Oxide_<version>_x64-setup.exe` (per user) or `.msi` (per machine) |
| macOS 11+ (Apple Silicon and Intel) | `Oxide_<version>_universal.dmg` |
| Linux x64 (Ubuntu 24.04 or newer) | `.AppImage`, `.deb` or `.rpm` |

The installers aren't code-signed:
- **Windows:** SmartScreen warns. Choose **More info → Run anyway**.
- **macOS:** right-click the app → **Open** the first time.

Then add a provider and its API key in **Settings → Providers**. Local servers such as LM Studio need no key.

The Chrome extension comes with every installer; **Settings → Chrome** shows its folder and how to load it. It is also attached to each release as a zip.

## Build from source

**You need:**
- Rust stable (pinned in `rust-toolchain.toml`)
- Node.js 22
- The Tauri CLI: `cargo install tauri-cli --version "^2"`
- **Windows:** Visual Studio Build Tools ("Desktop development with C++") and WebView2 (included with Windows 11)
- **macOS:** Xcode Command Line Tools
- **Linux:** `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`

**Then:**

```sh
npm install
npm run fetch-uv       # the uv sidecar for your platform
npm run extension      # builds the Chrome extension
cargo tauri dev        # run with hot reload
cargo tauri build      # installers for your platform
```

**Checks** (also run by CI on all three systems):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run typecheck
npm run lint
npm test
```

## Repository layout

| Path | What's there |
|---|---|
| `src-tauri/` | The Tauri app: windows, tray, commands, events |
| `crates/oxide-core` | Domain types |
| `crates/oxide-store` | SQLite storage and migrations |
| `crates/oxide-llm` | Provider clients (Anthropic, OpenAI-compatible), streaming, media generation |
| `crates/oxide-mcp` | MCP client, server manager, `.mcpb` |
| `crates/oxide-agent` | Replies, the agent loop, tools, approvals, permission modes, Utility Apps |
| `crates/oxide-bridge` | WebSocket bridge to the Chrome extension |
| `crates/oxide-pyenv` | Managed Python (`uv`) |
| `crates/oxide-sandbox` | Docker sandbox |
| `crates/oxide-computer` | Screen capture and input |
| `src/` | React + TypeScript front end (`src/bindings` is generated by tauri-specta) |
| `python/` | The browser agent (Browser-Use as an MCP server) |
| `extension/` | Chrome extension (MV3) |
| `utility-apps/` | The built-in Utility Apps |

## More

- [`docs/RELEASING.md`](docs/RELEASING.md): installers, signing, updates and the release workflow.
- [`docs/SECURITY.md`](docs/SECURITY.md): the threat model and how each boundary is protected.
- [`PLAN.md`](PLAN.md) and [`PROGRESS.md`](PROGRESS.md): the build plan, phase by phase, and its log.
- [`CLAUDE.md`](CLAUDE.md): conventions for contributors and coding agents.

## License

Licensed under either of MIT or Apache-2.0, at your option (as declared in `Cargo.toml`).
