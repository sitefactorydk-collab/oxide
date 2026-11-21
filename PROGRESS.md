# Oxide — Progress Log

The current phase and hand-off notes live here. Every phase chat reads this file first and appends to it last (template in PLAN.md §4).

## Status
- **Phase 13** — Browser agent (Browser-Use): done (`1af53b2`, fix `86f147d`; all exit checks passed by hand on 2026-11-12). Follow-up (Google search instead of guessed addresses) committed as `09c1ca2` (2026-11-12 13:53, all checks and the real-browser test green; the hidden test browser's real Google search was refused with a robot check and reported plainly, as designed); then click fixes `23e3fd5` and Google's AI Overview + longer extension-folder retries `4412e21` (2026-11-12 15:16, all green)
- **Phase 13b** — Context management & chat compaction: done (`d58672b`, fixes `fccd2c7`, `2c31e21`; exit checks passed by hand on 2026-11-12)
- **Phase 14** — Chrome extension & WebSocket bridge: committed `176a672` (2026-11-12 22:10, every check green); the real-extension test passed on Windows in Edge 157 (22:22); all manual exit checks passed by hand (22:57). Follow-up `67b59b6`: no permission prompts once paired (checked by the user after reloading the extension); web-research testing fixes `aa5a4f2` (Google results readable) and `051884a` (task list closes at a reply's end), both checked by the user on 2026-11-13
- **Phase 14c** — Models create and edit skills: done (`b9d8faf`, 2026-11-13 08:42, every check green on Windows; all exit checks passed by hand at 08:54)
- **Phase 15** — Scheduled tasks: done (`68ee955`, follow-up `d7c34cd`; every check green on Windows; all exit checks passed by hand on 2026-11-13 11:13)
- **Phase 16** — Computer use: closed by the user on 2026-11-13 (`898553d`, follow-ups `ef0098f`, `9046b25`, `6afc299`, `7af87f7`; every check green on Windows). The exit check was **not fully passed**: with the local Strata `swift-iq2_xs` the model opened Notepad and typed the text but never managed the Save dialog (its clicking isn't precise enough); the check with Claude and the Ctrl+Alt+Esc check by hand weren't done (Stop from the app worked). See Phase 16's notes.
- **Phase 17** — Packaging, updates & CI: done for Windows (2026-11-14): `82d6fe3`, CI fixes `ff775fc`, Groq/0.1.1 `3f61eed`. CI is green on Windows, macOS and Ubuntu. Releases v0.1.0 and v0.1.1 were published from the public repo `sitefactorydk-collab/oxide`. The macOS and Linux installer checks were deferred by the user.
- **Phase 18** — Hardening & polish (+ usage dashboard, HTML export, dictation): done (2026-11-19, `ff92037`). Every check is green on Windows and every manual check passed. Then the Vitest 5 maintenance step (`a4615c4`, pushed; ESLint 10 waits for plugin support).
- **Phase 19** — Generation endpoints & media generation: **done** (2026-11-20 00:06). Commits `dcd3bdb`, `03b06bc` and `727d13e`, pushed. Every check is green on Windows. The user passed every exit check on their local AI server: image, video (synchronous `/videos/generations`), music (comfy-bridge `ace-step`), and changing a default.
- **Phase 19 follow-up (2026-11-19 evening):** at the user's request, the page is renamed **Settings → Media generation** and the Text group is dropped (text comes from Providers and Models). Script: `scripts\win\phase19-media.cmd`, message `logs\commit-msg-phase19-media.txt`.
- **Phase 19 follow-up 2 (2026-11-19 late evening):** at the user's request, Audio (speech) became **Music generation** on `/music/generations` (the user's local comfy-bridge server). Image was checked in Oxide by the user.
- **Phase 19 follow-up 3 (2026-11-19 late evening):** video failed on the user's server, which has only the synchronous `POST /videos/generations`. `generate_video` now falls back to it when `/videos` is missing, and images, music and that video wait up to 30 min. Follow-ups 2 and 3 were committed together as `727d13e` (2026-11-19 23:35, every check green on Windows on the second run of `scripts\win\phase19-music.cmd`; pushed 2026-11-20).
- **Phase 20** — Utility Apps: runtime and gallery: **done** (2026-11-20). Committed `1915243` after four Windows runs (every check green on the fourth); the user passed every manual exit check at 11:14. A few non-breaking visual bugs were noticed; the dialog tooltip one was fixed in `0839fa8` (with extension-folder cleanup).
- **Phase 21** — Utility App builder: **done** (2026-11-20). Committed `1ce2afe` (every check green on Windows on the third run), template fix `c606ab4`; the user passed every manual exit check (18:35), building the CSV chart app with LM Studio's `google/gemma-4-12b-qat`.
- **Phase 22** — More Utility Apps (Translator, Plugin Creator, llms.txt Generator): **built, waiting for the Windows run** (2026-11-20). `scripts\win\phase22.cmd` runs every check, commits with `logs\commit-msg-phase22.txt` only when all pass, then starts Oxide for the manual exit check.
- **Next phase:** none planned after Phase 22 (PLAN.md lists a community catalogue for sharing apps as “later”).
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
- **Phase 17 harness note:** the cloud container's mirror was rebuilt from `logs\p17-mirror.tgz` (repo tarred without node_modules/target/.git/logs/dist) plus `logs\linux-vendor.tgz`; the harness is as in Phase 12, with members core, store, llm, agent, mcp, pyenv, sandbox, bridge and the `specta-macros` stub at `2.0.0-rc.18`. `src-tauri/src/acl.rs` can be tested there in a scratch crate that `#[path]`-includes it (only `serde_json` needed).
- **Linux-side Rust builds, Phase 3 setup** (the cloud container has Rust 1.97; no crates.io):
  - Mirror the repo: on the device, `tar` it into `logs\` (exclude `node_modules`, `target`, `.git`, `logs`), stage the tarball and unpack it in the container. Stage `logs\linux-vendor.tgz` too and unpack it to `~/vendor-oxide`.
  - Harness `~/oxide-linux`: a symlink `crates -> <mirror>/crates`; `.cargo/config.toml` replacing crates-io with `~/vendor-oxide/vendor` (offline); a workspace `Cargo.toml` made from the repo's with `members` = the four library crates (core, store, llm, agent) plus `[patch.crates-io] specta-macros = { path = "stubs/specta-macros" }`. The stub is a no-op `#[proc_macro_derive(Type, attributes(specta, serde))]` plus a pass-through `#[specta]` attribute.
  - Builds use `CARGO_TARGET_DIR=$HOME/oxide-target-linux`. clippy and rustfmt work there.
  - New crates must already be in the vendor bundle. `futures-util` stays `default-features = false` because `futures-macro` isn't vendored.
  - Frontend files: copy them to the device and run tsc/eslint/prettier there (see above), then copy the formatted files back to the mirror.
- **Syncing many files into the linked folder** (added in Phase 8): without delete permission, `tar x` fails on existing files (it unlinks first); `tar xzf <file> --overwrite` writes them in place.
- **Linux harness and node_modules** (Phase 12): the cloud container's mirror is `~/oxide-mirror` (not `/home/claude`); the harness `~/oxide-linux` now has `oxide-pyenv` as a member and the `specta-macros` stub must report version `2.0.0-rc.18`. The cloud container has no network for downloads, so uv and Python can't run there: anything that needs the real uv is an ignored test (`crates/oxide-pyenv/tests/real_uv.rs`) that `phase12.ps1` runs on Windows. To sync a phase to the device: tar the files newer than the phase start, `device_commit_files` the tarball into `logs\`, then `tar xzf … --overwrite` in the device VM; tsc/eslint/prettier run there against the Windows `node_modules` (about 30 s for tsc).
- **Chrome extension in the cloud container** (Phase 14): `crates.io` and npm are unreachable, but TypeScript and Prettier work when staged from the device (`node_modules/typescript`, `node_modules/prettier`); `node scripts/build-extension.mjs` builds `extension/dist`, and Chrome for Testing (`/usr/bin/google-chrome`) loads it unpacked for `crates/oxide-bridge/tests/real_chrome.rs`.
- **Docker in the cloud container** (Phase 12b): `dockerd` runs there (start it with `nohup dockerd &`), but Docker Hub isn't reachable. Make a stand-in command image with `tar -c --one-file-system … / | docker import - oxide-test:local` and build a stand-in file server image from it; then run the ignored real-Docker tests with `OXIDE_SANDBOX_IMAGE` / `OXIDE_SANDBOX_FILES_IMAGE`. The harness `~/oxide-linux` has `oxide-sandbox` as a member.
- **Browser agent in the cloud container** (Phase 13): Python 3.13 there has `mcp` 2.3.0 and Playwright 1.63, and `/usr/bin/google-chrome` exists (no Browser-Use, no PyPI). See Phase 13's notes for the commands.
- **Linux harness after Phase 18** (Phase 19): `logs\linux-vendor.tgz` has no `dbus-secret-service`, so in `~/oxide-linux` `crates/oxide-store` is a folder with symlinked `src` and `migrations` and a `Cargo.toml` whose `secret-service` feature is `[]`. Start without the repo's `Cargo.lock` (cargo resolves offline from the vendor folder). Mirror tarball: exclude `src-tauri/binaries`, `docs/ui-parity`, `docs/*.json` and `extension/dist` too (they are large). `src-tauri` files are formatted with `rustfmt --edition 2024 --config-path rustfmt.toml <files>`.
- **Deleting files:** `rm` in the linked folder needs delete permission (`device_request_delete_permission`). Never run git from the Linux VM; use `commit.ps1` on Windows.

## Phase 22 — More Utility Apps (rest of AI-Toolkit) — 2026-11-20
Status: **built, waiting for the Windows run** (`scripts\win\phase22.cmd`: every check, then a commit with `logs\commit-msg-phase22.txt` only when all pass, then the app for the manual check).

Before starting: Phase 21's exit checks were passed (18:35) and its last commits (`0388cbb`, `5cea061`) are in, so the phase went ahead. The originals were read from `github.com/sitefactorydk-collab/ai-toolkit` (`src/components/Translator.tsx`, `AIPluginCreator.tsx`, `LLMSTxtGenerator.tsx`, `server.ts`'s `/api/crawl` and `/api/fetch-content`, `lib/gemini.ts`'s `categorizeUrls`).

Exit criteria (Windows, by hand):
- [ ] **Multi Purpose AI Translator** (Content): paste text → pick a language and style → **Translate** streams the translation (asks for *AI text* once); **Include translator comments** adds the 💡 section in the chosen language; **Copy**. **Document / Image**: a PDF and a picture with text (vision model) → the layout-style translation → **Save as Rich Text file** opens in Word. The ⚙ next to Style: add, edit and delete styles; they are still there after reopening the app.
- [ ] **AI Plugin Creator** (Code): WordPress, a name and instructions → **Generate Extension** → the file tree and code → **Save to ZIP file** (`<name>-v1.0.0.zip`) installs in WordPress (or at least has a valid plugin header). Joomla shows Plugin/Module. Drop that ZIP back in with a change request → **Modify Extension** → the saved ZIP keeps the files the model didn't change.
- [ ] **llms.txt Generator** (SEO): a site URL (several lines for several sites) → **Generate Files** (asks for *AI text* and *reading web pages* once each) → crawling, triage, the high/low value lists, the `llms.txt` / `llms-full.txt` preview → **Download .zip Bundle**. **Markdown Ready?** adds the Guidance section.

Done:
- `utility-apps/multi-purpose-ai-translator/`, `utility-apps/ai-plugin-creator/`, `utility-apps/llms-txt-generator/` (manifest + `App.tsx` each), listed in `crates/oxide-agent/src/apps/builtin.rs` (7 built-in apps now) and in `apps/tests.rs`' listing test.
- **Translator** (`text`): the original's 15 languages, four default styles plus own ones (⚙: add, edit, delete; kept in the app's `storage` under `instructions`), the original's prompts word for word (text mode with or without translator comments in a chosen language; the layout format for images and PDFs), `ai.text` with streaming and **Stop**, images to `images` and PDFs to `documents` (base64), Markdown output with single line breaks kept, **Copy**, **Save as Rich Text file** (the original's RTF writer, now also for characters outside the BMP), the answering model under the output.
- **Plugin Creator** (`text`): WordPress / Joomla (Plugin or Module) / Drupal, name, version, author, description, instructions; an optional ZIP to modify (picker or drop; its text files go into the prompt, binaries are kept aside); `ai.json` with the original's schema; paths cleaned (no `..`, no leading `/`); files the model didn't return are kept; file tree with folders, code view with **Copy**; **Save to ZIP file** merges the uploaded ZIP's untouched and binary files (JSZip, saved with `files.save`).
- **llms.txt Generator** (`text`, `webFetch`): one or more site URLs; each site crawled with `web.crawl` (robots.txt followed, same origin, depth 5, **Pages per site** 10/25/50); media URLs filtered as in the original; the original's triage prompt and schema through `ai.json` (if the answer can't be read, every page counts as high value, as in the original); the model's grouping is kept only for crawled pages (real titles; invented URLs dropped; pages it left out become low value); `llms.txt` exactly as the original built it (`# host`, optional Guidance, `## High Value Pages (<lang>)`, `## Low Value Pages`) and `llms-full.txt` from the crawled Markdown of the high-value pages; a preview of both with **Copy**; **Download .zip Bundle** (`llms-pack.zip` via `files.zip` + `files.save`); **Stop**.
- **Tests** (`src/features/apps/BuiltinApps.test.tsx`, fake SDK now with `web.crawl`, `files.zip/readText/toBase64`, `storage` and the `jszip` module): Translator text (language, style, prompt, streaming, line breaks, model, Copy) and PDF with a new saved style (storage, `documents`, layout prompt, RTF); Plugin Creator new (prompt, schema, tree, `..` cleaned, ZIP contents) and modify (uploaded ZIP in the prompt without binaries, the model's change and the untouched binary in the saved ZIP); llms.txt (invalid URL, crawl options, triage prompt without media, llms.txt/llms-full.txt contents, skipped count, preview, download). Rust: `built_in_apps_are_valid` (7 apps, imports only what the frame provides) and the listing test.

Deviations from the originals (and why):
- **No token counts in the Translator**: the SDK's `ai.text` reports the model, not usage; the model is shown instead.
- **Text files in the Translator's Document tab** (TXT, HTML, Markdown, XML, JSON, CSV) are read in the app and translated like pasted text, with comments; the original sent them to Gemini as inline data with the image-layout format, which doesn't fit text.
- **Styles live in the app's storage** (kept by Oxide) instead of the browser's localStorage, which the app frame doesn't keep.
- **No `temperature: 0.2`** in the Plugin Creator: the SDK has no temperature option.
- **llms.txt crawling is Oxide's `web.crawl`**: at most 50 pages per site (the original 100), the same origin rather than the same host name, robots.txt read for the agent `oxide` (the original's `SignalPathBot`), and Oxide's own main-content Markdown instead of a second fetch with cheerio + Turndown. No sitefactory.dk fetch proxy is used.
- Added: Stop buttons (Translator, llms.txt), Copy per file (Plugin Creator), the llms.txt preview and Pages per site.
- Not ported (as decided with the user): AI Web Auditor, AI SEO Recommendations, AI Schema Markup Generator, AI FAQ Questions Generator, Markdown for AI Agents testing tool, AI SFT Dataset Generator, SP Page Builder - AI.

Known issues / TODO carried forward:
- Big plugins depend on the model returning all files in one JSON answer; small local models may cut it off (the SDK retries once on unreadable JSON).
- A 50-page crawl sends all page titles to the model in one request; very long titles on big sites use much of a small model's context.
- `llms-full.txt` holds what Oxide's web fetcher reads (pages capped by the fetcher's size limit).

Verified (agent side): tsc ✅ (app, utility-apps) eslint ✅ prettier ✅ on the device. Windows (`phase22.cmd`): pending. Manual exit checks: pending.

**Windows run 1 (`phase22.cmd`, 2026-11-21 15:26–15:29):** fmt, Prettier, clippy, typecheck, lint and Vitest (all the new app tests included) passed; `cargo test` failed only on `apps::tests::a_clone_is_an_editable_own_app`, which still counted 4 built-in apps (6 with two clones). It now counts from `builtin::all()`. Not committed; run `phase22.cmd` again.

## Phase 21 — Utility App builder (build mode) — 2026-11-20
Status: **done** (2026-11-20). Committed `1ce2afe` (17:09, every check green on Windows) and `c606ab4` (template); the user passed every manual exit check (18:35).

**Windows runs:** 1 (`phase21.cmd`, 16:30–16:45): bindings, SDK types, fmt, clippy, cargo test, typecheck, lint and vite build passed; Vitest failed 4 tests, all test mistakes: the builder's “Back” clashed with the title bar's Back (renamed **Close the builder**), `appIcons.test.ts` resolved its path from `import.meta.url` (not a file URL under jsdom; now `process.cwd()`), and `sdkTypes.test.ts` asked for the node environment, which `src/test/setup.ts` doesn't support (now jsdom). 2 (`phase21-retry.cmd`, which skips the prep): one test looked for the app page's toolbar before it loaded (now `findByRole`). 3 (17:00–17:09): all green, committed.

Before starting: Phase 20's exit checks were passed (11:14) and the user's fix commit `0839fa8` (dialog tooltips, extension folders) was in. No Phase 20 visual bugs were listed beyond that one, so the phase went ahead.

Exit criteria (Windows, by hand):
- [x] Utility Apps → **New app** opens the builder on a draft (“New app”, from the template). In its chat: “build an app that turns a pasted CSV into a chart and lets me download it as PNG”. The model writes the files (the Preview reloads as it works), checks the Preview (`app_preview_status`), fixes what it reports, and the app works in Preview (paste a CSV → chart → **Download PNG**).
- [x] **Code**: change something by hand (e.g. a heading in `App.tsx`), **Save** (or Ctrl+S) → the Preview shows it; **Checkpoints** lists “Saved App.tsx” and the model's turns; restoring an older one goes back (and the current one is kept).
- [x] **Save to Utility Apps** → the app is in the gallery under My apps (name, icon and category from the model or **App details**).
- [x] A built-in app: **Clone** → **Edit** opens the builder → change it (by chat or in Code) → **Save to Utility Apps** → it runs changed from the gallery.
- [x] Both apps **Export** (from the app page, or the builder's ⋯ → Export saved app) and import again.
- [x] Optional: an error in the Preview shows **Fix with AI**, which asks the builder chat to fix it.

Done:
- **core** `app_builder.rs`: the builder tool names (`app_list_files`, `app_read_file`, `app_write_file`, `app_edit_file`, `app_delete_file`, `app_rename_file`, `app_update_manifest`, `app_preview_status`, `app_screenshot`), `AppDraft` (the draft's app and files, published, unsaved, the builder chat), `AppCheckpoint` + `CheckpointKind` (start, model, manual, restore), `PreviewRequestKind` (status, screenshot), `PreviewReport` + `PreviewConsoleLine`, `checkpoint_label` (“After: <the request's first line>”), `MAX_APP_CHECKPOINTS` (60 per app).
- **store**: migration `0012_app_builder.sql`: `app_builder_chats` (one chat per app, deleted with it) and `app_checkpoints` (every file of the draft as JSON with base64 data, a digest that skips identical versions, the newest 60 kept). The chat list and search leave builder chats out (`list_conversations`, `search`); `get_conversation` still finds them. `create_builder_chat`, `builder_chat`, `builder_app_of`, `last_user_text`, `add_app_checkpoint`, `app_checkpoints`, `app_checkpoint_files`, `delete_builder_data`.
- **agent** `apps/builder.rs`:
  - **Drafts** in `<data>/app-drafts/<id>/` (on `UtilityApps`): made from the template (`utility-apps/_template/`, compiled in) for a new app, or copied from the saved app the first time an own app is opened; built-in apps must be cloned first. `manifest_at(id, draft)`; `list_drafts` (drafts never saved); ids are unique across apps and drafts.
  - `AppBuilder`: `open` (draft + chat + a Start checkpoint), `state` (unsaved = the draft differs from the saved app), `save_file`/`delete_file`/`rename_file`/`update_manifest` from the window (each makes a checkpoint), `checkpoint_turn` (after each builder reply, if the draft changed), `checkpoints`, `restore` (keeps the current files as “Before restoring” first, unless the newest checkpoint already has them), `publish` (**Save to Utility Apps**: the draft replaces the saved app's folder), `discard` (a never-saved app goes completely; a saved app keeps its chat and checkpoints).
  - The model's tools, confined to the draft: paths through `valid_app_path` (no `..`, absolute paths, backslashes or drive letters), the 2 MB / 20 MB / 200-file limits, the manifest stays valid with the same id, the manifest and the entry file can't be deleted, renaming the entry updates the manifest, edits must match once (or `replace_all`; CRLF files are matched too). `app_update_manifest` checks capabilities and icons. Every change tells the window (`BuilderSink::draft_changed`).
  - `app_preview_status` / `app_screenshot` ask the open Preview through `BuilderSink::preview_request` and wait up to 45 s for `answer`; without a Preview the model is told so. The report: rendered or not, errors, the newest console lines, a PNG.
  - `prompt`: how apps work (React 18 + TS, the importable packages, no network, Tailwind with `dark:`, capabilities, `OxideError`), how to work (read before editing, check the Preview and fix errors, name and icon the app), the manifest and file list now, the gallery's icon names (`APP_ICON_NAMES`, checked against the frontend by a Vitest test), and the SDK's types (`SDK_TYPES` = `src/appframe/oxide-sdk.d.ts`).
  - `ChatService`: a chat in `app_builder_chats` gets the builder tools, the task list and web search/fetch only (no MCP, files, shell, Python, sandbox, browser, Chrome, computer, artifacts or skills), the builder prompt after the usual one, no approvals for builder tools (drafts only; checkpoints undo), a checkpoint at the end of each reply, and no auto-title or memory.
  - The Preview runs the draft as `draft:<id>`: `AppRuntime::call` and `UtilityApps::grant` read the draft's manifest and use the app's settings and grants (so the draft can declare a new capability and ask for it). Deleting an app removes its draft, chat and checkpoints. Export all includes `app-drafts/`.
- **src-tauri**: `commands/builder.rs` (`open_app_builder`, `app_draft`, `list_app_drafts`, `save_draft_file`, `delete_draft_file`, `rename_draft_file`, `update_draft_details`, `app_checkpoints`, `restore_app_checkpoint`, `publish_app_draft`, `discard_app_draft`, `answer_preview_request`), events `AppDraftChanged` and `AppPreviewRequest` (`TauriBuilderSink`), `AppState.builder`, capabilities.
- **frontend**:
  - `features/builder/`: `BuilderPage` (`/apps/:id/build`): the header (back, icon and name → **App details**, status Draft / Unsaved changes / Saved, **Checkpoints** menu, **Save to Utility Apps**, ⋯ with details, Export saved app, Discard unsaved changes / Delete draft); the builder chat on the left (`ChatView` embedded, with its own model picker); **Preview | Code** on the right. `PreviewPane` (the draft in `AppFrame` as `draft:<id>`, reloads 300 ms after the files change, answers `appPreviewRequest`: reload, wait for render or error (≤ 10 s) plus 1.2 s, then errors, console and an optional screenshot; **Fix with AI** on errors). `CodePane` (file tree, new / rename / delete, unsaved dots, Save and Ctrl+S, a notice when the model changed a file being edited). `CodeEditor` (a textarea over a Shiki-highlighted copy, line numbers, Tab / Shift+Tab, Ctrl+F find with match marks). `DetailsDialog` (name, description, category, icon picker or emoji). `queries.ts`.
  - The frame: console lines go to the main window; `snapshot.ts` draws the page into a PNG (DOM → SVG `foreignObject` → canvas) for `app_screenshot`. `AppFrame` takes `appRef`, `events`, `control` and `onFixError`.
  - Gallery: **New app**, a **Drafts** section (never-saved apps open in the builder). App page: **Edit** opens the builder (the Phase 20 `CodeDialog` is removed). Breadcrumb “Utility Apps / Builder”. `ChatView` is exported with `conversation`, `embedded`, `onModelChange` and `placeholder`. `AppIcon` knows 58 icons (`appIcons.ts`).
  - `scripts/sdk-dts.mjs` (`npm run sdk-types`): writes `src/appframe/oxide-sdk.d.ts` from `sdk.ts` with the TypeScript compiler (the frame's own wiring left out), formatted by Prettier; `sdkTypes.test.ts` fails when it is stale.
- **Tests**: store 2 (builder chats hidden from the list and search; checkpoints skip repeats, keep 60, are per app); core 2; agent `apps/builder_tests.rs` 9 (a new app is a draft with a chat and a Start checkpoint; tools stay in the draft; write/edit/rename/delete/list/manifest; checkpoints after turns and saves and restore; preview errors, console and screenshots reach the model, and a closed Preview is reported; Save to Utility Apps and later changes; built-in apps are cloned first; the Preview's draft manifest with the app's grants; a whole builder chat through `ChatService` with a fake model). Vitest: `features/builder/Builder.test.tsx` (new app → draft and hidden chat → Drafts; Code save → checkpoint → Preview loads the new file; the model's changes and restoring; the preview check answers with errors and console, Fix with AI; details, Save to Utility Apps, Edit; an own app saved and exported; tree and find helpers), `Apps.test.tsx` (clone → Edit in the builder → Save → Export), `appIcons.test.ts`, `sdkTypes.test.ts`. Fake backend: builder commands and events in `src/test/fakeApps.ts`, hidden builder chats in `fakeBackend.ts`.

Deviations from PLAN.md (and why):
- **The builder chat is an ordinary chat** that the chat list and search leave out (`app_builder_chats`), so streaming, approvals, stop, retry, branches and compaction work as in any chat.
- **Drafts are folders** (`<data>/app-drafts/<id>/`) and the Preview calls the SDK as `draft:<id>` with the app's own settings and grants; the gallery runs the saved app until Save to Utility Apps.
- **Screenshots are drawn by the frame** (DOM → SVG → PNG), not taken from the screen: the frame is sandboxed and Oxide's window may be hidden. Web fonts don't come along (a fallback font shows).
- **The code editor is Oxide's own** (a textarea over Shiki's highlighting) rather than a new editor dependency such as CodeMirror: it reuses the bundled highlighter and needs nothing new from npm.
- **Builder tools don't ask**: they change only the draft, and every turn is a checkpoint.
- **Clone stays as in Phase 20** (a copy in My apps); **Edit** then opens the builder. Export works on the saved app.

**After the commit (manual check, 17:21):** the user typed the CSV request into the *template app's* prompt box in the Preview and pressed its **Ask** button, so the template sent it to the model as a plain question (an HTML answer came back) instead of going to the builder chat. The template is now a static “Your new app will appear here — describe it in the builder's chat on the left” screen with no prompt box and no capabilities. Drafts made before this keep the old template until the builder rewrites `App.tsx`.

**After the exit checks (18:52):** in the dark theme the header's **Save to Utility Apps** showed as an empty light pill when there was nothing to save: it combined `headerButton`'s `text-fg` with `text-app` on `bg-fg`, and `text-fg` won (`cn` doesn't merge Tailwind classes). It now has its own `primaryButton` style, and when the draft is saved the button gives way to a quiet **✓ Saved**.

Known issues / TODO carried forward:
- Checkpoints keep whole copies of the draft (base64 in the database, 60 per app); fine for text apps, heavy for apps with large files.
- The screenshot's fonts and anything from linked stylesheets are missing; canvases with cross-origin pixels stay blank.
- The builder's model sees the Preview only while the builder is open (otherwise it is told so).
- Unsaved edits in Code are lost when the builder is left (no prompt yet).

Notes for Phase 22:
- Port the remaining AI-Toolkit apps with the builder or by hand into `utility-apps/<id>/` (list them in `builtin.rs` and `BuiltinApps.test.tsx`).
- After changing `src/appframe/sdk.ts`, run `npm run sdk-types` (the Vitest test checks it).

**Exit check (18:35, by the user, all OK):** with `google/gemma-4-12b-qat` (LM Studio) the builder made a working CSV → chart app with PNG download. The model struggled at first: it believed PNG export needed `html2canvas`, offered screenshots instead, then planned a canvas export in a long loop until Oxide cut the repetition, and finally built SVG charts plus a canvas-drawn PNG through `files.save`. The builder prompt now says the browser's own APIs work without packages (`<canvas>` `toBlob`/`toDataURL`, SVG → canvas through `Image` with `data:`/`blob:` URLs, `URL.createObjectURL`, `FileReader`), how to save a chart as PNG, and not to give up before trying them (agent test re-run on Linux).

Verified: fmt ✅ clippy ✅ cargo test ✅ typecheck ✅ lint ✅ Vitest ✅ vite build ✅ (Windows, `phase21-retry.cmd`, third run); committed `1ce2afe`. Manual exit checks ✅ (by the user).

## Phase 20 — Utility Apps: runtime and gallery — 2026-11-20
Status: **done** (2026-11-20). Committed `1915243` by `scripts\win\phase20.cmd` (fourth run, 10:37–10:44, every check green). The user passed every manual exit check (11:14).

Exit criteria (Windows, by hand):
- [x] Sidebar → **Utility Apps** lists the four built-in apps (search and categories work).
- [x] Each built-in app runs with the global models (text from Providers and Models):
  - **AI Meta Tag Generator**: a URL (asks for *AI text* and *reading web pages* once each) → three suggestions; also with pasted HTML.
  - **Readability & Tone Analyzer**: a URL or pasted text → score, tone, strengths, improvements, analysis.
  - **AI HTML Table from Image**: a picture of a table (needs a vision model) → table preview, Code, Copy, Download.
  - **AI SVG Creator**: a description → SVG; a second request refines it; Download.
- [x] With a per-app override (an app's **Settings** → Text model) the app uses that model (check the endpoint's log or the Meta Tag app's model label).
- [x] Each capability asks once per app; **Revoke** in Settings makes it ask again; Deny makes the call fail with a message in the app.
- [x] **Clone** a built-in app → it appears under “My apps” with **Edit**; change its `App.tsx` in Edit → Save → it reloads changed; **Export** saves an `.oxideapp`; delete the clone, **Import app** that file → the preview lists what it may ask for → Install → it runs.

Done:
- **core** `utility_app.rs`: `AppManifest` (id, name, description, icon, category, version, entry, capabilities, default models, author) with validation; `AppCapability` (text, image, video, music, webSearch, webFetch, connectors, python, sandbox, browser, chrome); `UtilityApp`, `AppBundle`, `AppFile`; `AppPrefs` (text model, `AppGenerationOverride` per kind, granted capabilities) and `AppPrefsView`; `AppImportPreview`; `AppCallError`/`AppErrorCode`; `app_secret_name` = `app:<id>:generation:<kind>`; limits (200 files, 2 MB a file, 20 MB an app, 10 MB storage).
- **store**: migration `0011_utility_apps.sql` (`app_prefs`, `app_storage`), with prefs, storage get/set/delete/keys/clear/bytes (capped) and `delete_app_data`.
- **agent** `apps/`:
  - `mod.rs` `UtilityApps`: built-in apps (read-only) plus own apps in `<data>/apps/<id>/`; list (broken folders are listed with their problem), bundle, clone (“X (copy)”, unique id), save one file (the manifest is validated, its id can't change), delete (folder, settings, storage, keys), export (zip via `ZipWriter`), import in two steps (preview with a token, then install; unsafe paths, sizes and a missing entry are refused; a zip with one top folder is accepted; a built-in id gets a new id; an own id is replaced and its grants and keys forgotten), prefs (the dialog can only remove grants), grant, keys, clear storage.
  - `builtin.rs`: the four apps from `utility-apps/<id>/`, compiled in with `include_str!`; a test checks their manifests and that they import only what the frame provides.
  - `runtime.rs` `AppRuntime`: each SDK method maps to a capability; undeclared → `undeclared`, not granted → `needsPermission`. Text: the app's model (setting, else the manifest's when available, else the first model with tools); prompt or messages; images and documents (base64) on the last user message; streaming deltas as events; JSON mode by instruction (+ schema) with a lenient parser and one retry. Media through `Generation::run_on` with the app's override (`AppGenerationOverride::apply` over the global endpoint and the manifest's model) and keys tried in order: the app's, then the global one only when the app keeps the global endpoint. Web search; fetch as Markdown or HTML (`WebTools::fetch_raw`, new); crawl. Connectors (list, call). Python in `<data>/app-work/<id>`. Sandbox, browser agent and Chrome with the session `app-<id>` (each checks its setting is on). Storage. Cancel per call.
  - `crawl.rs`: robots.txt (the Oxide group, else `*`; longest match, `*` and `$`), anchor links, same-origin breadth-first crawl (≤ 50 pages, depth), skipped pages with reasons.
  - `generation.rs`: `endpoint_keyed` and `run_on` (keys and endpoint given by the caller); `tools/web.rs`: `fetch_raw`/`RawPage`; `Tools::mcp()`.
  - `data.rs`: Export all includes `apps/`; Delete all removes the apps' keys (`app_secret_names`).
- **src-tauri**: `app_frame.rs` (the `oxide-app` scheme; CSP with no network at all: no `https:` sources, `connect-src 'none'`), sharing `artifact_frame::serve`/`Frame`; commands in `commands/apps.rs` (`list_apps`, `app_bundle`, `clone_app`, `delete_app` (also forgets the app's sandbox and browser), `save_app_file`, `export_app`, `pick_app_import`, `install_app_import`, `app_prefs`, `save_app_prefs`, `grant_app_capability`, `set_app_key`, `clear_app_storage`, `app_call` (JSON in and out), `cancel_app_call`, `save_app_output`); event `AppCallEvent`; capabilities; the main CSP's `frame-src` allows the app frame.
- **frontend**:
  - `app-frame.html` + `src/appframe/`: `main.ts` (removes Tauri globals, Tailwind browser build with the `dark` class variant, module map: react, react-dom, lucide-react, react-markdown, remark-gfm, jszip, oxide), `loader.ts` (multi-file CommonJS loader: relative imports, index files, JSON, CSS → `<style>`), `protocol.ts` (messages), `sdk.ts` (the `oxide` module: `ai.text/json/image/video/music/info`, `web.search/fetch/crawl`, `connectors`, `python`, `sandbox`, `browser`, `chrome`, `files.pick/save/zip/toBase64/toDataUrl`, `storage`, `app.copy/theme`, `OxideError`).
  - `features/apps/`: `AppsPage` (gallery: My apps, Built-in, search, category chips, Import with a preview dialog), `AppPage` (`/apps/:id`: Reload, Settings, Clone, Edit for own apps, Export, Delete), `AppFrame` (the frame, approval cards, error bar, theme), `host.ts` (answers calls: asks once per capability, Deny for the visit, media paths → Blobs, `files.save` and `app.copy` in the main window), `AppSettingsDialog` (text model, overrides and keys for declared kinds, permissions with Revoke, storage), `CodeDialog` (plain editor), `AppIcon`.
  - Sidebar item **Utility Apps** after Projects (`nav.apps` in en/da), breadcrumb, routes.
  - `utility-apps/`: the four apps ported from AI-Toolkit onto the SDK (Gemini calls → `ai.text/json`; sitefactory.dk fetchers → `web.fetch`; react-dropzone, motion and clsx dropped), with `tsconfig.json` checked by `npm run typecheck`.
- **Tests**: core 5, store 2, agent: builtin 1, crawl 3, runtime 2, apps 10 (listing, clone/edit, export/import round trip and refusals, ask once and revoke, text/stream/images/JSON retry, storage, connectors, keys removed with the app, fetch + crawl against a local site, cancel), data export (apps folder); acl; app frame CSP. Vitest: `appframe/loader.test.ts`, `features/apps/Apps.test.tsx` (gallery, approvals once/deny/undeclared, media blobs, files.save, clone → edit → export, import preview, settings revoke), `features/apps/BuiltinApps.test.tsx` (the four apps' happy paths with a fake SDK). Fake backend: `src/test/fakeApps.ts`.

Deviations from PLAN.md (and why):
- **Files and storage don't ask.** The user picks every file and save location in a dialog, and storage belongs to the app; asking would add a card with no decision in it.
- **Deny lasts for the visit** (not stored): the next visit asks again, so a mistaken Deny doesn't need Settings.
- **Edit is a plain file editor** for own apps; the AI builder with Preview/Code is Phase 21.
- **JSON output by instruction**, not a provider JSON mode (Oxide's request type has none; it works the same on every endpoint), with one retry.
- **Stricter frame CSP than artifacts**: no remote scripts, styles, images or fonts, so an app can't send data anywhere except through the SDK.
- **Bundled libraries**: react, react-dom, lucide-react, react-markdown, remark-gfm, jszip (new dependency) and oxide. The ports don't use motion, react-dropzone or clsx.
- **Imports through a file picker** (no drag-drop onto the gallery yet).

Known issues / TODO carried forward:
- Visual bugs the user noticed during the manual checks (2026-11-20). Fixed so far: dialogs opened with a “Close” tooltip under their ✕ (they focus it on opening), so the tooltip is now off on every dialog's close button (`tooltip={false}`; the accessible name stays).
- Check run 2026-11-20 14:19 (for the tooltip fix): the extension-update fallback kicked in (the old folder was busy) and worked, but uninstall then left that busy folder behind (held 25 s+), failing the extensions test. Uninstall now retries busy folders in the background for about two minutes (`remove_later`), each install sweeps folders of uninstalled extensions and superseded versions (`sweep_leftovers`, under the `busy` lock), and the test waits up to 60 s for the folders to go. Two old Vitest tests (Artifacts' sandboxed frame, ChatPhase4's branch navigator) timed out in that slow run; they passed in every earlier run today, so treated as load-related.
- Only text files of an app reach the frame (images in an app's folder are kept and exported but not loadable yet).
- Generated media reach the frame as base64 over IPC (as in Phase 19); fine for images and short clips.
- An app's Python working folder (`<data>/app-work/<id>`) isn't removed when the app is deleted (Delete all removes it).

Notes for Phase 21:
- The builder's model tools write into `<data>/apps/<id>/` with `UtilityApps::save_file` (validated paths, manifest checks); add delete/rename there.
- The SDK's types for the builder's system prompt: `src/appframe/sdk.ts` (generate a `.d.ts` or embed the file).
- Compile errors and runtime errors already reach the host (`type: "error"` messages); forward them to the builder chat for **Fix with AI**.
- `AppFrame` takes a bundle and a reload key; the Preview pane can reuse it with unsaved files.

Verified (agent side): fmt ✅ clippy ✅ cargo test ✅ (Linux: every library crate; acl in a scratch crate) tsc ✅ (but `jszip`) eslint ✅ prettier ✅. Windows (`phase20.cmd`, fourth run): fmt ✅ prettier ✅ clippy ✅ cargo test ✅ typecheck ✅ lint ✅ Vitest ✅ vite build ✅; committed `1915243`. Manual exit checks ✅ (by the user).

**Windows run 1 (`phase20.cmd`, 2026-11-20 09:31–09:46):** npm install, bindings, fmt, prettier, lint and vite build passed; not committed.
- clippy: `artifact_frame::allowed` is now only used by tests (`#[cfg_attr(not(test), allow(dead_code))]`).
- typecheck: the generated `AppManifest` marks fields with a serde default as optional (`entry?`, `icon?`, `capabilities?`, …); the frontend now falls back to the defaults.
- Vitest: the loader and the four built-in apps passed; the six gallery tests failed because `renderApp` reset the fake apps set up before it. The fake apps now reset in the test file's `beforeEach`, and two queries were made exact (the gallery loads asynchronously; a toast also says “My apps”).
- cargo test: only the known flaky `oxide-mcp` extensions test (os error 32, see Phase 19). Re-run.

**Windows run 2 (10:07–10:14):** everything passed (cargo test too) except one Vitest test: the media test stored its fake file before `renderApp`, which resets the fake generation backend. It now stores it after. Not committed yet.

**Windows run 3 (10:22–10:28):** Vitest, clippy, typecheck and lint passed; `cargo test` failed only on the `oxide-mcp` extension-update test again (os error 32, its second failure in three runs). As decided in Phase 19, updates no longer depend on renaming a busy folder: `Extensions::install` moves the old version aside with a short retry (about 3 s), and when Windows still holds its files, installs beside it as `<name>~<id>`. `current_dir` picks the newest of `<name>` and `<name>~<id>` (names can't contain `~`); the old folder is removed right after, or at the next start (`clean_staging`); uninstall removes every version; Export all takes each extension's manifest from its current folder. Test: `the_newest_version_folder_is_the_current_one`. Checked on Linux: fmt, clippy, every oxide-mcp test and the agent's data tests.

## Phase 19 — Generation endpoints & media generation — 2026-11-19
Status: **done** (2026-11-20; every exit check passed by the user). It was first built and tested on the agent side. Every Rust test passes on Linux (library crates, clippy `-D warnings`, fmt, and the acl/CSP tests in a scratch crate); tsc, ESLint and Prettier pass on the device. `scripts\win\phase19.cmd` does the rest:
- it regenerates the bindings and runs every check and vite build on Windows;
- it commits with `logs\commit-msg-phase19.txt` only when all of that passes (no push);
- it then starts the app for the manual exit check.

The exit checks were passed by the user on 2026-11-20 (see below).

**API check (2026-11-19, OpenAI's reference at developers.openai.com):**
- **OpenAI shut down the Videos API (Sora 2) on 2026-09-24.** No one-to-one replacement exists. Oxide's `/videos` client follows OpenAI's last published shape: JSON `POST /videos` {model, prompt, `seconds` as a string, size, `input_reference.image_url`}, then `GET /videos/{id}` (`queued`/`in_progress`/`completed`/`failed`, `progress`), then `GET /videos/{id}/content`. Gateways that kept that API work with it (CometAPI, haimaker, apiyi, LLM Gateway, …). The video exit check therefore needs such an endpoint, **not OpenAI**.
- `gpt-image-1` was shut down on 2026-10-23, so the default image model is `gpt-image-2`. GPT image models always return base64 and reject `response_format`, so Oxide never sends it. It reads `b64_json`, or downloads `url` (without the key).
- `/audio/speech` takes `input` up to 4096 characters, a voice, `response_format` (mp3/opus/aac/flac/wav/pcm), `instructions` and `speed` (0.25–4). `tts-1`, `tts-1-hd` and `gpt-4o-mini-tts` are announced to shut down on 2027-01-06.

Exit criteria:
- In a chat on Windows, each of these works against endpoints set in Settings → Media generation:
  - [x] "make an image of …" (checked by the user, 2026-11-19, on their local server);
  - [x] "make a 5-second video of …". It first failed: the user's server has no `/videos` job API. After Follow-up 3, the user checked it on comfy-bridge `wan2.2-5b` (2026-11-20);
  - [x] "make a 30-second song about …" (checked by the user, 2026-11-20) on the user's comfy-bridge server (`http://192.168.8.80:8190/v1`, model `ace-step`). This replaced "read this paragraph aloud"; see the music follow-up below.
- [x] Changing a default changes which model and endpoint are used (checked by the user, 2026-11-20). Check the Request in the tool line and the endpoint's own log.

Done:
- **oxide-llm `generation.rs`:**
  - `generate_images`: JSON to `/images/generations`. With reference images it sends multipart to `/images/edits`, as `image` for one image and `image[]` for several (up to 16).
  - `generate_speech`, `generate_video` (polling every 5 s, up to 20 min, with a progress callback) and `list_model_ids` (all of `GET /models`, image, speech and video models included).
  - 404, 405 and 501 answers that don't mention a model become "this endpoint doesn't offer … (/path)".
  - A finished video comes from `/content`, or else from the job's `video_url`/`url`.
  - Results are capped at 200 MB. A new `LlmError::Generation` carries plain messages.
  - `http::Multipart` is shared with `transcribe.rs`. The test server can send binary replies.
- **oxide-core:**
  - `generation.rs`: `GenerationKind`, `GenerationEndpoint` (provider id or custom URL, plus model) and `GenerationSettings` (four endpoints, voice, audio format) as `AppSettings.generation`.
  - Also there: `GenerationTest`, `ToolProgress`, and the tool names and `GENERATION_SWITCH`.
  - `StreamEvent::ToolProgress` and `ToolUsageKind::Generation`, stored as "generation" in the tool log.
- **oxide-agent:**
  - `Generation` resolves endpoints. A custom URL wins over a provider; its key comes from the keychain entry `generation:<kind>`, else the provider's key. Anthropic endpoints are refused with a reason.
  - `test` checks the model against the endpoint's list and generates nothing, so it costs nothing. `models` gives the model list, and `run` runs the three tools.
  - Generated images are stored like attachments, so models get an image they accept. When the stored copy was scaled, the original is kept as `<file>.original` for downloading. Video and audio are stored unchanged (`save_media`).
  - Tool results are text plus an `Image` block, or a `Document` block with `video/*` or `audio/*`. Providers turn those into "can't be read by this model" text.
  - `AttachmentStore::read_file` serves the player and downloads (it prefers `.original`).
  - Chat: each kind that is set up gets its tool, unless the "+" menu's `generation` switch or a tool's own name is in `disabledBuiltins`.
  - Each call asks, like `run_python`, with "Allow always for this chat". Scheduled tasks set to run without asking don't ask.
  - `use_attached_images` / `use_attached_image` take the newest images in the chat. Those are the user's attachments or images made earlier, up to 4 (a video takes 1).
  - A video's progress streams as `ToolProgress`, and Stop ends the wait. Usage labels are the prompt's first line.
  - Delete all removes the `generation:*` keys.
- **src-tauri:**
  - Commands `generation_keys`, `set_generation_key`, `test_generation`, `generation_models`, `attachment_media` (base64 for a `blob:` URL) and `save_attachment_file` (a save dialog).
  - Capabilities updated. The CSP gains `media-src 'self' blob:`, and `acl.rs` checks it.
- **Frontend:**
  - Settings → Media generation (`features/generation/GenerationSection.tsx`) has three cards: Image, Video, Audio (the first build also had Text; see the follow-up below). Each header shows the chosen model.
  - Each card has an endpoint (non-Anthropic providers), a custom URL, the model (suggestions plus **List models**), its own API key (Save/Remove), and **Test**. Audio adds the voice and format.
  - The "+" menu's **Generate media** appears when a kind is set up.
  - Tool lines say "Making an image", "Making a video" and "Reading aloud" with the prompt, and show the progress ("Making the video 42%").
  - Under the line (`GeneratedMedia`): the images with Download, and video and audio players with Download.
  - "Used in this chat" shows "Generated media". The nav label is translated (`section.generation` in en and da).
- **Tests:**
  - Rust: llm generation (9), agent generation (5: resolution and keys, Test, all three tools stored, failures), `generation_tools_are_offered_ask_and_store_their_results` (chat), core settings (3), Delete all's key names, and acl's `media-src`.
  - Vitest: `features/generation/Generation.test.tsx` covers Settings → Generation, video progress → player → download, and the headlines and usage rows. The fake is `src/test/fakeGeneration.ts`.

Deviations from PLAN.md (and why):
- **Video isn't checked against OpenAI**, which no longer offers it (see above).
- **Test doesn't generate.** It lists the endpoint's models and looks for the chosen one. A real generation costs money, and the first chat call shows real errors anyway.
- **No Text endpoint** (dropped by the user after the first build): chats keep their own model, and Utility Apps pick a chat model from Providers and Models.
- **Reference images are the newest images in the chat** (the model sets `use_attached_images`). Models don't see attachment paths, so they can't pick a file.
- **Video is sent as JSON, not multipart.** OpenAI's reference lists JSON, and the gateways checked accept it.

Known issues / TODO carried forward:
- Generated files stay in the attachments folder when a chat is deleted (the existing per-chat sweeping issue from Phase 4). Delete all removes them.
- Speech is one request of at most 4096 characters. The model is told to split longer text over several calls.
- A stopped video job may still finish (and be billed) on the server. Oxide only stops waiting.
- The player loads the whole file over IPC as base64 (up to 200 MB). That is fine for seconds-long clips; a streaming URI scheme would be better for long media.
- The scheduled task dialog has no "Generate media" switch. Runs use whatever the endpoint settings allow.

Notes for Phase 20:
- `oxide_agent::Generation` is the backend for the Utility Apps SDK's image, video and audio calls. `run` takes the tool input shape, and `endpoint(kind, &GenerationEndpoint)` resolves per-app overrides: pass the app's endpoint instead of the global one.
- Per-app keys need their own keychain names (for example `app:<id>:generation:<kind>`). Delete all must remove them, and `data::secret_names` should learn the pattern.
- Apps' text generation uses a chat model from Providers and Models, through `ModelRegistry`. It is the per-app model, or by default the first available one or the one the app's manifest names. Phase 19's Text endpoint was dropped.
- New user-facing strings go through `useT` only for the shell, as before.

**Windows runs of `phase19.cmd`:**
- **11:52:**
  - The bindings regenerated cleanly. fmt, clippy, typecheck, lint and vite build passed.
  - Two of the new Vitest tests were wrong: the endpoint was picked before the provider list loaded, and the fake video was stored before `renderApp`, which resets the fake backend. Both are fixed.
  - `oxide-mcp`'s Phase 10 test `install_configure_toggle_update_uninstall` failed with os error 32.
- **12:23:** everything passed except that same test, again.
- **`scripts\win\ext-diag.cmd` (12:43):** run alone, the test passed 3 out of 3 times in 0.3 s, and no mock server was left running. It fails only beside the binary's other two tests, one of which starts a real `node` server.
  - The kill path is sound: a Job Object with `KILL_ON_JOB_CLOSE`, a non-inheritable job handle, a kill after a 2 s grace period, and std's spawn lock against handle leaks. So the cause is unconfirmed (a scanner, or something in parallel spawning).
  - **Change:** `crates/oxide-mcp/tests/extensions.rs` now runs its three tests one at a time (a shared `tokio::sync::Mutex`). They test install flows, not concurrency.
  - Known issue: if users ever see "the installed version couldn't be replaced" while updating an extension, look here again.
- **`phase19-music.cmd` (22:45–22:54):** the test failed again with os error 32, even with its tests serialized. Rename retries ran out after about 20 s (the test took 22.6 s).
- **`scripts\win\ext-diag2.cmd` (23:20):** the whole test binary passed 4 out of 4 times, in about 0.5 s each. A sampler checked every 250 ms and found no test server left running, and Windows' Restart Manager reported no process holding files in the extension folder.
  - So nothing in Oxide holds the folder. The failure shows up only in the full `cargo test --workspace` run right after a fresh build, when the machine is busiest.
  - The likely cause is a scan by antivirus (Defender) that lags under load. It is not proven.
  - **Decision:** re-run (the second run, 23:31, passed), and keep it as a known issue. If it comes back, make updates install into a new folder instead of renaming the old one.

Verified (agent side): fmt ✅ clippy ✅ cargo test ✅ (Linux: every library crate; acl/CSP in a scratch crate) tsc ✅ eslint ✅ prettier ✅. Windows (`phase19.cmd`, third run 13:01–13:06): fmt ✅ prettier ✅ clippy ✅ cargo test ✅ typecheck ✅ lint ✅ Vitest ✅ vite build ✅; committed `dcd3bdb` (not pushed). Manual exit check: pending.

**Follow-up: "Media generation" without Text (user's request, 2026-11-19 evening):**
- Settings → Generation is now **Settings → Media generation** (the nav label in en and da; `section.generation` = "Media generation" / "Mediegenerering"). The route stays `/settings/generation`.
- The **Text** group is gone: chats always used their own model, and text comes from Providers and Models. `GenerationKind` is now `image | video | audio`, and `GenerationKind::tool()` returns `&str`. `GenerationSettings` has no `text`. A `text` entry saved by the first build is ignored, and Delete all still removes the old keychain entry `generation:text`.
- PLAN.md changes:
  - Phase 20's per-app text model is a chat model from Providers and Models: the first available one, or the one the app's manifest names.
  - Phase 21's builder model is picked in the builder.
- Checked: Rust fmt, clippy and tests on Linux; tsc, ESLint and Prettier on the device. The Windows checks and commit run in `scripts\win\phase19-media.cmd`.

**Follow-up 2: Music generation instead of speech (user's request, 2026-11-19 22:15):**
- The user's audio server is **comfy-bridge**, which is local and custom: `http://192.168.8.80:8190`, with Bearer auth. Its `POST /v1/music/generations` (also `/v1/audio/music`) is **synchronous**: one request returns one finished MP3.
  - Request: `model` (`ace-step`), `prompt` (style tags), `lyrics` (`[verse]`/`[chorus]`/`[bridge]`; leave out for an instrumental), `duration` (5–240 s, default 30), `seed`, `steps`, `cfg_scale`, `lyrics_strength`.
  - Reply: `data[0]` with `format`, `url` (works without a key) and `b64_json`.
  - The server sends a space every 20 s while it works, and a failure after the 200 comes as `{"error":…,"data":[]}`. Requests can wait about 11 min behind a video job.
- **oxide-llm:**
  - `generate_music` + `MusicRequest` send `model`, `prompt`, `lyrics`, `duration` (clamped to 5–240) and `seed`, with a 20 min timeout. The tool doesn't expose `steps`, `cfg_scale` or `lyrics_strength`, so the server's defaults apply.
  - It reads `b64_json`, or downloads `url` without the key.
  - A 200 with `error` becomes "the endpoint couldn't make the music: …", and 404/405 becomes "doesn't offer music generation".
  - `list_model_ids` treats 404/405 on `/models` as an empty list, so Test says "Connected. The endpoint lists no models…" instead of failing.
  - Test's failure text is now "The test failed: …" instead of "Couldn't reach the endpoint", which was wrong for a rejected key.
  - `generate_speech` stays in the library, unused, for later.
- **core:**
  - `GenerationKind::Audio` became `Music` (`"music"`), and `GENERATE_MUSIC` = `generate_music`. `GenerationSettings.music` defaults to the model `ace-step`.
  - `audio`, `voice` and `audioFormat` are gone; old saved values are ignored. Delete all also removes `generation:audio`.
- **agent:**
  - The `generate_music` tool takes `style` (required), `lyrics`, `duration_seconds` and `seed`. Its description tells the model to write lyrics unless the user gave them.
  - The tool line shows "Making the music" while it waits. The result is an audio player plus Download (`generated-music.mp3`).
- **Frontend:**
  - The card is "Music generation", with a music icon and `ace-step` suggested. Voice and format are gone.
  - The tool line says "Making music" / "Made music" with the style. The "+" menu's text is updated.
- **Tests:**
  - llm: `music_is_made_in_one_request` (leading spaces, base64, link, error-in-200, 400, 404, no key on the link) and `a_missing_model_list_is_an_empty_one`.
  - agent: the three-tools test now makes music, and Test covers 404 and 401.
  - Vitest: the Music card and its custom URL.

**Follow-up 3: synchronous video (user's correction, 2026-11-19):**
- Video failed on the user's test server. Oxide sent `POST {base}/videos` (OpenAI's job API), but the server only has **`POST /v1/videos/generations`, which is synchronous**:
  - comfy-bridge `http://192.168.8.80:8190/v1`, model `wan2.2-5b`, accepts `model`, `prompt`, `duration` (1–5 s), `size`, `seed`, `negative_prompt`, `steps`, and `image` (a data URL or http URL, for image-to-video).
  - OmniRoute `http://192.168.8.80:20128/v1`, model `openai-compatible-images-generations-comfy-bridge/wan2.2-5b`, passes on only `model`, `prompt` and `duration`.
  - It replies `data[0]` with `b64_json`, and the bridge adds a `url`. 2 s at 1280x704 takes about 11 min, and the GPU runs one job at a time.
- **oxide-llm:**
  - `generate_video` still tries the job API first. On 404/405/501 it sends one `POST /videos/generations` with `model`, `prompt`, `duration` (a number), `size` and `image` (a data URL), and waits up to 30 min.
  - The reply is read like music, with a shared `sync_file`: leading spaces are skipped, `error` in a 200 is an error, and `b64_json` or `url` is used. The type is `video/mp4` unless the format or download says otherwise.
  - If both paths are missing: "doesn't offer video generation (/videos or /videos/generations)".
  - Image requests and music now share `LONG_TIMEOUT` (30 min), since an image can queue behind a video on a local GPU. It replaces `MUSIC_TIMEOUT` (20 min).
- **agent:** the video tool's description now says it can take up to a quarter of an hour, and that local models often make only 1–5 s.
- **Frontend:** the Video card's hint names both API shapes.
- **Tests:** llm `a_video_without_a_job_api_is_made_in_one_request` (404 then base64, 405 then link, error in 200, the body with `duration`, `size` and the data-URL `image`); the missing-endpoint tests expect the new text.

## Maintenance — Vitest 5 — 2026-11-19
Between Phase 18 and Phase 19. It closes the last `npm audit` findings, which were dev-only: Vitest ≤ 4.1.10, @vitest/mocker and tinypool (2 critical).
- `vitest` goes from `^3.2.0` to `^5.1.1`. Vitest 5 needs Node 22.12+ and Vite 6.4+ (Oxide has Vite 7.3); `scripts\win\vitest5.cmd` checks the Node version.
- `.vitest/` (Vitest 5's output folder) is ignored by git, Prettier and ESLint.
- Vitest 5's new defaults that touch Oxide:
  - `clearMocks` is on, so mock call history is cleared before each test.
  - `jsdom` globals assignments now reach the DOM.
  - The tests already put `vi.mock` at the top level and use none of the removed APIs.
- **ESLint 10 is not done.** `eslint-plugin-react-hooks` 5.x declares ESLint ≤ 9, and newer releases bring the React Compiler rules. Revisit when the plugin officially supports ESLint 10. ESLint 9 has no open audit findings.
- Also in this commit: `scripts\win\push.cmd`, which runs `git push` in a console so Git Credential Manager can sign in. It was needed on the new PC, where the first push from a script failed with "Invalid username or token". git's `user.email` was fixed on 2026-11-19; `ff92037` carries the placeholder email.
- Phase 18's CI run #8 (`ff92037`) is green on all three systems (11 m 22 s).

## Phase 18 — Hardening & polish (+ usage dashboard, HTML export, dictation) — 2026-11-15/19
Status: **done**. Every check is green on Windows (`phase18.cmd`, 2026-11-19 08:25–08:38), and the user passed every manual check (08:54). Committed by `scripts\win\phase18-close.cmd` (message `logs\commit-msg-phase18.txt`).

The phase was built in two sessions. The first paused at the usage limit, and its work in progress was committed by accident in `490ddf1` (`commit.cmd` runs `git add -A`), which turned CI #7 red at `cargo fmt --check`; this phase's commit fixes that. Between the sessions the project moved to a new PC (user folder `C:\Users\mo`).

The scope was agreed with the user: hardening plus all three stretch items. Dictation goes through an OpenAI-compatible `/audio/transcriptions` endpoint. Danish covers the scaffolding and the main shell only.

Exit criteria:
- [x] Security review: path sandbox fuzzing, IPC review (chosen paths, capabilities), bridge fuzzing, MCP trust prompts, secret redaction in logs, Linux Secret Service. Written up in `docs/SECURITY.md`.
- [x] Performance:
  - Long chats mount only the newest 40 messages, plus `content-visibility`.
  - Start-up time and memory are logged and shown in About.
  - The 50-message demo chat scrolls smoothly (by hand).
- [x] i18n scaffolding: English and Danish (by hand: the Danish shell).
- [x] Stretch items, all checked by hand:
  - the usage dashboard (Settings → Usage);
  - export as a web page, which opens in a browser with its images;
  - dictation with a cloud endpoint.
- [x] Known issues triaged (below), and `npm audit` triaged.

Manual checks (Windows, 2026-11-19, by the user, all OK): the trust card for a server added by editing `oxide_config.json` (Trust and start), the Danish shell, Settings → Usage, HTML export in a browser, dictation, About's numbers, and the demo chat's scrolling.

Done:
- **core:**
  - `redact.rs`: secret redaction of key prefixes, Bearer/Basic credentials, JWTs and secret-named pairs.
  - `usage.rs`: `UsageReport` by provider, model and local day.
  - Settings `appearance.language` (`UiLanguage` system|en|da) and `dictation` (`DictationSettings`).
  - `McpStatus::NeedsTrust` and `McpServerState.trustChanged`.
- **mcp:**
  - `trust.rs`: a SHA-256 fingerprint of command, args and env per server name, kept in `<data>/mcp-trust.json`. The first start seeds the servers already configured.
  - The manager holds new or changed stdio commands in `NeedsTrust`; `McpManager::trust` releases them.
  - Saves in Settings, the import and the editor trust as they save; extensions are trusted on install.
  - ServerLog lines are redacted.
  - `process::own_memory_bytes()` (Windows `K32GetProcessMemoryInfo`, Linux `VmRSS`).
- **store:**
  - Linux Secret Service behind the feature `secret-service` (`sync-secret-service` + `crypto-rust`), which src-tauri enables. keyutils is the fallback, and old keyutils keys migrate when read.
  - `usage_records()`.
- **llm:** `transcribe.rs` (multipart written by hand).
- **agent:**
  - `markdown_html.rs` (safe Markdown → HTML).
  - `export_html`: a self-contained page with a CSP meta tag and `data:` images.
  - The fuzz test `generated_paths_never_leave_the_grant`.
- **bridge:** fuzz tests for WebSocket frames (random and mutated frames), and 130 garbage clients after which pairing still works.
- **src-tauri:**
  - `logging.rs`: a redacting writer to stderr and `<logs>/oxide.log`, closed before Delete all.
  - `chosen.rs`: paths from pickers and drops. `resolve_folder`, `preview_extension` and `add_skill` confirm any other path in a native dialog.
  - Commands `mcp_trust_server`, `usage_report`, `app_ready`, `app_metrics`, `transcribe_audio`, and `ExportFormat::Html`. Capabilities updated, and the acl tests pass.
- **Frontend:**
  - `src/i18n` (en/da, `useT`, the `html lang` attribute).
  - Sidebar, profile menu, Home, composer and the Settings nav are translated. Appearance → Language.
  - Connectors' "Trust and start" card.
  - Settings → Voice and the composer mic (`features/dictation`).
  - Settings → Usage (`features/usage`).
  - About's start-up time and memory.
  - The chat menu's "Export as web page".
  - `features/chat/messageWindow.ts`.
- **Second session:**
  - The 4 Vitest failures on the new PC were fixed. Two were test timing or modal issues (the MCP status update arrives on TanStack Query's timer; Settings is a modal over the sidebar). One was a jsdom gap (`Blob.arrayBuffer()` is missing; dictation now reads with `FileReader`). One was a real bug: the mic opened Settings → Voice when clicked before settings had loaded, and it is now disabled until they load.
  - The bindings are regenerated. They differed from the hand edits only in doc comments and order.
- **Tests:**
  - Rust: redaction, usage, trust, transcribe, markdown_html/export_html, the fuzz tests, `chosen`, logging, and `new_or_changed_local_commands_wait_until_trusted`.
  - Vitest: `features/usage/Phase18.test.tsx` (language, usage, About, dictation), the Mcp trust test, ChatPhase4 additions, `messageWindow.test.ts`.

Deviations from PLAN.md (and why):
- **A message window instead of a virtualized list:**
  - The newest 40 messages are mounted, and older ones on scrolling up, with `content-visibility: auto`.
  - Real virtualization fights streaming, variable heights and scroll anchoring.
  - The demo chat scrolls smoothly.
- **Start-up under 1.5 s is not measured on a release build yet:**
  - The dev build logged 10.3 s, which includes Vite's first compile and an unoptimized binary.
  - Measure it on the next installed release (About shows it, and `oxide.log` logs it).
- **Dictation uses provider speech-to-text** (OpenAI-compatible `/audio/transcriptions`), not local `whisper-rs`:
  - It needs no native build or model download.
  - A local Whisper server works through the same endpoint.
- **Danish covers the shell only** (sidebar, menus, Home, composer, Settings nav). Settings pages stay English, as agreed.
- **Extensions are trusted on install.** Their install dialog is the confirmation, so they don't get a second trust card.

npm audit (15 findings: 5 low, 8 moderate, 2 critical; `logs\npm-audit.txt`):
- **KaTeX < 0.18.2** (low, GHSA-238p-pmpm-9mq7). It ships in the app, but needs an existing prototype pollution to matter, and Oxide doesn't set `trust`. **Fixed:** `katex` is pinned to `0.18.10` with an `overrides` entry, so mermaid and rehype-katex use it too. 0.18.11 slipped in breaking changes.
- **minimatch 7–11** (moderate, dev-only, via typescript-eslint). **Fixed** by `npm audit fix` in `phase18-close.cmd`.
- **Vitest ≤ 4.1.10, @vitest/mocker, tinypool ≤ 2.1.1** (2 critical, 2 moderate). Dev-only test tooling, never shipped, and only reachable through worker options Oxide controls. **Deferred:** the fix is Vitest 5, a major upgrade. Do it as its own step before Phase 19 (see below).
- After the close-out (`logs\npm-audit-after.txt`): 3 findings are left, all in the deferred Vitest/tinypool group. `npm ls katex` shows 0.18.10 everywhere. The first close-out run (08:58) didn't commit because `npm audit fix` exits 1 while findings remain; that step is now informational.

Known issues triage (every "Known issues" list in this file and the unchecked boxes, 2026-11-19):
- **Fixed by this phase:**
  - The Linux keychain (now the Secret Service).
  - MCP servers start without a trust prompt (Phase 5) and extensions are not covered by it (Phase 10). Both are now handled by the trust card, and extensions by their install dialog.
  - "`src/bindings/index.ts` edited by hand" (Phases 2–17). The bindings are regenerated, and the hand edits matched.
- **Superseded by later phases** (no action):
  - Phase 0's placeholder icon.
  - Phase 1's mock data and placeholder controls.
  - Phase 2's "no replies yet", the unused `enabled` flag, and model picker notes.
  - Phase 3's tool calls not run and the Phase 4 work that was still open.
  - Phase 5's tools not given to models and the web search placeholder.
  - Phase 9's `menu.rs` built only on macOS (built by CI since Phase 17).
  - Phase 10's "Vitest runs only on Windows" (CI runs it on 3 systems).
- **Accepted** (by design or a platform limit; documented where users see it):
  - Scheduled tasks run only while Oxide runs. DST skips an hour once a year. "Ask me" runs wait without a timeout.
  - Unix grandchildren may survive a timeout (Phases 11 and 12).
  - Elevated windows ignore input (UIPI). Key combos follow the keyboard layout. The UI Automation timeout counts as "not sensitive", and this is logged.
  - The `.deb`/`.rpm` packages only check for updates. Linux needs Ubuntu 24.04+.
  - The identifier `dev.oxide.app` stays (changing it moves data and keys).
  - WebView2's EBWebView folder isn't deleted (the window clears its own storage).
  - WebView2 injects `__TAURI_INTERNALS__` into the artifact frame. It has no IPC rights (opaque origin, no capability).
  - `web_fetch` can reach local addresses (every fetch is shown). The allowlist is a prefix check, so chained commands always ask.
  - Pairings are bound to the extension id. Edge is used for the real-extension test.
  - Search is prefix matching. Token counts are estimates. Phase 13b's Strata context-window default.
  - Browser-Use needs capable models. CDP mode's open-debugging warning. Docker `cp` name limits and the non-root image user. A changed sandbox image replaces the container.
  - Python is limited to the locked packages. Skill scripts aren't run. A switched-off skill can't be read before saving.
  - Act warnings in Vitest output. Alt+Space takes over Windows' window menu. Dev-build notification and start-at-login quirks.
  - npm 12's blocked postinstall (Vite works).
- **Open, carried forward** (none blocks a release):
  - Phase 16's computer-use exit check with Claude, the Ctrl+Alt+Esc check by hand, and the safety spot checks. The local model's accuracy was accepted by the user. Also: re-check the foreground app before each typed piece, and numbered UI Automation marks for weak models.
  - Phase 17's macOS (`.dmg` on the MacBook) and Linux (`scripts\win\test-linux.cmd`) installer checks (deferred by the user). Code signing, when certificates exist.
  - Phase 4:
    - Orphaned attachment files when a chat is deleted. Delete all removes everything; per-chat sweeping is still open.
    - LM Studio thinking is untested.
    - OpenAI `xhigh`/`minimal` reasoning levels.
    - Footnote ids can repeat across messages.
    - Attachments can't be changed while editing a message.
  - Phase 3: a reply streaming during a window reload isn't shown live again (Phase 6 likewise).
  - Phase 6:
    - Anthropic `redacted_thinking` isn't stored.
    - There is no UI for the per-chat "Allow always" list.
  - Phase 7: the memory pass uses the chat model on local servers. The minute in the system prompt changes the cache key.
  - Phase 8:
    - React artifacts can import only react, react-dom and lucide.
    - HTML artifact errors aren't reported back.
    - Artifacts from regenerated branches stay listed.
  - Phase 11: `read_file` reads text only. Moves across drives fail.
  - Phase 13b: exports don't include compactions.
  - Phase 14: the Web Store build. A running Chrome action finishes after Stop.
  - Phase 5: the first `npx -y` start may exceed the 20 s probe.
  - Phase 10: malformed tool arguments from small local models.
  - Phase 0: ESLint 9 → 10 (with its plugins). Do it together with Vitest 5.
  - The unchecked optional boxes: Phase 13's "My own Chrome" (CDP) check, Phase 6's web search with a service, and Phase 3's OpenAI and LM Studio runs. Covered by tests and later use; no action.
- **New in this phase:**
  - The trust file is per data folder. Delete all forgets it, and the next start trusts what is configured then.
  - The Danish translation covers the shell only.
  - The release start-up time is unmeasured.
  - Dictation needs a cloud or local endpoint.

Notes for Phase 19:
- Before starting: upgrade Vitest to 5 and ESLint to 10 in one step (its own script and commit), then re-run `npm audit`.
- Generated bindings are now committed as generated. Keep regenerating them (`npm run bindings`) instead of editing by hand.
- New user-facing strings go through `useT` with keys in both `src/i18n/en.ts` and `da.ts` (a test checks that the key sets match).
- Generation tools that call endpoints should log through `tracing`, which is redacted, and never put keys in URLs.

Verified: fmt ✅ clippy ✅ cargo test ✅ (Windows, every crate and src-tauri, the acl tests included) typecheck ✅ lint ✅ Vitest ✅ vite build ✅ (`phase18.cmd`, 2026-11-19). Manual checks ✅ (by the user).

## Planning — Phases 19–22 added (Utility Apps) — 2026-11-14
After Phase 17 the user proposed **Utility Apps**: a sidebar item after Projects with a gallery of AI utility apps that can be used, cloned, modified and created in Oxide, modelled on their Google AI Studio app AI-Toolkit (`github.com/sitefactorydk-collab/AI-Toolkit`: 14 webmaster/SEO/content/CMS tools, React 19 + an Express proxy to Gemini, with “Generation endpoints” settings for text, image, video and audio). The user's answers: apps are **small web apps** (React/TS in a sandboxed frame), a **chosen few** of the 14 are ported first and the rest later, apps may do **everything a chat can** with **ask once per app**, Oxide gains **image, video and audio generation** with global default models per kind (Settings → Generation, like AI Studio's) and **per-app overrides**, creating/editing works **like AI Studio's build mode** (chat + Preview/Code switch), export/import of one file is enough for sharing now, and the work starts **after Phase 18**. PLAN.md got **Phase 19** (generation endpoints & media generation), **Phase 20** (Utility Apps runtime and gallery; built-ins: Meta Tag Generator, Readability & Tone Analyzer, HTML Table from Image, SVG Creator — changeable), **Phase 21** (builder) and **Phase 22** (the remaining tools).

Later the same day the user decided that **every generation endpoint (text, image, video, audio) speaks the OpenAI-compatible API only**, which covers OpenAI, Gemini's and Groq's compatible endpoints and most local setups (LM Studio, Strata, LocalAI, vLLM). Phase 19 was rewritten to match: no Gemini preset and no provider-specific native calls; an endpoint without a kind of generation says so. Support for Anthropic's and Google's own APIs may come later.

## Phase 17 — Packaging, updates & CI — 2026-11-13
Status: done for Windows (macOS and Linux installer checks deferred by the user) — built on the agent side (Linux: every library crate's tests, clippy and fmt green; `src-tauri/src/acl.rs` tested in a scratch crate; tsc, ESLint and Prettier on the device). Windows checks, the installers and the commit run in `scripts\win\phase17.cmd` (message `logs\commit-msg-phase17.txt`). The exit check needs a GitHub repository (there is no remote yet) and clean VMs.

Exit criteria:
- [x] **Windows** (Sandbox, 2026-11-14) · [ ] macOS · [ ] Ubuntu (deferred) — **Installers from CI install and run on clean Windows 11, macOS and Ubuntu VMs.** Push the repo to GitHub, add the secret `TAURI_SIGNING_PRIVATE_KEY` (docs/RELEASING.md), push a tag `v0.1.0` → the **Release** workflow fills a draft release with `Oxide_0.1.0_x64-setup.exe`, `.msi`, `Oxide_0.1.0_universal.dmg`, `.AppImage`, `.deb`, `.rpm`, `latest.json` and `oxide-chrome-extension-0.1.0.zip`. On each VM: install, start, add a key, chat, Settings → Chrome shows the bundled extension folder, Settings → Data → Export all. (macOS: right-click → Open, unsigned; Windows: SmartScreen → More info → Run anyway.)
- [x] **CI is green on all three systems** (run #2, `ff775fc`, 2026-11-14 15:39).
- [x] Locally (phase17.cmd): the NSIS and MSI installers build, and the installed Oxide runs (About says “This build doesn't look for updates”).
- [x] **Settings → Data** (in the Sandbox): the folders and size show; **Export all…** saves a zip (open it: `manifest.json`, `database/*.json`, `attachments/`, `skills/`, `config/oxide_config.json` with env/headers replaced); **Delete all data…** asks for “delete”, removes everything and Oxide restarts empty (provider keys gone from Windows Credential Manager, start at login off). Try it on the installed copy or after an export.
- [x] **Least privilege**: quick entry (Alt+Space → a message, and the screenshot button) and the computer-use control bar still work with their reduced capabilities; the main window works as before.
- [x] Updates (0.1.0 → 0.1.1 in the Sandbox): Settings → About → Check for updates → Install … and restart.

Done:
- **Bundles** (`src-tauri/tauri.conf.json` + `tauri.{windows,macos,linux}.conf.json` choose the targets): Windows NSIS (per user) + MSI with the WebView2 download bootstrapper (silent); macOS `.app` + `.dmg`, min 11.0, hardened runtime, ad hoc signing (`signingIdentity: "-"`) unless the release workflow has Apple secrets; Linux AppImage + `.deb` + `.rpm` (built on Ubuntu 24.04 since the CI fix). Publisher, category, descriptions, licence. `createUpdaterArtifacts: true`. `beforeBuildCommand` also builds the extension.
- **Sidecars and resources**: the `uv` sidecar per target as before (release builds still refuse to build without it); `scripts/fetch-uv.mjs --target universal-apple-darwin` joins both macOS binaries with `lipo` for the universal build. The built Chrome extension ships as a resource (`"../extension/dist/": "extension/"`); `build.rs` makes a placeholder folder in debug builds when it isn't built and fails a release build. `extension_dir` looks in the resource dir (installed) or, in debug builds, the repo's `extension/dist` first so an existing pairing keeps its extension id.
- **Updater** (`src-tauri/src/updater.rs`, `commands/updates.rs`): `tauri-plugin-updater`; the update URL is compiled in from `OXIDE_UPDATE_URL` (the release workflow sets it to `https://github.com/<repo>/releases/latest/download/latest.json`; local and debug builds have none and say so); the public key is `plugins.updater.pubkey` (written by `scripts/updater-key.mjs`, which makes `%USERPROFILE%\.tauri\oxide-updater.key` once). Commands `update_status`, `check_for_update`, `install_update` (downloads with progress, installs, restarts; Windows' installer restarts Oxide itself); event `updateChanged`; an automatic check a minute after start and every 6 hours while **Settings → About → Check automatically** is on (`AppSettings.updates.checkAutomatically`, default on). Linux `.deb`/`.rpm` installs can check but not install (only an AppImage replaces itself) and say so. Frontend: `features/data/UpdatesCard.tsx` in About, a toast once when a background check finds a version (`useUpdateEvents`).
- **Settings → Data** (`oxide-agent::data`, `oxide-store::dump`, `commands/data.rs`, `features/data/DataSection.tsx`):
  - *Where your data is*: data, config and log folders and the data folder's size; **Show folder**.
  - *Export all*: a save dialog (Downloads, `oxide-export-<date>.zip`) → one zip: `database/<table>.json` for every table (read in one transaction; FTS indexes and the migration log left out; blobs as `{"$hex"}`), `attachments/`, `skills/`, each extension's `manifest.json`, `chrome-pairings.json` (no tokens), `config/oxide_config.json` with every `env` and `headers` value replaced, `manifest.json` (version, tables, files, what's not included) and `README.txt`. Written to `<name>.partial` and moved into place; files over 4 GB are listed as skipped. **Show in folder** reveals only the export just made.
  - *Delete all data*: typed confirmation (“delete”, checked by the backend too) → computer use stops, start at login off, running replies stop, every chat's sandbox (container + volume) removed, extensions uninstalled (their keychain settings with them), Chrome pairings forgotten, MCP servers and browsers shut down, keychain entries removed (provider keys, Brave/Tavily keys, MCP sign-ins incl. split ones), the database closed, then the data, config and log folders emptied. What can't be removed yet is listed in `.oxide-delete-pending.json` and removed at the next start before anything opens (only paths inside Oxide's folders). The window clears its localStorage; Oxide restarts 1.5 s later.
  - Zip writer: `oxide_mcp::archive::ZipWriter` streams entries to a file (deflate when smaller, DOS timestamps, Zip64 end records and offsets past 4 GB); round-trips through Oxide's reader, Python's `zipfile` and `unzip -t`.
- **Capabilities and CSP** (`src-tauri/capabilities/*.json`, `src/acl.rs`, `build.rs`): `build.rs` reads `collect_commands!` and declares the commands to Tauri (`AppManifest`), which turns on the per-window check. Main window: every command plus only the core APIs the shell uses (event listen/unlisten, drag, minimize, toggle/internal maximize, is-maximized, close, set theme) — `core:default` is gone. Quick entry: `list_models`, `start_conversation`, `hide_quick_entry`, `open_quick_entry_chat`, `capture_screenshot`, listen, drag, theme. Control bar: `computer_activity`, `computer_stop_all`, listen, theme. No remote origins, so artifact frames (opaque origin; their own CSP has `connect-src 'none'`) get no IPC. CSP: `script-src 'self'`, `worker-src 'none'`, `object-src 'none'`, `base-uri 'self'`, `form-action 'none'`; the unused `asset:` source is gone. Tests: the main capability lists exactly the registered commands, the small windows exactly theirs, no `:default` sets or plugin permissions, and the CSP's directives.
- **CI** (`.github/workflows/ci.yml`): fmt, prettier, clippy `-D warnings`, cargo test, typecheck, lint, Vitest on windows-latest, macos-latest and ubuntu-24.04 (uv sidecar fetched, extension built first). **Release** (`release.yml`): on a `v*` tag or by hand; tauri-action on Windows x64, macOS universal and Ubuntu 24.04 → a draft release with installers, signatures and `latest.json`; then the extension zip is attached (and kept as a workflow artifact). Optional Apple signing secrets are passed only when set. `docs/RELEASING.md`: setup, secrets, releasing, testing on clean VMs, signing.
- **Tests**: core (update settings default, status JSON), mcp (zip writer round trip, Zip64, DOS dates), store (`dump_tables`, `data_tables`, `all_conversation_ids`), agent (`exports_tables_files_and_a_redacted_config`, refused destinations, secrets removed, wipe + pending leftovers, config inside data, redaction shapes, folder sizes, and `delete_all_removes_chats_files_settings_and_keys` end to end with a real store, MCP manager, extensions, bridge, sandboxes and browser agents), src-tauri (`acl.rs`: commands parsed, capabilities exact, CSP). Vitest `features/data/Data.test.tsx` (locations, export + cancel, delete with the typed word, update check/install/failure/not configured/switch, the background toast once) against `src/test/fakeData.ts`.

Deviations from PLAN.md (and why):
- **The update URL is set per build, not in `tauri.conf.json`**: there's no GitHub repository yet, and the release workflow knows it (`github.repository`). Local builds don't look for updates.
- **The update signing key has no password** (made with `tauri signer generate --ci`); it lives in the user profile and the GitHub secret.
- **Windows and macOS code signing aren't set up** (no certificates); installers are unsigned (SmartScreen) and the Mac app ad hoc signed (Gatekeeper's right-click → Open). The workflow signs and notarizes when Apple secrets are added.
- **Delete all restarts Oxide** (`app.restart()`) instead of wiping in place: every subsystem holds open handles; what a running process keeps is removed at the next start.
- **Export leaves out caches** (managed Python, sandbox workspaces, browser profiles, extensions' server files) and every secret.
- The capability JSON files stay hand-written (with a test) rather than generated, so a reviewer sees each window's rights.

Known issues / TODO carried forward:
- **Linux keychain**: `keyring`'s `linux-native` (kernel keyutils) forgets keys at reboot; Phase 18 should move Linux to the Secret Service (noted in PLAN.md).
- Linux packages need Ubuntu 24.04+ (PipeWire 1.x for xcap's screen capture); built on ubuntu-24.04 since CI run #2.
- Installers aren't code-signed; the `dev.oxide.app` identifier ends in `.app` (Tauri warns; changing it moves data and keychain entries).
- The `.deb`/`.rpm` don't update themselves (only check); AppImage, Windows and macOS do.
- WebView2's localStorage folder (`%LOCALAPPDATA%\dev.oxide.app\EBWebView`) isn't deleted by “Delete all” (the window clears its own storage instead).
- `npm run bindings` regenerates `src/bindings/index.ts` (edited by hand for this phase). *Resolved in Phase 18: regenerated.*

Notes for Phase 18:
- Still to do from Phase 17: the MacBook check of the `.dmg` (steps in this chat's notes: right-click → Open / Privacy & Security → Open Anyway, or `xattr -dr com.apple.quarantine`) and the Linux packages (`scripts\win\test-linux.cmd` with Docker Desktop; files in `release-test\`).
- To release: `node scripts/set-version.mjs X.Y.Z`, checks, commit, push, tag `vX.Y.Z` (as `scripts\win\phase17-fix.ps1` does), then publish the draft.
- IPC review: start from `src-tauri/capabilities/*.json` and `src/acl.rs`; commands still trust their arguments (paths are checked inside the commands).
- Add Windows code signing when a certificate exists (`docs/RELEASING.md`).

**Windows run (2026-11-14 14:14–14:30):** every step green on the first run (bindings, extension, uv pins for all six targets, fmt, prettier, clippy, cargo test, typecheck, lint, Vitest, `tauri build` 689 s → `Oxide_0.1.0_x64-setup.exe` 24.5 MB and `Oxide_0.1.0_x64_en-US.msi` 33.8 MB with `.sig` files); committed `82d6fe3`. The installed copy was checked by hand by the user (all fine). Tauri warns that the identifier `dev.oxide.app` ends in `.app` (a macOS bundle-extension clash); changing it would move the data folder and keychain entries, so it stays for now.

**GitHub:** private repo `sitefactorydk-collab/oxide`, `main` pushed, secret `TAURI_SIGNING_PRIVATE_KEY` added. **CI run #1** (`82d6fe3`): macOS green; Ubuntu 22.04 failed compiling `libspa` 0.10.1 (xcap → pipewire) against Ubuntu 22.04's PipeWire 0.3.48 (`spa_meta_first` missing etc.); Windows failed one flaky test (`computer_use_asks_for_the_chat_then_each_app_and_acts_in_order` hit the chat tests' 5 s event wait on the slow runner; 4.6 s locally). Fix (`scripts\win\phase17-ci.cmd`, message `logs\commit-msg-phase17-ci.txt`): Linux CI and release on **ubuntu-24.04** (package names `libayatana-appindicator3-dev`, `libfuse2t64`), so Linux packages need Ubuntu 24.04+; `actions/checkout`/`setup-node` v5; the chat tests wait up to 30 s.

Verified (agent side): fmt ✅ clippy ✅ cargo tests ✅ (Linux: all library crates; new: core 2, mcp 3, store 1, agent 8) acl tests ✅ tsc ✅ eslint ✅ prettier ✅. Windows: ✅ (`phase17.cmd`, `82d6fe3`). CI: ✅ all three systems (run #2, `ff775fc`, 20 m 48 s). **Release #1** (tag `v0.1.0`, `ff775fc`, run 43606281940, finished ~16:20): green; draft release “Oxide v0.1.0” holds 17 assets — `Oxide_0.1.0_x64-setup.exe`, `.msi`, `Oxide_0.1.0_universal.dmg`, `Oxide_0.1.0_amd64.AppImage` (108 MB), `.deb` (36.5 MB), `Oxide-0.1.0-1.x86_64.rpm` (36.5 MB), `.sig` files, `latest.json`, `oxide-chrome-extension-0.1.0.zip`. Test helpers added (uncommitted): `scripts\win\test-linux.cmd` (Docker Desktop: installs the .deb on ubuntu:24.04, the .rpm on fedora:41 and the AppImage, starts Oxide under Xvfb, screenshot + “started” check), `scripts\win\test-windows-sandbox.wsb` (Windows Sandbox with `release-test\` mapped read-only); `release-test/` is git-ignored. The user chose to test **Windows only for now** (MacBook and Docker/Linux tests later). VM exit check: Windows pending; macOS and Linux deferred.

**Windows Sandbox test (16:45–):** v0.1.0 installs and starts in Windows Sandbox (SmartScreen “Run anyway”). Local servers on the host (Strata, LM Studio at 127.0.0.1) can't be reached from inside the Sandbox — expected; use a cloud key or a `netsh portproxy` on the Sandbox's gateway address. **Bug found:** a Groq endpoint (OpenAI kind, `https://api.groq.com/openai/v1`) tested “Connected. 11 models” but the picker listed none: the OpenAI kind kept only ids in OpenAI's curated table. Fix (`scripts\win\phase17-fix.cmd`, message `logs\commit-msg-phase17-fix.txt`): when a listing has no OpenAI ids, chat-looking models (`looks_like_chat`: no embed/whisper/tts/transcribe/image/moderation/guard/rerank) are listed and probed for tool support once each, like Strata's; test `other_openai_compatible_services_are_listed_and_probed`. The script raises the version to **0.1.1** (`scripts/set-version.mjs`), runs the checks, commits, pushes and tags `v0.1.1`, so the release also tests the updater (publish v0.1.0 first). v0.1.0 was published (17:14). The Sandbox's update check then failed with “Could not fetch a valid release JSON”: the repository was private, and GitHub serves release files of private repos only to signed-in users. The user made `sitefactorydk-collab/oxide` **public** (17:20); the check now says “Version 0.1.0 is the newest.” (`docs/RELEASING.md` notes that updates need a public repo, or a public releases repo.) **v0.1.1** (`3f61eed`): CI #3 green (13 m 28 s), Release #2 green (33 m 31 s); published as Latest with all 17 assets (17:43 check). **Update test (17:46, checked by the user):** in Windows Sandbox, the installed 0.1.0 found 0.1.1 (Settings → About), downloaded, installed and restarted as 0.1.1; Groq's models then listed (tool support probed) and chatted. All Windows checks passed.

## Phase 16 — Computer use — 2026-11-13
**Windows run 1 (13:47):** everything built on Windows the first time (bindings, cargo test, typecheck, lint, vite build green). Failed: clippy `chunks_exact_to_as_chunks` (Rust 1.99 on Windows), one Vitest test (the control bar read its first, empty activity after the test's event), and the real-computer test: on the user's 4K screen at 250 % the test process wasn't DPI aware, so the pointer read 1536×864 instead of 1920×1080. Fixed: `Desktop::new()` makes the process per-monitor DPI aware itself (`SetProcessDpiAwarenessContext`, harmless when Tauri already did), `as_chunks`, the test waits for the first read. Run 2 started 2026-11-13 ~14:05; the desktop link dropped before its logs could be read.

**Windows run 2 (14:04):** all green (Vitest 241/241, the real-computer test passed with physical pixels on the 4K screen); committed `898553d phase 16: computer use`.

**First exit try (14:16):** the chat ran on Strata `swift-iq2_xs`, which never got the computer tool: Strata's `/v1/models` doesn't say which models see images, so Oxide marked every Strata model as text-only (computer use needs a model that sees images) and the model fell back to the sandbox and a shared folder. The user's model does see images. Follow-up (`scripts\win\phase16-fix.cmd`, message `logs\commit-msg-phase16-fix.txt`): **Settings → Models has an “Images / No images” switch per model** (`ModelSettings.images: [ModelImagesOverride { model, images }]`, applied by `ModelRegistry` to listings, `find` and `models_of`, so chats, attachments and computer use follow it; × resets it); the “+” menu's **Use the computer** says when the chat's model can't see images; Settings → Computer use says which models work. Tests: `models_can_be_marked_as_seeing_images`, `models_marked_as_seeing_images_in_settings_see_images`, Vitest “marks a model as seeing images, and resets it”. (Anthropic had an API incident that afternoon, so Claude couldn't be used for the check meanwhile.)

**Second exit try (20:27, after marking `swift-iq2_xs` as seeing images; follow-up committed 21:16, all green):** computer use worked — Run dialog (Win+R), Notepad opened and maximized, a new tab, “Hello from Oxide” typed, Ctrl+S — but saving failed: the model typed `C:\Users\Mikkel\Documents\oxide-test.txt` (guessed from the profile name; the Windows user folder is `info`), then wandered in the dialog until the user stopped it. Clicks landed where aimed (maximize, text area, file name field, Save). It also first sent `"key"` instead of `"text"` for a key press. Follow-up 2 (`scripts\win\phase16-fix2.cmd`, message `logs\commit-msg-phase16-fix2.txt`): the computer-use paragraph of the system prompt lists the user's **Home, Desktop, Documents and Downloads** folders (Tauri's known-folder paths, OneDrive moves included; `ComputerUse::with_folders`), and `"key"` is accepted in place of `"text"` for `key`/`hold_key`.

**Third exit try (21:40, after follow-up 2 `9046b25`, all green):** the model opened Start with the Windows key and typed “notepad”, then clicked a web suggestion in the search panel, which opened Chrome; the user stopped it (Stop worked: “computer use stopped in 1 chat(s)”). In this and the second try the model said its first screenshot still showed Oxide's window (couldn't be checked: the screenshots are in app data). Follow-up 3 (`scripts\win\phase16-fix3.cmd`, message `logs\commit-msg-phase16-fix3.txt`): the computer-use prompt says to open apps with Win+R and the program name or the Windows key, the name and Enter, never to click the search panel's web results, to type full paths into Save/Open dialogs and to prefer keyboard shortcuts; `src-tauri` logs “computer use: minimized Oxide's window” (info) so the next run shows whether hiding happened.

**Fourth exit try (21:55, after follow-up 3 `6afc299`):** Win+R → notepad → Enter, text typed, Ctrl+S — all by keyboard, as steered — then the reply hit the 25-step limit inside the Save dialog. The screenshots (checked from `dev.oxide.app\attachments`) show why: the model's clicks land about 55 px **above** what it aims at (File name box at y≈478, it clicked y=425 and selected a folder; the address bar at y≈240, it clicked y=184 on the desktop), so its typed path went into the file list. Oxide's mapping is right (its zoom of the requested region shows exactly that region; the reported app under each click matches the screenshot). That's `swift-iq2_xs`'s visual grounding, not Oxide; the File name box was already selected after Ctrl+S, so typing the path + Enter would have worked. Oxide was already minimized for every action (“already out of the way” in `dev.log`), so the earlier “Oxide is on screen” remarks were the model's mistake.

**Follow-up 4 (requested by the user: “calibrate” the model's clicks; `scripts\win\phase16-fix4.cmd`, message `logs\commit-msg-phase16-fix4.txt`):** Settings → Computer use → **Models** lists every model that sees images, each with **Calibrate** and a **Grid** switch.
- *Calibrate* (`ChatService::calibrate_pointer`, command `computer_calibrate`): two test images the size of the model's screenshots (a plain screen with a window, a taskbar and six coloured dots, `computer::calibration_image`, the grid drawn if it's on for that model) go to the model with a request for each dot's centre as JSON; `PointerCalibration::fit` fits `reported = true × scale + offset` per axis by least squares (≥ 4 dots; scale 0.5–2, offset ≤ 300 px; a second fit without answers far off the first — a dot taken for another), records the typical miss before/after, and saves it in `ComputerSettings.models[] { model, grid, calibration }`. `Frame::with_tuning` applies it in `to_screen` (clicks, drags, scrolls), `region_on_screen` (zoom) and `to_model` (the pointer position, reported in the model's terms) — only when measured at the same screenshot size and grid setting.
- *Grid*: faint red lines every 100 px with 3×5-pixel digit labels along the top and left (`computer::draw_grid`), on that model's screenshots; the system prompt tells it to read positions from them.
- *What a click reached*: `ComputerControl::element_at` (UI Automation `ElementFromPoint`, name + localized type, at most 1.5 s) is read before each click; results say “Done: clicked “File name:” (edit) in Notepad.” instead of “Done (Notepad).”
- Tests: core `calibrations_fit_correct_and_apply_only_where_measured` (fit, outlier dropped, refusals, per-model/size/grid, frame mapping); agent `clicks_say_what_they_reached_and_models_get_their_grid_and_correction`, `calibration_images_have_their_dots_where_they_say`, `calibrating_measures_how_a_model_points_and_saves_the_correction` (fake model 55 px high with one wild answer → offset −55, 11 of 12; a model that finds nothing fails plainly); Vitest “calibrates a model, switches its grid and resets it, and shows failures”.

**Fifth try (22:34, calibrated: “Points 36 px too far right, stretched −32 % across: corrected (about 180 → 2 px off) · 12 of 12 dots found”):** during the run, the text “notepad” + Enter arrived in the user's Claude desktop app chat (the Run dialog apparently didn't get focus); an approval card had come up just before (22:34:52), which app it named is unknown. The user then closed the phase: the local model's accuracy isn't good enough for the task, and that's accepted.

**Closed** by the user (22:51) with the exit criteria not fully met (see Status). To do when computer use is next touched:
- Run the PLAN exit check with Claude (Notepad save + Ctrl+Alt+Esc while typing).
- Find out whether keys can reach an app the chat wasn't allowed to use: the type/key check reads the foreground app *before* acting; if focus moves between the check and the keystrokes (a Run dialog that doesn't open), they land elsewhere. Consider re-checking the foreground app right before each typed piece and refusing when it changed.
- Bigger help for weak models: numbered marks on UI Automation elements (“click 12”) instead of pixel coordinates.

Status: done with known gaps (closed by the user) — built and green on the agent side (Linux: core, llm, store, agent tests; tsc, eslint, prettier on the device). Windows checks, the real-computer test and the commit run in `scripts\win\phase16.cmd`; the exit check is by hand. Started in one session (paused at the usage limit with `PHASE16-WIP.md`, removed by the phase script) and finished in a second.

Exit criteria (by hand, on Windows):
- [ ] **The model opens Notepad, types text and saves a file.** Settings → **Computer use** → switch on “Let models use the computer” (the panic shortcut card must not show an error). New chat with a model that sees images and has tools (Claude gets Anthropic's own computer tool; LM Studio/OpenAI models the `computer` function): “Open Notepad, type ‘Hello from Oxide’ and save it as oxide-test.txt in Documents.” → card **Let Oxide control your computer in this chat?** (screen named, no Allow once) → **Allow for this chat** → Oxide minimizes; the bar “Oxide is controlling your computer · …” with **Stop** shows at the top of the screen; the first action reaching an app shows **Allow Oxide to use Windows (taskbar, Start and search)?** / **Notepad?** cards (Oxide comes back for each) → allow → Notepad opens, the text is typed, Ctrl+S, the file name, Enter. The file exists; the bar goes and Oxide comes back when the reply ends. The thread shows each action (“Clicked at (…)”, “Typed “…””, “Pressed ctrl+s”) with a small screenshot; **Used in this chat** has a **Computer** row.
- [ ] **The panic shortcut stops it immediately.** Ask for something long (“type the numbers 1 to 500 in Notepad, one per line”) → while it types, press **Ctrl+Alt+Esc** → typing stops within a moment, the reply ends as stopped, the bar goes, Oxide comes back. Same with the bar's **Stop**.
- [ ] Safety spot checks: a chat asked to type into a password box (e.g. Windows' “Run as different user” or any sign-in form) refuses; clicking on Oxide's own window is refused.

Done:
- **`oxide-core` (`computer.rs`)**: `ComputerAction` (Anthropic's action set: screenshot, zoom, clicks, drag, mouse move/down/up, cursor position, scroll, type, key, hold key, wait) parsed from the tool input with errors written for models; key names and combos (`ctrl+shift+s`, `Return`, `Page_Down`, F1–F20); screenshot geometry (`screenshot_size`: at most 1366×1024 and ~1.15 MP, never larger than the screen; `Frame` maps screenshot pixels ↔ screen pixels, multi-monitor offsets); apps (`app_of`: lower-case exe file names as ids; Explorer/taskbar/Start/search grouped as “Windows (taskbar, Start and search)”; `ApplicationFrameHost` and other hosts named by window title), `blocked_app` (Oxide itself, password managers, Windows' credential, UAC and sign-in screens), `sensitive_field` (UI Automation's IsPassword, or names/ids/classes/help text about passwords, PINs, one-time codes, card numbers, CVC, IBAN), `sensitive_text` (Luhn-valid card numbers, IBANs with a valid checksum); `ComputerSettings` (off by default; screen; minimize Oxide while acting; Claude's own tool; apps every chat may use); approval, activity and status types.
- **`oxide-llm`**: `ChatRequest.computer` (`ComputerTool { flavor, width, height, display }`); `anthropic_computer_flavor`: the newest Claude models take `computer_toolset_20260801` (no beta header), Opus 4.5–5.1-generation models `computer_20251124` (+ beta), older ones `computer_20250124` (+ beta), unknown ids the toolset. Toolset member calls decode to `computer {action, …}` and go back in the history as members with `toolset_name`.
- **`oxide-agent` (`computer.rs`, `tools/computer.rs`, `chat.rs`, `context.rs`)**: `ComputerControl` (the screen and input) and `ComputerSink` (bar, Oxide's window); `ComputerUse` keeps a session per chat, maps coordinates, finds the app an action reaches (the window under the point for mouse actions, the foreground app for keys and typing) and asks the chat service when the chat may not use it yet, refuses blocked apps and sensitive fields/text, types in 20-character pieces so Stop is immediate, waits 0.7 s after an action and returns a JPEG screenshot (only the newest 3 stay in a request); `stop_all` (panic) and `end` (reply over: held button released, bar told). Chat service: computer use is offered when Settings → Computer use is on, the model sees images and the chat's switch is on; Claude gets Anthropic's tool (unless switched off in Settings), other models the `computer` function; the **session card** comes first in every chat (also in scheduled runs set to run without asking), then an **app card** for each app at action time (Allow once / Allow for this chat; grants `builtin/computer` and `computer-app/<id>`); computer calls run one after another in the model's order, and after one fails the rest of that turn's are “Not executed”; computer use ends with the reply and when a chat is deleted. A system-prompt paragraph explains the screen size, approvals, the panic shortcut and the never-typed rules.
- **`oxide-computer` (`desktop.rs`)**: `Desktop` — screens and capture with xcap (screen ids are Windows' `\\.\DISPLAYn`, names the monitors' friendly names), input with enigo 0.6.1 (buttons, keys — letters and digits as their virtual keys so Shift only goes down when asked —, text, wheel), moves with `SetCursorPos` (enigo's absolute moves only map the main screen), the app at a point (`WindowFromPoint` → root window → process path with `QueryFullProcessImageNameW`, the program's own description from its version info, the window title), the foreground app, and the focused control from UI Automation (on its own thread, at most 1.5 s). Windows only for now: elsewhere `Desktop::supported()` is false and the app doesn't offer computer use. Ignored test `tests/real_computer.rs` (screens, a non-blank capture, pointer moves on every screen, the app in front).
- **`src-tauri` (`computer.rs`, `commands/computer.rs`, `desktop/shortcut.rs`)**: `DesktopControl` (oxide-computer behind the agent's trait), `TauriComputerSink`: the **control bar** (`index.html?window=computer`, created hidden at startup, always on top, not focusable so Stop leaves the app in front, `content_protected` so it isn't in screenshots, top centre of the screen in Settings), minimizes Oxide's window while a chat acts (it comes back for approval cards and when nothing acts), event `computerChanged`. **Panic shortcut Ctrl+Alt+Esc** registered while computer use is on (quick entry's shortcut no longer uses `unregister_all`, and refuses the panic shortcut); it and the bar's Stop call `computer::stop_all` (every session's actions stop, then `ChatService::stop` on their chats — scheduled runs included — and Oxide comes back). Commands `computer_status`, `computer_activity`, `computer_stop_all`.
- **Frontend (`features/computer/`)**: Settings → **Computer use** (switch, panic shortcut card with its error, “a chat is controlling the computer” + Stop, screen picker, minimize Oxide, Claude's own tool, apps every chat may use with Remove; a note on systems without support); approval cards (`ComputerApprovalCard`: the session card lists what it means, Deny / Allow for this chat; the app card has Allow once / Allow for this chat and “Always allow <app> in every chat”); the control bar page; “+” menu **Use the computer** switch; scheduled task dialog switch; tool lines (“Clicking at (683, 384)” / “Clicked …”, “Typed “…””, a small screenshot under each); **Computer** row in Used in this chat; grant labels. Vitest `features/computer/Computer.test.tsx` (against `src/test/fakeComputer.ts`).
- **Tests (Rust)**: core (parsing, keys, geometry, apps, blocked apps, sensitive fields and text, settings), llm (flavors, request shapes and beta headers, toolset decoding and history), agent (`computer::tests`: clicks land and come back with a screenshot; keys, drags, scrolls and the pointer; password fields and card numbers never typed; only the newest screenshots stay; stop ends typing at once; chat tests: the session card then each app's and actions in order; a denied start halts the rest of the turn; scheduled runs set to run without asking still ask; Claude gets its own tool).
- **Dev builds**: `image` and `zune-jpeg` build with `opt-level = 3` in the dev profile (resizing and encoding a screenshot took ~2 s unoptimized, in tests and `cargo tauri dev`).

Deviations from PLAN.md (and why):
- **Windows only for now.** Input, the per-app check and the field check need platform APIs; Windows is the primary machine. macOS/Linux builds still compile `oxide-computer` (xcap only) and don't offer computer use. Recorded in PLAN.md's decisions table.
- **One `computer` function for other models** (Anthropic's input shape, with `action`), not one function per action: the same executor, labels and approvals serve both, and models handle it well.
- **Two kinds of approval** instead of an allowlist prompt only: the chat's start (never “Allow once”, never skipped by scheduled runs) and each app at action time; apps can be allowed for every chat from their card, and removed in Settings.
- **Oxide minimizes itself while a chat acts** (Settings switch, on by default), so the model sees what's behind it.

Known issues / TODO carried forward:
- `npm run bindings` regenerates `src/bindings/index.ts` (edited by hand for this phase).
- Key combos with characters other than letters and digits (`ctrl+plus`) go through the keyboard layout and may not work on every layout.
- UI Automation that doesn't answer within 1.5 s counts as “not a sensitive field” (logged).
- Elevated (administrator) windows ignore input from Oxide (Windows' UIPI); models get a screenshot showing nothing changed.

Notes for Phase 17:
- The control bar is a third window label (`computer`) in `capabilities/default.json`; audit it with the others.
- macOS/Linux computer use needs: enigo on those platforms, the app at a point (AX / X11), the focused field (AX / AT-SPI), and screen-recording/accessibility permission prompts on macOS.

Verified (agent side): fmt ✅ clippy ✅ cargo tests ✅ (Linux: core 95, llm 51, store 52, agent 176 + 2 ignored) tsc ✅ eslint ✅ prettier ✅. Windows: pending (`phase16.cmd`).

## Phase 15 — Scheduled tasks — 2026-11-13
Status: done. Committed `68ee955 phase 15: scheduled tasks` on the second run of `scripts\win\phase15.cmd` (2026-11-13 09:52: bindings, fmt, prettier, clippy, cargo test, typecheck, lint, Vitest 234/234, vite build all green). Run 1 (09:36) failed only two new Vitest tests that added a task before the page's event listener was ready; they now wait for the page first. All exit checks passed by hand on 2026-11-13 (11:13).

**Follow-up (requested by the user after the main exit check):** deleting a chat a scheduled run wrote now also removes that run from the task's history (it used to stay, with its chat link cleared). `Store::delete_conversation` deletes the run's `task_runs` row in the same transaction (migration 0010 keeps its `ON DELETE SET NULL`, now unused, because an applied migration can't change); a run whose chat is deleted while it goes ends quietly (no entry, no notification); deleting a chat refreshes the Scheduled pages. Tests: `runs_keep_history_and_go_with_their_chat`, `deleting_a_runs_chat_stops_the_run_and_removes_it_from_the_history`, Vitest “drops a run from the history when its chat is deleted”. Committed `d7c34cd` (2026-11-13 10:29, all green, Vitest 235/235); checked by hand by the user.

Exit criteria (by hand, on Windows; all checked by the user on 2026-11-13):
- [x] **A daily task runs on time from the tray** (the PLAN exit; checked by the user 2026-11-13 10:10, two on-time runs with output). Sidebar → **Scheduled** → **New task**: name “Copenhagen weather”, prompt “Search the web for today's weather in Copenhagen and give me a three-line summary.”, **Every day** at a time 2–3 minutes from now, a tools model, Web search on, approvals **Run them without asking**. The dialog shows “Next: <today>, HH:MM”. Close the window with ✕ (Oxide goes to the tray). At that time: a notification titled “Copenhagen weather” with the reply's first words. Tray → **Scheduled tasks** → the task's **History** shows *Done* · *On schedule* and the output; **Open chat** opens the run's chat (named after the task, with the “Scheduled run of …” banner); the task now says “next <tomorrow>, HH:MM”.
- [x] **Ask mode pauses and notifies.** A task with approvals **Ask me** that needs an asking tool (e.g. a shared folder + “write note.txt saying hi”) → **Run now**, then put Oxide in the background → notification “… waiting for your approval”; History shows *Needs your approval* with **Open the chat to approve** → approve the card → *Done*.
- [x] **Pause, edit, delete.** The switch on the list pauses (“Paused”) and resumes; Edit changes the schedule (e.g. Every week → Friday) and the description follows; ⋯ → Delete asks, then removes the task (its runs' chats stay in Recents).
- [x] **Missed runs.** Tray → Quit Oxide; wait past a task's time; start Oxide → a task set to **Run once when Oxide is back** runs once (*Made up for a missed run*); one set to **Skip it** shows *Missed*.

Done:
- **`oxide-core` (`schedule.rs`):** `TaskSchedule` (`cron { expr }` five-field cron or `@hourly/@daily/@weekly/@monthly/@yearly`, read in local time; `once { at }`), `CronSpec` (lists, ranges, steps, month/weekday names, 7 = Sunday, day-of-month OR day-of-week like cron; refuses dates that never come) with `next_after(after, zone)`: wall-clock times through a `LocalZone` (seconds east of UTC at a moment) — a time in the spring-forward gap runs an hour later, a time that happens twice in autumn runs once (the first). `FixedZone` for tests. `TaskInput` (`clean()`: trims, limits 100/20,000 characters, validates the schedule, dedupes folders), `ScheduledTask` (+ `nextRunAt`, `lastRun`), `ApprovalMode` (`ask` | `auto`), `MissedRunPolicy` (`runOnce` | `skip`), `TaskRun` (`trigger` schedule | manual | catchUp, `scheduledFor`, `status` running | waitingApproval | done | failed | stopped | missed | skipped, `output`, `error`), `ScheduleChange`. Limits: 200 tasks, 100 runs kept per task, 4,000 characters of output, a run > 5 minutes late is missed. `DesktopAction::OpenScheduled { taskId }` and `oxide://scheduled[/<id>]` links.
- **`oxide-store`:** migration `0010_scheduled_tasks.sql` (`scheduled_tasks` with JSON schedule/folders/tools; `task_runs`, cascading with the task; deleting a chat deletes its run, see the follow-up); `list_tasks`, `get_task`, `create_task`, `update_task`, `set_task_schedule_state`, `delete_task`, `due_tasks`, `earliest_next_run`, `add_task_run`, `update_task_run`, `task_runs`, `active_task_runs`, `task_run_for_chat`, `prune_task_runs`.
- **`oxide-agent` (`scheduler.rs`):** `Scheduler` + `SchedulerSink` (`changed`, `chat_started`, `run_event`). One loop: sleeps until the earliest next run (at most 60 s, so sleep/wake and clock changes are noticed), `tick(now)` moves each due task to its next time first (a one-time task switches off), then runs it — or, if more than 5 minutes late and the task skips missed runs, records *Missed*; late runs of `runOnce` tasks run once (*catchUp*), however many were missed. At start, runs Oxide was closed during become *Failed* (“Oxide was closed while this run was going.”). A run = `start_conversation` with the task's prompt, model, project, folders and tool switches, renamed to the task's name, then `ChatService::reply`; the scheduler follows the chat's signals (approval → *Needs your approval* + notification, approved call running → *Running*, Done/Failed → outcome; a 15 s check reads the chat if a signal was missed). Output = the reply's text, keeping its end (the report). A run while the previous one still goes is *Skipped*; deleting a run's chat stops it and removes it from the history (follow-up); deleting a task stops its running run. `create`/`update`/`set_enabled`/`delete`/`run_now`/`runs`/`preview` (next 3 times).
- **`oxide-agent` (`chat.rs`, `context.rs`):** `ChatService::set_unattended/clear_unattended/is_unattended` (in memory, only while a run goes): the system prompt says it's a scheduled run nobody watches (no questions, finish, end with a report); the chat keeps the task's name (no automatic title); with `auto` every tool call runs without a card except `save_skill`, and no grants are saved (if the user carries on in the chat, it asks as usual). `ChatService::subscribe()`: a broadcast of ToolApproval / ToolRunning / Done / Failed for the scheduler (sent after the window gets the event).
- **`src-tauri`:** `zone.rs` (`SystemZone` on `chrono::Local`, DST included), `AppState.scheduler` (started in `setup`), commands `list_scheduled_tasks`, `get_scheduled_task`, `create_scheduled_task`, `update_scheduled_task` (folders resolved like a chat's), `set_scheduled_task_enabled`, `delete_scheduled_task`, `run_scheduled_task`, `scheduled_task_runs`, `preview_schedule`, `task_run_for_chat`; event `scheduleChanged`; `TauriScheduleSink` (a run's new chat goes out as `conversationUpdated`); notifications for runs that need approval, finish (the reply's first words) or fail, while Oxide isn't in front — the generic reply notifications skip run chats; tray item **Scheduled tasks**.
- **Frontend (`features/scheduled/`):** sidebar **Scheduled** (`/scheduled`, `/scheduled/:id`); list with schedule in words, next run, last run's status, **Run now** and a pause switch, plus a note when closing the window quits Oxide or it doesn't start at sign-in (link to Settings → Desktop); task page (Run now, Edit, ⋯ Pause/Resume/Delete with a confirm, details, **History** with status, trigger, time, error, output with Show all, **Open chat** / **Open the chat to approve**); `TaskDialog` (name, prompt, Once / Every hour / Every day / Weekdays / Every week / Every month / Custom cron, live “Next: …” preview or the schedule's error, model, project, folders, tool switches — *Save skills* off by default for new tasks —, extended thinking, approvals, missed runs); `ScheduledRunBanner` above a run's chat; `useScheduleEvents` (refreshes, adds a run's chat to Recents, toasts while in front); `openScheduled` desktop action. Vitest `features/scheduled/Scheduled.test.tsx` (against `src/test/fakeSchedule.ts`) and `schedule.test.ts`.
- **Tests (Rust):** core: cron fields/names/steps/macros, refusals, DST gap and overlap on a Copenhagen-like zone, once, upcoming, input cleaning, JSON shapes, `oxide://scheduled` links. Store: round trip, due tasks, history kept when the chat is deleted, pruning, cascade. Agent: `a_daily_task_runs_on_time_in_a_new_chat_and_keeps_its_output`, `ask_mode_waits_for_approval_and_auto_mode_does_not`, `missed_runs_follow_the_task_policy`, `runs_oxide_closed_during_are_marked_failed_at_start`, `one_time_tasks_switch_off_after_their_run`, `pausing_editing_and_deleting`, `a_run_is_skipped_while_the_previous_one_is_still_going`, `deleting_a_runs_chat_stops_the_run_and_keeps_its_history`, `a_run_whose_model_is_gone_fails_with_the_reason`, `the_loop_runs_a_task_when_its_time_comes` (real clock, 1.5 s), output tail, scheduled-run system prompt; the command futures are `Send`.

Deviations from PLAN.md (and why):
- **Missed runs are configured per task** (run once / skip), not app-wide: a news brief wants making up, a “remind me at 9” doesn't.
- **“Run without asking” is per run, in memory:** it applies only while the scheduled run goes (not to the user's follow-ups in that chat), saves no grants, and never covers `save_skill`.
- **Local time comes from `chrono` in `src-tauri`** (already in the dependency graph through Tauri); the library crates take a `LocalZone`, so nothing new had to enter the offline vendor bundle.
- **Notifications only while Oxide isn't in front** (like replies); in front, a toast. The user's idea of phone push notifications isn't part of this phase.
- **Run chats are ordinary chats** in Recents (no separate sidebar section); the banner and the task's history link them.

Known issues / TODO carried forward:
- Tasks run only while Oxide runs (open or in the tray). The Scheduled page says so when closing the window quits Oxide or it doesn't start at sign-in.
- Wall-clock cron: an hourly task skips one hour on the autumn DST change (the repeated hour runs once).
- A run with approvals “Ask me” waits until the user answers (no timeout); the next scheduled time is then *Skipped*.
- `npm run bindings` regenerates `src/bindings/index.ts` (edited by hand for this phase).

Notes for Phase 16:
- `ApprovalMode::Auto` skips every approval except `save_skill`; computer use must not run unattended — add its tools to the always-ask list in `ChatService::needs_approval` (next to `SaveSkill`).
- The scheduler's sink is how background work reaches notifications; computer use's panic hotkey should stop scheduled runs too (`ChatService::stop` on their chats; `Scheduler::is_run_chat`).

Verified (agent side): fmt ✅ clippy ✅ cargo tests ✅ (Linux: core 85, store 52, agent 162 + 2 ignored; the two sample-file tests need `docs\samples`, which the Linux mirror leaves out) tsc ✅ eslint ✅ prettier ✅. Windows: fmt ✅ clippy ✅ tests ✅ (cargo + Vitest) typecheck ✅ lint ✅ vite build ✅ (phase15.cmd). tauri dev on Windows ✅ (manual exit checks by the user).

## Phase 14c — Models create and edit skills — 2026-11-13
Status: done. Committed `b9d8faf phase 14c: models create and edit skills` on the first run of `scripts\win\phase14c.cmd` (2026-11-13 08:42: bindings, fmt, prettier, clippy, cargo test, typecheck, lint, Vitest, vite build all green). All exit checks passed by hand on 2026-11-13 (08:54).

Exit criteria (by hand, on Windows; all checked by the user on 2026-11-13):
- [x] **Turn it into a skill.** In a chat with a tools model, do something small (e.g. “write a haiku about Copenhagen in November, then a one-line English gloss under it”), then “turn what we just did into a skill”. Expect an approval card **“Save a new skill <name>?”** with the whole SKILL.md open, any extra files, and only **Deny** / **Allow once** (no “Allow always”). Nothing is in the skills folder before you click. After **Allow once**: the line reads **Created skill <name>**, with **Open <name> in Settings → Skills** (opens that skill's editor); Settings → Skills lists it; “Used in this session” shows it under Skills.
- [x] **A new chat uses it.** New chat: “haiku about the harbour, please” → the model calls `load_skill` with the new skill and follows it.
- [x] **A later change edits the same skill.** “Change that skill: always add a title line” (in either chat) → a card **“Change the skill <name>?”** with a red/green diff of SKILL.md (and the whole new SKILL.md folded below); Allow once → **Updated skill <name>**; Settings → Skills still has one skill of that name, with the change.
- [x] Optional: “+” menu → **Save skills** off → the model no longer has `save_skill` in that chat. A skill added from a folder or zip shows “You added this skill from a folder…” on its change card.

Done:
- **`oxide-core`:** `SAVE_SKILL` (`save_skill`); `SkillSavePreview { name, folder, isNew, skillMd, diff, files: [SkillFileChange { path, bytes, isNew, content, diff }], notes }`; `ToolApprovalRequest.skill: Option<SkillSavePreview>`.
- **`oxide-agent` (`skills.rs`):**
  - `Skills::plan_save(input)` works out exactly what a call writes, without writing: `skill` (an existing skill's folder or name; omitted = new), `name`, `description`, `instructions` (or `body`), `files: [{path, content}]`. A change keeps what isn't passed (description, instructions, license); a new skill needs all three. Checks as in Settings (name slug, description ≤ 1,024, instructions), names unique (a new skill with a taken name is refused with “call save_skill again with "skill": "<folder>"”; renaming onto another skill too), SKILL.md ≤ 200,000 characters, ≤ 20 files per call, ≤ 256 KB each, the skill ≤ 50 MB / 2,000 files. Paths: relative only, `\` → `/`, no `..`, no drive letters, no hidden parts, ≤ 8 levels, not SKILL.md, no duplicates; nothing on the way may be a link (checked again when writing), a folder where a file goes or a non-text file to replace.
  - Notes for the card: added from a folder / a zip (now recorded: `origins` in `.oxide-skills.json` — `settings`, `chat`, `folder`, `zip`; older skills with a license or extra files get “may have been added from a folder or zip”), switched off (stays off), renamed.
  - `unified_diff` (LCS, 3 lines of context, `@@ -a,b +c,d @@` hunks; very large files fall back to all-removed/all-added) for SKILL.md and replaced files.
  - `Skills::apply_save(plan)` writes the plan (a new skill whose folder appeared meanwhile, or a changed one that was removed, is refused; a failed new skill is cleaned up).
  - `save_skill_entry(skills)`: the tool, its description listing the user's skills (folder, “switched off”, description; up to 100) or “no skills yet”.
- **`oxide-agent` (tools, chat loop, prompt):** `Builtin::SaveSkill` (usage kind Skill; label “Created <slug>” / “Updated <skill>”). Offered whenever the skills folder exists and the chat hasn't switched `save_skill` off (also with no skills yet); `load_skill` stays as before. In a step, a `save_skill` call is planned first: a plan that fails returns its error at once, without a card. It **always asks**, whatever was granted before; “Allow always” is answered as Allow once and saved nowhere (also in `answer_approval`); the card carries the preview; Allow writes exactly the previewed SKILL.md and files. The result says what was saved and that new replies are offered it. System prompt (when the tool is on): how to write a skill from what just worked, offer when work looks repeatable but save only when asked or agreed, load and edit instead of duplicating.
- **Frontend:** `features/extensions/SkillSavePreview.tsx` (notes, the SKILL.md diff with added/removed lines coloured, the whole SKILL.md — open for a new skill, folded for a change — and each extra file with its content or diff); the approval card for `save_skill` says “Save a new skill <name>?” / “Change the skill <name>?” and has no “Allow always”; tool lines “Creating / Created skill”, “Updating / Updated skill” with **Open <folder> in Settings → Skills** (`useSkillFocus` opens that skill's editor); the skills list refreshes after a save; “+” menu **Save skills** switch (`disabledBuiltins: ["save_skill"]`). Vitest `features/extensions/SaveSkill.test.tsx`.
- **Tests (Rust):** `a_model_saves_a_skill_after_approval_and_the_next_reply_uses_it` (an earlier grant doesn't skip the card, nothing written before Allow, “Allow always” saves no grant, the file is exactly the preview, the next reply's prompt and tools list it), `changing_a_skill_shows_a_diff_and_never_makes_a_second_one` (diff, a duplicate refused without a card, deny changes nothing, allow changes the same skill), `save_skill_can_be_switched_off_per_chat`; unit tests for confinement (`..`, absolute, drive letters, hidden, SKILL.md, size, count, duplicates), links out of the folder, keeping unpassed fields, unique names, notes and origins, diffs.

Deviations from PLAN.md (and why):
- **No separate `list_skills` tool:** `save_skill`'s own description lists the user's skills (folders, on/off, descriptions), and the system prompt already lists the usable ones for `load_skill`.
- **The “link to it” opens the skill's editor in Settings → Skills** (there's no per-skill page).
- **The card's approval is per save**: “Allow always” isn't shown, and an `allowChat` answer that arrives anyway counts as once.
- Where an older skill came from isn't known (origins are recorded from now on); the card guesses from a license or extra files and says “may have been”.

Known issues / TODO carried forward:
- A switched-off skill can be changed by `save_skill`, but `load_skill` can't read it first (the model passes the whole new text).
- Scripts saved in a skill are reference only (Oxide doesn't run skill scripts; `load_skill` says so).

Notes for Phase 15:
- A scheduled run that tries `save_skill` always waits for approval (it never runs unattended).

Verified (agent side): fmt ✅ clippy ✅ cargo tests ✅ (Linux: core 75, agent 151 + 2 ignored, all crates green) tsc ✅ eslint ✅ prettier ✅. Windows: fmt ✅ clippy ✅ tests ✅ (cargo + Vitest) typecheck ✅ lint ✅ vite build ✅ (phase14c.cmd). tauri dev on Windows ✅ (manual exit checks by the user).

## Phase 14 follow-ups (web research testing) — 2026-11-13
The user made a `web-research` skill in Oxide (Chrome, then Browser, then Web search; Google first, DuckDuckGo fallback; 2–3 sources for quick questions; open results by clicking their [n], never guessed addresses; Sources list; new tabs only, closed when done) and tested it with “whats the latest news about the Nvidia scandal?” on `swift-iq2_xs`. Fixes:
- **Page readers saw nothing inside `display: contents`** (aa5a4f2): `checkVisibility()` is false for such elements, and Google wraps all its results in one, so `chrome__read_page` (extension/src/content.ts) and the Browser reader (python/oxide_browser_agent/pagetext.py) read Google results pages as empty. Such elements now let their children decide. Checked on live Google: 0 → 8 result titles.
- **Task list closes when a reply ends normally** (051884a): small models skip the last `task_list` call, leaving “In this chat” stuck. When a reply that used `task_list` ends with EndTurn, `finish_task_list` marks every step done; Stop or failure leaves it as is. Verified by the user.
- Skill wording (not code) fixed: using `web_fetch` while Chrome was available, going too deep (14 steps), and guessing article addresses. Test 4: one search, two clicked sources, tab closed.
- Possible later: show link addresses in `chrome__read_page`; hide `web_search`/`web_fetch`/`browser__*` when a paired Chrome is connected; make the `web_fetch` failure hint and the browser prompt point to Chrome when it's on.

## Planning — Phase 14c added — 2026-11-12
After Phase 14 the user asked whether Oxide's models can create or edit skills: they can't (only `load_skill`; skills are made by hand in Settings → Skills). **Phase 14c (Models create and edit skills)** was added to PLAN.md: a `save_skill` tool that always asks with the whole `SKILL.md` (and a diff for edits), files confined to the skill's folder, existing skills listed so models edit instead of duplicating. Exit: “turn what we just did into a skill” saves a skill a new chat uses, and a later change edits the same skill.

## Phase 14 — Chrome extension & WebSocket bridge — 2026-11-12
Status: committed `176a672 phase 14: Chrome extension & WebSocket bridge` on the second run of `scripts\win\phase14.cmd` (2026-11-12 22:10: bindings, extension build, fmt, prettier, clippy, cargo test, typecheck, lint, Vitest, vite build all green). **The real-extension test passed on Windows** (`scripts\win\phase14-chrome.cmd`, 22:22): Microsoft Edge 157 loaded `extension\dist` from the command line, paired through the popup and ran every tool against the local test site. All manual exit checks passed by hand on 2026-11-12 (22:57), including the optional unpair.

Windows run 1 (22:02, not committed): clippy `too_many_arguments` on `AppState::open` (8 with the Chrome sink; allowed with a reason), and the Vitest pairing check read “Valid for 6 more minutes” — the countdown counted from when the page opened, not from the new code (fixed: it resets per code). The informational real-Chrome step picked a template `python.exe` from Python's `Lib\venv`; the scripts now take `Scripts\python.exe` next to the test environment's `pyvenv.cfg`. Its first real run (after the commit) paired with Edge and then failed only on the test's own assertion that the browser's name contains “Chrome” (fixed: any “… on …” name). `scripts\win\phase14-chrome.cmd` runs that test alone (commits nothing).

What the phase script does (`scripts\win\phase14.ps1`): stops a running dev app; `npm run bindings`; `npm run extension` (builds `extension\dist`, the folder to load unpacked); `check.ps1`; `vite build`; commits with `logs\commit-msg-phase14.txt` only if all of that passed. Then, informational only (never blocks the commit): when Phase 13's test Python (`target\browser-test-python`, with Playwright) is there, the real-Chrome test with Playwright's Chromium, else Edge, else Chrome (`logs\phase14-chrome*.log`; branded Chrome/Edge may refuse `--load-extension`). Then starts the app.

**No permission prompts once paired** (requested by the user after the exit checks; `scripts\win\phase14-fix.cmd`, message `logs\commit-msg-phase14-fix.txt`): the site prompt in the tab (“Let Oxide use duckduckgo.com?”) and Oxide's approval cards for Chrome are now both **off by default** — pairing is the permission. The extension's popup has **Ask before Oxide uses a new site** (off; `askSites` in `chrome.storage.local`); blocked sites stay blocked. Settings → Chrome's approvals switch defaults to off (`ChromeSettings.ask = false`; settings saved earlier keep their value, so switch it off once). Password, one-time-code and card fields are still refused (a rule, not a question). The real-Chrome test no longer allows the test site first, so it proves no prompt appears. Committed `67b59b6` (23:05, all green). **After rebuilding the extension, reload it** (the round arrow on its card in `chrome://extensions`): the popup reads its files from disk each time it opens, but Chrome keeps the old service worker running — the first try still showed the prompt (and an unchecked box) until the user reloaded; then no prompt.

Exit criteria (by hand, on Windows; all checked by the user on 2026-11-12):
- [x] **Pairing.** Settings → **Chrome**: switch on **Let models use Chrome** → the card says “Not paired yet”, listening on 127.0.0.1:47820. In Chrome: `chrome://extensions` → Developer mode → **Load unpacked** → `C:\AI Apps\Oxide\extension\dist` (Settings shows the folder; **Open the folder**) → pin the Oxide icon. In Oxide click **Pair a browser**, type the six words into the extension's popup → popup “Connected to Oxide”, Oxide “Connected: Chrome … on Windows”, listed under Paired browsers.
- [x] **Open a tab, read, click, type, screenshot.** New chat with a tools model (Chrome on): “In Chrome, open example.com, read it, click the link on it, then take a screenshot.” Expect: an approval card for `navigate` (“opens the page in your own Chrome…”), a new tab in an orange **Oxide** tab group, the site prompt in the tab (**Allow for this chat** / **Always allow**), the orange frame and the “Oxide is controlling this tab · Stop” pill, the read page in the tool result, the click's new page, and the screenshot image. Then e.g. “In Chrome, search duckduckgo.com for oxide rust” (type + Enter).
- [x] **Stop.** While a Chrome reply runs, press **Stop** in the tab (or **Stop Oxide** in the popup): the reply stops in Oxide; the next reply may use Chrome again.
- [x] **Unpaired or foreign-origin connections are rejected** (tested: `oxide-bridge` tests over real sockets — no/other origins, `moz-extension://`, upper-case ids, a non-local `Host` (DNS rebinding), plain HTTP, a made-up token, the right token from another extension id, a wrong token, tool calls or garbage before pairing, silence past 10 s, wrong codes (5 tries), expired, used and cancelled codes, an old protocol). By hand, optionally: **Forget** in Settings → Chrome → the popup says “Not paired”.

Done:
- **`oxide-core` (`chrome.rs`):** `CHROME_SERVER`/`CHROME_SWITCH` (`chrome`), the 14 tool names, `chrome_tool_reads_only` (tabs_list, tabs_select, read_page, find, scroll, screenshot, get_console never ask), `ChromeSettings { enabled (off), port 47820, ask (off since the follow-up) }` in `AppSettings.chrome`, `ChromeStatus`, `PairedBrowser`, `PairingCode`, `clean_pairing_code`. `ToolUsageKind::Chrome` (stored as `"chrome"`).
- **`oxide-bridge`:**
  - `ws.rs`: RFC 6455 by hand (server handshake with `sha1` + base64, frame codec: masking required from clients, fragments joined, control frames, 7/16/64-bit lengths, 32 MB limit, UTF-8 checked). Unit tests incl. the RFC's accept-key example.
  - `lib.rs` — `Bridge`: `configure` (listen on `127.0.0.1:<port>`, move, stop; a taken port is reported for users), upgrade only for `chrome-extension://<32 a–p>` origins with `Host` `127.0.0.1|localhost|[::1]:<port>`; first message within 10 s must be `pair {code}` or `hello {browserId, token}` (`protocol: 1`), else a JSON-RPC error and close `4001`/`1008`. Pairing: `start_pairing` (6 words from 256 → 48 bits, 5 minutes, 5 wrong tries, single use), token = 256 OS-random bits base64url, saved in the keychain as `chrome-token-<id>` and bound to the extension id; the paired list (no secrets) in `<data>\chrome.json`. Constant-time compares. A newer connection of the same browser replaces the old (`4002`); `forget` closes it (`4003`) and deletes the token. Heartbeat: Oxide pings every 20 s, drops a connection silent for 60 s; answers the extension's `ping`. `call(session, tool, args)` → `tools/call`, MCP-shaped result → `McpToolResult` (180 s limit); `release(session)` → `session/release`; `session/stopped` from the extension → `BridgeSink::stopped` (stopped chats are remembered so `release` reaches the extension). `testing` feature: a stand-in extension client (`TestClient`, `fake_extension`).
  - Tests (`src/tests.rs`, real sockets): refusals above, pairing and reconnecting, persistence across a restart, forgetting, wrong/expired/cancelled codes, calls from two chats at once, errors, ping, stop and release, disconnects mid-call, a silent extension dropped and a live one kept, settings moving and stopping the listener and a taken port.
  - `tests/real_chrome.rs` (ignored): the built extension in a real Chrome (`extension/tests/drive_chrome.py` drives the popup with Playwright: pairs by typing the code, allows the test site, blocks another, presses Stop): navigate, find, click (by text and a link opening a new tab), back, type + submit, form_input (select, checkbox), password refused, a stale ref, scroll, press_key, get_console (messages from the page's load, and filtered), screenshot (JPEG), tabs_list/create/close, a blocked site, Stop reaching Oxide and release re-enabling, banner up and down, Forget reaching the popup. **Passed in the cloud container** (Chrome for Testing 153), twice in a row.
- **`extension/`** (MV3, TypeScript built with `tsc` by `npm run extension` into `extension/dist`; `README.md` has install, safety, the protocol and Chrome Web Store notes):
  - `background.ts` (module service worker): the link (pair/hello, reconnect with backoff 1→30 s, JSON-RPC ping every 20 s, `chrome.alarms` every 30 s to wake and reconnect), sessions per chat in `chrome.storage.session` (the chat's tab, the tabs it opened, sites allowed for the chat, stopped), the 14 tools, per-site permission (asks in the tab; choices in `chrome.storage.local`), a tab group “Oxide”, links opening a new tab followed, `back`/`forward` through the page's own history (`chrome.tabs.goBack` skips entries made without a user gesture), Google search for words, only http(s), `chrome://` and the Web Store refused, Stop handling, badge.
  - `content.ts` (isolated world, injected on demand): page text with numbered elements (ported from Phase 13's `pagetext.py`, plus `<label>` wrapping and `aria-labelledby` names), find, click (scrolled into view, refused when covered/disabled; by ref, text or screenshot pixels), type (native value setter + input/change; contenteditable), keys (Enter submits, Tab moves focus, PageUp/Down/Home/End scroll), scroll, form_input (select/checkbox/radio/value), console buffer; password/one-time-code/card fields refused; banner (orange frame + pill with Stop) and site prompt in closed shadow roots, trusted clicks only.
  - `console-hook.ts` (page's MAIN world): wraps console methods + errors; buffered until the content script says hello. Registered at `document_start` (`chrome.scripting.registerContentScripts`) only for the sites a working chat is on, unregistered when no chat works.
  - `popup.html/ts/css`: pairing (code + port), state, Reconnect/Unpair, “Working now” with **Stop Oxide**, site choices (allow/block, remove, add).
  - Icons from Oxide's own logo (`src-tauri/icons`).
- **`oxide-agent`:** `tools/chrome.rs` (the 14 entries as `chrome__<tool>`, labels), `ToolKind::Chrome { tool }` (grant key `builtin/chrome__<tool>`, usage kind Chrome), `Tools::with_chrome`. Offered when Settings → Chrome is on, the chat hasn't switched it off and a paired Chrome is connected. Approvals: acting tools ask while `chrome.ask`; looking never asks. `read_page`'s `max_chars` held to the result cap. **Chrome calls of one model turn run strictly in order** (each waits for the earlier ones, also while one waits for approval), so a read sees the page an earlier navigate opened. The reply's end releases the chat's session (banners down); deleting a chat too. System prompt paragraph. Tests: tools follow setting/connection/chat switch, a chat's calls reach the extension with the chat as session (approval, cap, image, usage log, release), Chrome going away is explained.
- **`src-tauri`:** `AppState.chrome` (opened with the keychain and `<data>\chrome.json`, configured at start and on `save_settings`, shut down on quit), `commands/chrome.rs` (`chrome_status`, `chrome_start_pairing`, `chrome_cancel_pairing`, `chrome_forget`, `chrome_open_extension_folder`), event `ChromeChanged`; `TauriChromeSink` stops the chat's reply when Stop is pressed in Chrome. The extension folder shown is `<exe dir>\extension` (packaged, Phase 17) or the repo's `extension\dist`.
- **Frontend:** `features/chrome/` (`queries.ts`, `ChromeSection.tsx`: Settings → **Chrome** with the connection card, **Pair a browser** and the code with Copy/Cancel and minutes left, install steps with the folder and **Open the folder**, paired browsers with Forget, port, approvals), Settings section “Chrome”, app-wide `chromeChanged` listener, “+” menu **Chrome** switch, Connectors **chrome (Built in)** card, tool lines (“Opened in Chrome”, “Clicked in Chrome [3]”…), approval card text, “Used in this session” **Chrome** row, grants “Chrome: navigate”. Vitest `features/chrome/Chrome.test.tsx` against `src/test/fakeChrome.ts`.

Deviations from PLAN.md (and why):
- **No `tokio-tungstenite`:** the bridge speaks WebSocket through a small RFC 6455 implementation of its own (`oxide-bridge/src/ws.rs`, ~300 lines with tests). The crate isn't in the offline vendor bundle the agent builds with, and the bridge needs only the server handshake, text frames, ping/pong and close; everything is tested here, including against real Chrome.
- **The extension builds with `tsc`, not Vite:** the service worker and popup are ES modules and the content scripts plain scripts, so no bundler is needed; `tsc` also runs on the Linux side (Vite needs Windows-native binaries here). Chrome API types are a small hand-written `extension/src/chrome.d.ts` (no `@types/chrome` offline).
- **Clicks, typing and keys are synthetic DOM events** (no `chrome.debugger`, which shows a “being debugged” bar); README lists the limits. Only the top frame is read and acted on.
- **Pairing is the permission:** Oxide's approval cards for Chrome and the extension's per-site prompt both exist but are off by default (the user's choice after the exit checks); blocked sites and the password/payment-field rule always apply.
- **Chats' Chrome calls run in order** within a turn (other tools keep running in parallel).
- **`tabs_close` closes only tabs Oxide opened for the chat**; `tabs_select` lets the chat work in one of the user's tabs.

Known issues / TODO carried forward:
- The real-extension test uses Edge on Windows (Edge 157 accepts `--load-extension`); branded Chrome 137+ may ignore it, so Chrome itself is checked by hand.
- Pairings are bound to the extension's id; an unpacked extension loaded from another folder gets another id and must pair again.
- While a chat works on a site, DevTools shows `console-hook.js` as the source of that site's console lines (README says so).
- Oxide's own Stop ends the reply; an action already running in Chrome finishes first, then the reply's end takes the banner down.
- The Web Store build isn't made; Phase 17 adds the zip to CI and the extension folder to the installers.

Notes for Phase 15:
- Scheduled runs can use Chrome only while a paired Chrome is connected; a run with approvals set to “ask” pauses on Chrome's approval cards like any other tool.
- In the cloud container: build with `node scripts/build-extension.mjs` (TypeScript from the device's `node_modules`, staged as `logs\p14-node-tools.tgz`), then `OXIDE_TEST_CHROME=/usr/bin/google-chrome cargo test -p oxide-bridge --test real_chrome -- --ignored` (Chrome for Testing loads unpacked extensions; Playwright 1.63 drives the popup).

Verified: fmt ✅ clippy ✅ tests ✅ (cargo + Vitest) typecheck ✅ lint ✅ vite build ✅ (Windows, phase14.cmd) real extension ✅ (Chrome for Testing 153 in the cloud container; Edge 157 on Windows). tauri dev on Windows ✅ (manual exit checks by the user).

## Phase 13b — Context management & chat compaction — 2026-11-12
Status: done. Committed `d58672b`, then `fccd2c7` (repetition guard, gentler clearing shown in the transcript) and `2c31e21` (Working / Compacting indicators); every check green on Windows; all exit checks passed by hand on 2026-11-12 (see below). Follow-up the user wants to try later: send the current reply's thinking back to Strata (`reasoning_content`) so notes in thinking survive clearing.

**Windows run 1 (2026-11-12 18:08):** every check green, committed as `d58672b phase 13b: context management & chat compaction`. `logs\phase13b-models.json`: Strata's `/v1/models` names no context window (only `id`, `meta.label` “Swift 1.5 IQ2_XS”), so its window must be set in Settings → Models (131072).

**First real badminton run (export `docs\can you make me a spread sheet…test-2.json`, not committed):** no context error, but (1) the model used `web_fetch` (Wikipedia) and never the browser, though it was on — its choice, the fetches worked; it ignored the 404 hint to use `browser__search`; (2) it checked for openpyxl with `sandbox_exec` (the sandbox image has none and no network); (3) the meter went from ~80K to ~35K tokens with nothing in the transcript (clearing all but the last 2 result turns at once); (4) after that its thinking, which isn't sent back to Strata, held the only copy of the facts it had read, and the next step's thinking turned into “No, 2017 Worlds MS champion:” ×3,228 (108K characters, 650 s) until the user stopped it. Fix (`scripts\win\phase13b-fix.cmd`, message `logs\commit-msg-phase13b-fix.txt`):
- **Repetition guard** (`compaction::is_repeating`): every 1,000 characters of text or thinking, the last 6,000 are checked; under 10 % distinct 32-character pieces (prose ≈ 0.95, fetched tables ≈ 0.3, that loop 0.005) the stream is dropped, the block cut where the loop began with “[Oxide cut this off here: …]”, and the step retried with a note to use the conversation or look things up again instead of recalling; after 2 retries the reply ends with a plain message. Tests: `a_model_repeating_itself_is_cut_off_and_nudged`, `a_model_that_keeps_repeating_ends_the_reply_plainly`, unit test on prose/table/loop.
- **Gentler clearing:** oldest result turns first, one at a time, only until the estimate is 15 % of the window under the trigger (`Budget::clear_goal`; 131K window: 98K → 79K), never the last 2 result turns.
- **Clearing is shown:** each clearing round is saved as a `cleared` compaction (never used as a summary; `pick_compaction` skips it) and shown as an **“Old tool results cleared”** note at that point of the reply (tokens before → after; click for the list of cleared calls).
- System prompt: facts go in the visible reply text (thinking isn't kept between steps); never fill gaps from memory.

**Windows run 2 (19:19):** all green, committed `fccd2c7 phase 13b: stop repetition loops, clear gently and show it`. **Second real badminton run** (export `docs\can you make me a spread sheet…test-3.json`): no context error, no repetition loop; it used `browser__search` (Google showed no usable results in the hidden browser) and `web_fetch`; clearing ran twice (98.1K → 72.5K, 96.6K → 67.7K tokens; “Old tool results cleared” notes shown); Strata's monitor stayed below 100 %. The user stopped it after ~45 min (102 blocks, 1.16 M input tokens over its requests) while it was still researching: the model keeps its notes in thinking (not sent back) and re-reads pages it had read (World Tour Finals 3×). A model-quality limit on IQ2_XS, not a context failure. UI fix after it (`phase13b-fix.cmd` again, message `logs\commit-msg-phase13b-fix.txt`): the live reply shows **Working** after a clearing or compaction note, and **Compacting the conversation** while a summary is made (they showed nothing before).
- Possible later improvement: send the current reply's thinking back to OpenAI-compatible servers as `reasoning_content` (if Strata accepts it), so notes in thinking survive clearing.

Agent-side checks passed: Rust fmt, clippy `-D warnings` and tests on Linux (core 72, store 49, llm 48, agent 134 + 2 ignored, mcp, pyenv, sandbox unchanged); tsc, eslint and prettier on the device VM. Vitest and `src-tauri` run only on Windows; `src/bindings/index.ts` was edited by hand and `phase13b.ps1` regenerates it.

What the phase script does (`scripts\win\phase13b.ps1`): stops a running dev app; `npm run bindings`; `check.ps1`; `vite build`; saves what Strata (`/v1/models`) and LM Studio (`/api/v0/models`, `/v1/models`) report into `logs\phase13b-models.json` (informational: shows whether Strata names a context window); commits with `logs\commit-msg-phase13b.txt` only if all of that passed; starts the app.

Exit criteria (by hand, on Windows):
- [x] **The badminton reply** (second real run, test-3: no context error, Strata below 100 %, two clearing notes; stopped by the user while still researching). Settings → Models: check `swift-iq2_xs` under Strata. If it says “Context: unknown”, click it and set **131072** (the window Strata's own error named). New chat on Strata `swift-iq2_xs` with web fetch / the browser on, same as on 2026-11-12: “can you make me a spread sheet with all the greatest male badminton players over the last 10 years?”. Expect: the reply finishes (no “context window” error); Strata's monitor stays below 100%; the composer's meter (ring left of the model picker) stays under the window; once it passes ~75%, old `web_fetch` results show up cleared in the requests (the meter's popover says “N old tool results were cleared”), and if that isn't enough a **Conversation compacted** divider appears in the reply (click it to read the summary).
- [x] **A 50-turn chat keeps working past the window** (tested in code: `a_fifty_turn_chat_keeps_working_past_the_window`). By hand, optionally: a small window (set e.g. 8192 for a local model in Settings → Models), then many turns: dividers appear and replies keep coming.
- [x] **Compact now** (checked 20:26): the meter's popover → Compact now → a divider after the last message; the meter drops; the next reply starts from the summary.
- [x] Settings → Models → **Long chats** (checked 20:26): the switch, the threshold, the compaction model and the Anthropic switch save; a model's window can be set and reset in the model list.

Done:
- **`oxide-core` (`context.rs`):** `ContextSettings { compaction (on), threshold 75 % (30–95), compactionModel, anthropicClearing (on), windows: [ContextWindowOverride { model, tokens }] }` in `AppSettings.context`; `Compaction { id, conversationId, messageId, afterToolUseId, summary, trigger (auto | manual | overflow), tokensBefore, tokensAfter, model, createdAt }`; `ContextUsage { tokens, window, windowSource (provider | setting | learned | default), threshold, compacted, clearedResults }`. New `StreamEvent`s: `context { usage }` (before every request), `compacting`, `compacted { compaction }`.
- **`oxide-store`:** migration `0009_compactions.sql` (`compactions`, cascade with the chat; `message_id` has no foreign key because a reply compacted while it runs is saved after its compaction); `add_compaction`, `compactions`.
- **`oxide-llm`:** `LlmError::is_context_full()` and `context_numbers()` (prompt size and window out of Strata's, OpenAI's, llama.cpp's, LM Studio's and Anthropic's wording; Strata's 2026-11-12 message is a test case). OpenAI-compatible listings read the window from `loaded_context_length` (LM Studio, preferred over `max_context_length`), `context_length`, `context_window`, `max_model_len`, `n_ctx`, `meta.n_ctx`. `ChatRequest.clear_tool_results` → Anthropic `context_management` with `clear_tool_uses_20250919` (trigger = Oxide's threshold, keep 3, `web_search` excluded) and the `context-management-2025-06-27` beta header.
- **`oxide-agent` (`compaction.rs` + the tool loop in `chat.rs`):**
  - Budget: window from Settings' override, then a window learned from a “context full” answer (in memory), then the listing, then a default per provider (Anthropic 200K, OpenAI 128K, local 32K); reserve = the request's output limit or ⅛ of the window (≤ 8K, ≤ ¼ of the window); trigger = threshold % of the window, below window − reserve.
  - Estimates: characters / 4 (images 1,600, documents by size), system prompt + tools counted; multiplied by a per-chat factor = the provider's reported prompt tokens / Oxide's estimate of the previous request (0.5–4).
  - Before every request above the trigger: (1) results of all but the last 2 result turns become `[Result cleared to save context: web_fetch https://… (from character 40000) · result of 40k characters …]` (calls stay; sticky for the rest of the reply; skipped on Anthropic when its own clearing is on); (2) still above: the oldest part is summarized — inside a reply, the history plus all steps but the last 2 (the compaction anchors to the last tool result before the cut), otherwise the history including the request being answered. The summary (headings: Task, Progress, Facts found with sources, Next steps) gets the user's latest request word for word, replaces the covered part as one user turn, is saved and emitted. Long transcripts are summarized in parts (rolling summary) sized to the compaction model's window; tool results are shortened to 4,000 characters for it.
  - Later turns: the latest compaction on the thread is used (`history_for`), so a summary is made once.
  - “Context full” (pre-stream error, stream error before any output, or Anthropic's `model_context_window_exceeded` before output): learn the window and the real prompt size, clear down to 1 result turn and compact (trigger `overflow`), retry once; a second time the reply fails with a plain message. Also when automatic compaction is off.
  - Tool results are cut to ~10 % of the window (`window × 0.4` characters, 2,000–100,000); `web_fetch` and `browser__read_page` get `max_chars` held to that cap so their “read on from …” notes stay right.
  - System prompt (compaction on and tools offered): old results may be cleared, so note the facts you need (with sources) as you go instead of re-reading pages.
  - `ChatService::context_usage`, `compactions`, `compact_now` (marks the chat busy while it runs).
  - Tests (fake provider with a small window): clearing in a long tool loop (every request under the limit, notes for old results, caps), a long request compacted once and reused next turn, compaction inside a reply and the next turn resuming after the anchor, context-full retry (window learned), a second context-full failing plainly, a 50-turn chat past the window, Compact now + the meter's numbers. Unit tests for budgets, clearing, caps, anchors, stored summaries, transcripts and chunks, and the error parser.
- **`src-tauri`:** commands `chat_context(conversationId, leafId)`, `chat_compactions`, `compact_chat(conversationId, leafId)`.
- **Frontend:** `features/context/` — `ContextMeter` (ring + % left of the model picker; popover with tokens / window, where the window came from with **Set the real window** for a default, the threshold, compacted / cleared notes, **Compact now**), `CompactionDivider` (“Conversation compacted”, why, tokens before → after, the summary on click) after the covered message or after the step a reply was cut at, live in a running reply too; `ContextSettingsFields` (Settings → Models → **Long chats**) and `WindowField` (each model's window in the model list, click to set / reset). Stream store: `context`, `compacting`, `compaction` blocks. Vitest `features/context/Context.test.tsx`, `streams.test.ts`.

Deviations from PLAN.md (and why):
- **Anthropic: server-side clearing only, not server-side compaction.** Compaction (`compact_20260112`) returns compaction blocks that must be sent back on later requests, which needs a new content block type end to end; Oxide's own compaction runs on Anthropic as everywhere else. Context editing (`clear_tool_uses_20250919`) is on by default; a proxy that refuses `context_management` makes the reply continue without it.
- **The window learned from a “context full” answer and the calibration factor live in memory** (per app run), not in the database. A wrong window can be fixed for good in Settings → Models.
- **The “compacted” marker and Compact now live in the composer's context meter popover**, not as separate toolbar buttons.
- **Summaries made while answering keep the user's request word for word** (first 4,000 characters), so a compacted reply never loses what was asked.
- `docs/can you make me a spread sheet…context-fill.json` (the 2026-11-12 export, 4.5 MB of fetched web text) stays local: `.gitignore` leaves it out.

Known issues / TODO carried forward:
- Whether Strata reports a context window in `/v1/models` is unknown (see `logs\phase13b-models.json` after the run). Without one, Strata models use the 32K default until set in Settings → Models or until a “context full” answer names it.
- Estimates are rough until the first answer reports its prompt tokens; the meter says so.
- Cleared results lose their facts unless the model wrote them down; the system prompt asks it to.
- Export (Markdown/JSON) doesn't include compactions.

Notes for Phase 14:
- Results of the extension's tools go through the same caps and clearing (they are tool results like any other).

Verified: fmt ✅ clippy ✅ tests ✅ (cargo + Vitest) typecheck ✅ lint ✅ vite build ✅ (Windows, phase13b.cmd / phase13b-fix.cmd). tauri dev on Windows ✅ (manual exit checks by the user).

## Planning — Phase 13b added — 2026-11-12
While testing Phase 13 the user asked a Strata `swift-iq2_xs` chat (128K context) “can you make me a spread sheet with all the greatest male badminton players over the last 10 years?”. The export (`can_you_make_me_a_spread_sheet_context-fill.json`) shows **one** reply with 35 tool calls: 2 `browser__search`, 31 `web_fetch` (3–40k characters each, many reading the same Wikipedia pages in slices), ≈ 410 KB of tool results in all; it ended with an error after 1,277,791 input tokens over its requests, and Strata's monitor showed the context at 100% (130k / 128K). Oxide has no compaction and no per-window limit on tool output, so a long agentic reply grows until the model's context is full. **Phase 13b (Context management & chat compaction)** was added to PLAN.md: per-model context budget, clearing old tool results and compacting inside a running reply and across turns (stored summaries), tool output caps scaled to the window, automatic recovery from “context full”, Anthropic's own context management where available, a context meter and a “compacted” divider in the UI. Exit check: that same request finishes on Strata without filling the context.

## Phase 13 — Browser agent (Browser-Use) — 2026-11-12
Status: done. Fix committed as `86f147d phase 13: a new chat's tool switches apply to its first reply` (2026-11-12 12:47, all checks green); the user rechecked the “+” menu switch in a new chat: it works. `1af53b2 phase 13: browser agent (Browser-Use)` on the first run of `scripts\win\phase13.cmd` (2026-11-12 12:11): uv lock, bindings, fmt, prettier, clippy, cargo test, typecheck, lint, Vitest, vite build, the real-browser exit check (Google Chrome found and used; **real Browser-Use 1.0.2 with Strata `swift-iq2_xs` found the test site's Saturday hours, 3 screenshots**) and the agent's Python tests on Windows all passed. Manual exit checks (2026-11-12): all passed except the “+” menu's Browser switch in a **new** chat — see the fix below.

Fix after the exit check (`scripts\win\phase13-fix.cmd`): switches chosen in a new chat's “+” menu were saved by a second call after `start_conversation` had already started the first reply, so that reply still had the browser (the same race hit web search, artifacts, the sandbox and connectors). They now travel in `StartConversation.tools` and are saved in the same transaction as the chat (like folders); the frontend sends the draft with the start. Tests: store `switches_chosen_before_the_chat_existed_are_saved_with_it`, Vitest “a new chat … Browser switch”.

Follow-up requested by the user after the exit check: **Google search instead of guessed addresses** — committed `09c1ca2 phase 13: Google search in the browser instead of guessed addresses` on the third run of `scripts\win\phase13-search.cmd` (the second failed only on a leftover assertion in `real_browser.rs`). The user tried the optional “My own Chrome” check: it works (signed out of Chrome, as advised), but many clicks timed out (`Locator.click: Timeout 10000ms exceeded`). Likely cause: Chrome hardly renders a background tab, so Playwright's wait for a stable element never ends. Fix committed as `23e3fd5 phase 13: browser clicks that time out are explained or done directly` (2026-11-12 14:22; the run before failed only on the flaky Phase 10 test `install_configure_toggle_update_uninstall` — os error 32 while replacing an extension folder on Windows, seen once before in Phase 12; passed on rerun; its retries were lengthened later, see AI Overview below): click/type bring the tab to the front in a visible browser; a click that only timed out waiting for stability is done directly (`element.click()`), while a covered element (cookie banner), a hidden or a disabled one is explained to the model instead (never clicked through); clicking by text uses visible matches only. Python test with a moving link and a covered button. Models often guessed URLs that don't exist.
- Browser agent: a `search(query)` tool (8 tools now; `BROWSER_SEARCH`), which searches Google in the browser (`hl=en`) and answers Google's EU cookie page (“Reject all”, else “Accept all”). `OXIDE_BROWSER_SEARCH_URL` (a URL with `{q}`) overrides the engine (tests). It's read-only for approvals (never asks): it doesn't act on websites. With **My own Chrome**, a tab the user keeps on the search engine (e.g. google.com) is used for searches, and it is never closed.
- **Checks for robots are never circumvented.** The first Windows run of `phase13-search.cmd` (13:08) failed: Google refused Oxide's hidden browser and the DuckDuckGo fallback showed a puzzle. A first fix (disguising the headless browser's user agent and hiding automation flags) was blocked as bot-check circumvention and dropped, as was hopping to other engines. Instead: a check for robots (Google's “sorry” page, “unusual traffic”, captcha) makes `search` fail with a plain message — with a visible browser (My own Chrome, or Show window), the model is told to ask the user to complete the check and search again; with the hidden browser, to use `web_search` or tell the user about My own Chrome. The system prompt says never to try to solve or get around such checks. Settings → Browser recommends **My own Chrome** for searching. The user asked whether Browser-Use could solve the checks: declined (that is circumvention, and against the sites' terms).
- `navigate`: search words instead of a URL are searched; an unknown site (DNS), a 404/410, or an error answer without a page is **searched for automatically** with words from the address (site + path words), with a note on top telling the model to open a real result; other errors (401/403/429/5xx) return the page with a note showing how to search. Retries a navigation that Chrome's error page interrupts.
- System prompt: “Whenever you don't know the exact address of the page you need, find it with browser__search first and open a real result; never guess URLs. When a page fails, search instead of trying made-up addresses.”
- A failed `web_fetch` gets a hint: search with `browser__search` (browser on) or `web_search`.
- UI: “Searching Google for / Searched Google for <query>”; the approval card for navigate mentions search words.
- Tests: Python (search words, refused searches, the user's Google tab in CDP mode, dead addresses with a local search engine, error page without body), agent (fetch hint, prompt), `real_browser` with `OXIDE_TEST_REAL_SEARCH=1` searches Google for real on Windows (real results must link natmus.dk; a plainly refused search also passes; a made-up natmus.dk page is reported, not asserted).
- **AI Overview** (requested by the user afterwards; `scripts\win\phase13-search.cmd` again, message in `logs\commit-msg-phase13-search.txt`): when Google's results have an “AI Overview”, `search` finds its section from the heading (`AI_OVERVIEW_JS` in `pagetext.py`; Google's markup has no stable names, so it's a heuristic: up from the heading until the section holds the answer, short of most of the page), clicks “Show more” when there is one, and puts the text first in the result, marked as an AI-written summary. The system prompt and the tool description say to use it when it answers the question, to say it comes from Google's AI Overview, and to open its sources when accuracy matters. Only reachable with a browser Google doesn't refuse (My own Chrome). Tested against a local stand-in page only. The first run (14:48) wasn't committed: the flaky Phase 10 test `install_configure_toggle_update_uninstall` failed again (os error 32, 2 of the last 3 Windows runs), though the user's manual AI Overview check passed. Committed as `4412e21 phase 13: Google's AI Overview leads browser search results` (2026-11-12 15:16, every check green). Fixed in the same commit: `rename_retry` in `oxide-mcp/src/extensions/mod.rs` now retries for about 20 s with growing waits (was 5 s), with unit tests.
- Notes for Phase 14: the extension works in the user's everyday Chrome (their profile and sign-ins), which is the natural place for searches; the user wants that.

Earlier agent-side notes: Agent-side checks passed: Rust fmt, clippy `-D warnings` and tests on Linux (core 70, store 47, llm 45, agent 119 + 2 ignored, mcp 46 + integration, pyenv 17 + 2 ignored, sandbox 10 + 3 ignored); **the ignored real-browser tests passed in the cloud container** against the real MCP Python SDK 2.3.0, Playwright 1.63 and Chrome 153, through Oxide's own MCP client (`real_browser`: navigate, read, click, a bad click, screenshot, live frames, close and restart, and `browser_task` with a stand-in for Browser-Use that drives the same browser over CDP; `browser_task_through_a_chat_uses_the_chats_model`: the chat's model reaches Browser-Use through a chat); the agent's Python unit tests (10) passed there too; tsc, eslint and prettier on the device VM. Vitest, `src-tauri` and the real Browser-Use run only on Windows.

What the phase script does (`scripts\win\phase13.ps1`):
1. Stops a running dev app.
2. `node scripts\fetch-uv.mjs`, then `uv lock --project python --python 3.12` (always: `python\pyproject.toml` has the new `browser` dependency group, so `python\uv.lock` changes; the commit includes it).
3. `npm run bindings` (replaces the hand-edited `src/bindings/index.ts`).
4. `check.ps1` (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest), then `vite build`.
5. The exit check: `cargo test -p oxide-pyenv --test real_browser -- --ignored --nocapture` with the real uv. It installs Python, the base packages and the browser group into `target\browser-test-python` (kept between runs, so a rerun is quick), uses Chrome or Edge (Edge is always on Windows; Chromium is downloaded only without one), and opens, reads, clicks and screenshots a local web site. **When Strata (127.0.0.1:47831) or LM Studio (127.0.0.1:1234) answers, it also runs `browser_task` with the server's first model** (Browser-Use for real: the test fails on import or API errors, not on a weak model's answer). Then the agent's Python unit tests in that environment, against Edge/Chrome.
6. Commits with `logs\commit-msg-phase13.txt` only if all of that passed, then starts the app.

Exit criteria (checked by hand by the user on 2026-11-12):
- [x] **Opening hours, with screenshots.** Settings → Browser: switch **Let models use a browser** on (the card says “Not set up yet” and “Chats use Microsoft Edge”). New chat with a tools model (an Anthropic or OpenAI model works best for `browser_task`; Strata is fine for the granular tools): “Find the opening hours of the National Museum of Denmark and summarize them.” Expect: an approval card (for `navigate` with the address, or `browser_task` with the goal); the first call sets the browser up (Settings → Browser shows the steps); the tool line shows a **live screenshot** while it runs; the answer summarizes the hours, and the screenshots stay under `browser_task` / `screenshot` calls.
- [x] Session panel → **Browser**: “Open”, the last screenshot and its address, **Close the browser** → “Closed”.
- [x] Settings → Connectors shows **browser (Built in)** with a switch (same as Settings → Browser); the “+” menu's **Browser** switch removes the tools from that chat (failed first in a new chat; fixed and rechecked).
- [x] Settings → Browser → **Download Chromium**: progress with a percentage, then “Downloaded”; choosing **Chromium downloaded by Oxide** makes new browsers use it; **Remove Chromium** removes it.
- [ ] Optional (not checked): **My own Chrome** — start Chrome with `--remote-debugging-port=9222 --user-data-dir=%LOCALAPPDATA%\OxideChrome`; the chat works in a new tab of that Chrome and Close leaves Chrome running.
- [x] After the idle time (Settings → Browser, default 15 min) and on quit, the chat's browser closes (no `msedge.exe` left with an `oxide-browser-…` profile).

Done:
- **`python/oxide_browser_agent/`** (MCP server on stdio, official `mcp` SDK 2.x `MCPServer`): tools `browser_task(goal, max_steps?)`, `navigate(url)`, `read_page(start?, max_chars?)`, `click(ref? | text?)`, `type(text, ref? | field?, submit?, clear?)`, `screenshot(full_page?)`, `close()`, with read-only annotations on `read_page` and `screenshot`. Results are `CallToolResult`s (text, JPEG images, `is_error` for failures the model should read). One browser action at a time.
  - `host.py`: the browser. **launch** mode starts Chrome/Edge/Chromium (`executable_path`) with Playwright's `launch_persistent_context` on a throwaway profile and a remote-debugging port on 127.0.0.1, so Browser-Use can drive the same browser over CDP; **cdp** mode connects to the user's running Chrome and works in a tab of its own (never closes theirs). Links that open new tabs are followed. Clear messages for a missing browser and an unreachable CDP address.
  - `pagetext.py`: the page as text — headings (`#`), lists, table cells, visible text, and numbered interactive elements (`[3] button "Go"`, `textbox "Search" (value="…")`, `checkbox (checked)`), skipping hidden elements, scripts and SVG, entering shadow roots. Numbers are `data-oxide-ref` attributes, given afresh on every read; a stale number says “read_page again”.
  - `agent.py`: `browser_task` runs Browser-Use 1.0 (`Agent(goal, llm=, browser=Browser(cdp_url=…, keep_alive=True))`, `use_vision=False` for models without images, `run(max_steps=)`, time limit) with `ChatAnthropic` for Anthropic (custom base URLs through `ANTHROPIC_BASE_URL`) and `ChatOpenAI(model, api_key, base_url)` for OpenAI, LM Studio and Strata. Every 1.5 s a JPEG of the page in front goes out as a progress notice (JSON: text, URL, frame), only when it changed. The result: Browser-Use's verdict and final result, its last errors, where the browser ended, and up to 3 screenshots (the last of each page). Without Browser-Use (or a model) it says to use the granular tools. Version differences (`TypeError`s) fall back to fewer arguments.
  - `python/tests/test_browser_agent.py`: unit tests against a real browser and a local web site (Browser-Use replaced by a stand-in).
- **`python/pyproject.toml`**: `[dependency-groups] browser = ["browser-use[anthropic]>=1.0.2,<1.1", "mcp>=2.3,<3", "playwright>=1.55"]` — installed only on first browser use (`uv sync --group browser`), so `run_python` users don't download it. The changed project hash makes existing environments reinstall the base packages once.
- **`oxide-core` (`browser.rs`):** server and tool names, `BROWSER_SWITCH` (`disabledBuiltins: ["browser"]`), `browser_tool_reads_only`, `BrowserSettings { enabled (off), source (auto | bundled | cdp), cdpUrl (127.0.0.1/localhost only), showWindow (off), ask (on), maxSteps 25, taskTimeoutSecs 600, idleMinutes 15, agentModel }` in `AppSettings.browser`, `BrowserStatus`, `BrowserStep`, `InstalledBrowser`, `BrowserFrame`. `ToolUsageKind::Browser` (stored as `"browser"`).
- **`oxide-pyenv`:**
  - `PythonEnv::ensure_group` / `group_ready`: installs a dependency group on top of the environment (`uv sync … --group <g> --locked`), checks its modules import, records it in `groups/<g>.json` with the project hash. The base install (exact sync) removes the markers, so a repair or update reinstalls the group on next use. Reset also removes `groups/`, `agents/` and `browsers/`.
  - `build.rs` embeds `python/oxide_browser_agent/*.py` (`AGENT_FILES`); they are written to `<python>/agents/oxide_browser_agent/` before a server starts.
  - `browser.rs` — `BrowserAgents`: setup (Python → packages → Chromium when needed or asked, with steps, percentages from Playwright's progress bars, log and errors as `BrowserStatus` events; `playwright install --no-shell chromium` into `<python>/browsers`, its path recorded), `find_installed_browsers` (Chrome, Edge, Chromium in their usual places), one server per chat (`python -m oxide_browser_agent` through `oxide-mcp`'s client, `PYTHONPATH`, `PLAYWRIGHT_BROWSERS_PATH`, `OXIDE_BROWSER_*`, `OXIDE_LLM_*`; tool timeout = task limit + 2 min), restarted when the settings, the browser or the model (key hashed, never logged) change; progress notices → `BrowserFrame`s; `call` (retries once when the server went away), `close`, `forget`, idle reaper, `shutdown`, `remove_chromium`. `tests/real_browser.rs` (ignored; Windows real mode or Linux stand-in mode).
- **`oxide-agent`:** `tools/browser.rs` (the seven entries as `browser__<tool>`, descriptions matching the server, labels). `ToolKind::Browser { tool }` (grant key `builtin/browser__<tool>`, usage kind Browser). Offered when Settings → Browser is on and the chat hasn't switched it off; the model for `browser_task` is Settings' agent model or the chat's own, with its endpoint, key (`ModelRegistry::endpoint`) and image support. Approvals: `read_page` and `screenshot` never ask; the others ask while `browser.ask` is on. System prompt paragraph (no passwords or payment data; nothing bought, posted or sent unless asked; close when done). Deleting a chat closes its browser. Tests: tools follow the setting and the chat switch, approvals, a failed setup is explained, the usage log (unix); ignored chat-level real test.
- **`src-tauri`:** `commands/browser.rs` (`browser_status`, `browser_setup(chromium)`, `browser_close`, `browser_remove_chromium`), events `BrowserChanged` and `BrowserLive`, `AppState.browser`, `save_settings` reconfigures it, the reaper starts with the app, quitting closes all browsers (with the sandboxes, 15 s cap), Python repair/reset close browsers first.
- **Frontend:** `features/browser/` (`queries.ts`: status query, setup/close/remove mutations, app-wide `browserChanged`/`browserLive` listener in `AppLayout`, a live-frame store by tool call; `BrowserSection.tsx`: Settings → **Browser** with the setup card (steps, percentage bar, log, errors, Try again, Set up now, Download/Remove Chromium, which browser chats use), the switch, which browser (this computer's / Oxide's Chromium / my own Chrome with the address and how to start it), show window, model for browsing tasks, limits, approvals; `BrowserPanel.tsx`: the session panel's **Browser** section with Open/Closed, the last screenshot and address, Close). Connectors: **browser (Built in)** card with a switch. “+” menu **Browser** switch. Tool lines: “Opening / Opened”, “Browsing / Browsed”, “Clicked [3]”, “Typed “rye””, a live screenshot under a running call with its status text, task and screenshot images under the call afterwards; the approval card shows the goal and what will happen. “Used in this session” has a **Browser** row; grants read “Browser: navigate”. Vitest `features/browser/Browser.test.tsx` against `src/test/fakeBrowser.ts`.

Deviations from PLAN.md (and why):
- **One server per chat, not an MCP-manager entry** (like Phase 12b's file server): each chat gets its own browser (pages and tabs don't mix between chats), and Browser-Use needs the chat's model. Connectors shows it as a “Built in” card whose switch is Settings → Browser's.
- **Browser-Use's model** comes in environment variables at server start (as planned); a chat whose model changes gets a new server (and browser). Settings → Browser can name a separate model for browsing tasks.
- **Chrome or Edge first, Chromium when needed:** the default uses a browser installed on this computer with a throwaway profile, so most people download nothing extra (and Playwright's Node downloader is spared on this PC, where Node's downloads from GitHub failed in Phase 12). Chromium is downloaded on first use when none is installed, or on request; “Chromium downloaded by Oxide” always uses it. Reusing the user's Chrome over CDP is a third choice.
- **Granular tools use Playwright directly**, not Browser-Use's actions: they're predictable, testable without a model, and share the browser with Browser-Use over CDP.
- **The browser packages are a dependency group** installed on first use, not part of every Python setup.
- **Live screenshots** travel as JSON in MCP progress notices and reach the UI as `browserLive` events (not `mcpChanged`), keyed by the tool call.
- **Off by default** (it installs packages and drives websites as the user).

Known issues / TODO carried forward:
- **Browser-Use 1.0.2 came out on 2026-11-11** and couldn't be installed on the agent side (no PyPI there): its API is taken from its documentation (`Agent(task, llm=, browser=)`, `Browser(cdp_url=, keep_alive=)`, `ChatOpenAI(model, api_key=, base_url=)`, `ChatAnthropic(model, api_key=)`, `run(max_steps=)`, `final_result()`/`is_done()`/`is_successful()`). The Windows exit check is its first real run; if it fails there, `agent.py` is the place to adapt.
- Browser-Use needs a capable model; small local models may wander or stop early (the granular tools work with any tools model).
- No per-site permissions yet (Phase 14's extension has them); the system prompt rules out passwords, payments and posting.
- In CDP mode, Chrome 136+ only allows remote debugging with a non-default `--user-data-dir`; while it runs, any local program can control that Chrome (Settings says so).
- Live screenshots are kept in memory for the session; screenshots in results are saved with the reply.
- Settings → Advanced → Reset also removes Oxide's Chromium; a changed model restarts the chat's browser (open pages are lost).
- A connector of the user's named “browser” with tools named like Oxide's would be drawn as the browser in tool lines (the same holds for “sandbox”).
- `src/bindings/index.ts` was edited by hand; `phase13.ps1` regenerates it.

Notes for Phase 14:
- `oxide-bridge` is empty; Phase 14 adds the Chrome extension. The browser agent's page text (`pagetext.py`) and tool names (`navigate`, `read_page`, `click`, `type`, `screenshot`) are a model for the extension's `read_page`/`find`/`click`.
- The cloud container has Python 3.13 with `mcp` 2.3.0 and Playwright 1.63 and `/usr/bin/google-chrome` (no Browser-Use, no PyPI): run `OXIDE_TEST_SYSTEM_PYTHON=/usr/local/bin/python3 OXIDE_TEST_CHROME=/usr/bin/google-chrome [OXIDE_TEST_FAKE_BROWSER_USE=1] cargo test -p oxide-pyenv --test real_browser -- --ignored` and `OXIDE_TEST_CHROME=/usr/bin/google-chrome python3 -m unittest discover -s python/tests`. Remove `python/**/__pycache__` before syncing.

Verified: fmt ✅ clippy ✅ tests ✅ (cargo + Vitest) typecheck ✅ lint ✅ vite build ✅ real browser + real Browser-Use ✅ (Windows, phase13.cmd). tauri dev on Windows ✅ (manual exit check by the user; the new-chat switch fix to recheck).

## Phase 12b — Docker sandbox workspace — 2026-11-12
Status: done. Committed `phase 12b: Docker sandbox workspace` on the second run of `scripts\win\phase12b.cmd` (2026-11-12 10:18): bindings, fmt, prettier, clippy, cargo test, typecheck, lint, Vitest, vite build, image downloads and both real-Docker exit tests (real `python:3.12-bookworm` and `mcp/filesystem`) passed. Every manual exit check passed on Windows (2026-11-12). Agent-side checks passed: Rust fmt, clippy `-D warnings` and tests on Linux (core 67, store 47, llm 45, agent 114 + 1 ignored real-Docker, mcp 46 + 21 integration, pyenv 13 + 1 ignored, sandbox 10 + 3 ignored real-Docker); **the ignored real-Docker tests also passed in the cloud container** against a real engine (`dockerd` runs there) with local stand-in images (a copy of the container's root filesystem for commands, and a small Python MCP server with `mcp/filesystem`'s tool names for files — Docker Hub isn't reachable there); tsc, eslint and prettier on the device VM. Vitest and `src-tauri` (bindings export) run only on Windows; `src/bindings/index.ts` was edited by hand and `phase12b.ps1` regenerates it.

First Windows run (2026-11-12 10:06): not committed. Bindings, fmt, prettier, clippy, cargo test, typecheck, lint, Vitest, vite build, the image downloads, the chat-level real-Docker test and 2 of 3 sandbox real-Docker tests passed. **Failed:** `build_fix_test_and_copy_out` — the `mcp/filesystem` image on Docker Hub is older than the README: it has `read_file` but no `read_text_file`. Since `read_file` was hidden as a “deprecated alias”, models would have had no read tool at all with the real image. Fix: `read_file` is hidden only when `read_text_file` is also offered; the test reads with whichever exists (exactly one is offered). Checked in the cloud container against a new-style and an old-style stand-in server.

What the phase script does (`scripts\win\phase12b.ps1`):
1. Stops a running dev app.
2. `npm run bindings`.
3. `check.ps1` (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest), then `vite build`.
4. The exit check against the real Docker engine: `docker version` (**Docker Desktop must be running**; otherwise nothing is committed), `docker pull python:3.12-bookworm` and `mcp/filesystem` (≈ 400 MB the first time), `cargo test -p oxide-sandbox --test real_docker -- --ignored --test-threads 1` and the chat-level `sandbox_build_fix_and_copy_out_through_a_chat`.
5. Commits with `logs\commit-msg-phase12b.txt` only if all of that passed, then starts the app.

Exit criteria (all checked by hand by the user on 2026-11-12):
- [x] **Build, test, fix, copy out.** Settings → Sandbox: Docker shows as running; switch **Let models use a sandbox** on (and, if you like, **Download images**). New chat with a tools model, “+” → Add folder → any folder. Ask: “In the sandbox, make a small Python project with a function and a unittest for it, run the test, make it pass, then copy the project to my folder.” Expect: approval cards for `sandbox_exec` (with the command) and the `sandbox__…` file writes (Allow always for this chat covers the rest of each), the tool lines “Ran in the sandbox …”, the session panel's **Sandbox** section with Running, the files, and a copy-out card; the project ends up in the shared folder. (The plan's “including a failing test the model then fixes” depends on the model; the real-Docker tests check exactly that path in code.)
- [x] **Docker stopped → clear message.** Quit Docker Desktop, then ask a chat to run something in the sandbox: the tool result says Docker isn't running (start Docker Desktop) and the model passes it on; Settings → Sandbox shows “Docker isn't running” with **Check again**; the panel's Sandbox section shows the message too.
- [x] **Host paths unreachable** (by hand: the model's `ls /` showed only the container's own root; `/mnt` empty, no `C:\Users`, `/c` or `/mnt/c`) (tested: `build_fix_test_and_copy_out` checks the container's mounts and that a host path doesn't exist inside; the file server refuses paths outside `/workspace`; copies only reach shared folders, never overwrite, refuse symbolic links). By hand: “In the sandbox, list C:\Users” → nothing of your computer is there.
- [x] Panel: **Save** (download icon) on a file copies it to a folder you pick; **Copy in a file**; **Stop** (files stay; **Start and show files** brings it back); **Reset** asks, then the sandbox is empty.
- [x] Idle: after the idle time (Settings → Sandbox, default 15 min) the container stops by itself (`docker ps` no longer lists `oxide-sandbox-…`); quitting Oxide stops all of them.
- [x] Deleting the chat removes its container and volume (`docker ps -a`, `docker volume ls`).

Done:
- **`oxide-core` (`sandbox.rs`):** tool names (`sandbox_exec`, `sandbox_copy_in`, `sandbox_copy_out`), `SANDBOX_SERVER` (`sandbox`), `SANDBOX_SWITCH` (`disabledBuiltins: ["sandbox"]` switches the sandbox off in a chat), `WORKSPACE_DIR`, defaults (`python:3.12-bookworm`, `mcp/filesystem`, 2 CPUs, 2 GB, 10 min per command, 15 min idle), `SandboxSettings { enabled (off), image, filesImage, cpus, memoryMb, network (off), storage (volume | folder), folder, idleMinutes, timeoutSecs, ask (on) }` in `AppSettings.sandbox` (normalized; `container_key` = what a container is made with), `clean_image`, `sandbox_name`, `workspace_path` (normalizes and refuses paths leaving `/workspace`), `DockerState/DockerStatus/SandboxImage`, `SandboxState/ChatSandbox/SandboxCommand`, `SandboxFile/SandboxListing`, `SandboxChange`. `ToolUsageKind::Sandbox` (stored as `"sandbox"`). No migration: a chat's sandbox state lives in Docker (labels) and its workspace.
- **`oxide-sandbox` (new crate, no Tauri):**
  - `Docker`: finds `docker` (PATH, then Docker Desktop's install folders), runs it without a console window, bounded output (4 MB per stream), time limits.
  - `Sandboxes`: Docker status (cached 20 s; engine version, CPUs, memory, whether both images are downloaded), `pull`/`prepare` (one download at a time; the status shows it), `ensure` (one per chat at a time: makes or starts `oxide-sandbox-<chat id>`; a container made with other image/limits/network is replaced and its workspace kept), `exec` (bash when the image has it; every process of a run carries `OXIDE_EXEC=<run>`, so a timeout kills the whole command inside the container, not just the `docker` client), `list`, `copy_in`, `copy_out`, `stop`, `reset`, `forget`, idle reaper (every minute; the first pass also stops sandboxes an earlier run left running), `shutdown`.
  - Containers: label `oxide.sandbox=1` + chat, config and workspace labels; `--cap-drop ALL` plus CHOWN, DAC_OVERRIDE, FOWNER, FSETID, SETUID, SETGID, KILL (what package managers need), `no-new-privileges`, `--pids-limit 1024`, CPU and memory limits (capped to what Docker has), `--network none` unless allowed, only the workspace mounted, `PYTHONDONTWRITEBYTECODE=1` (see below).
  - `files.rs`: the reference filesystem server per chat (`docker run -i --rm --name oxide-sandbox-files-<id> --network none … mcp/filesystem /workspace`) connected with Oxide's own MCP client (`oxide-mcp`), roots answered with `/workspace`; its tool list is learned once (from the first chat's server, or a throwaway one on an empty tmpfs when the image is downloaded) and cached; `read_file` is hidden only when `read_text_file` is there too (newer servers keep it as a deprecated alias; the Docker Hub image has only `read_file`).
- **`oxide-agent`:** `tools/sandbox.rs` (entries, input checks, result text with exit code, both streams cut to start and end, time-limit note). `Builtin::SandboxExec/CopyIn/CopyOut` and `ToolKind::SandboxFile { tool }` (its own kind, so a connector of the user's called “sandbox” is never routed to it; grant key `builtin/sandbox__<tool>`). Offered when Settings → Sandbox is on and the chat hasn't switched it off: `sandbox_exec` always (so a request ends in Docker's own message when Docker is down), copies when the chat has shared folders, file tools when known. Approvals: copies always ask; `sandbox_exec` and non-read-only file tools ask when `sandbox.ask` (default on), read-only file tools never. System prompt paragraph (image, network, file tools, copies). `ChatService::forget` removes a deleted chat's sandbox and volume. Tests: tools follow the setting, a missing Docker is explained, off by default; ignored real-Docker chat test (fail → fix via the file server → pass → copy out with approvals).
- **`src-tauri`:** `commands/sandbox.rs` (`sandbox_docker_status`, `sandbox_prepare`, `chat_sandbox`, `sandbox_files`, `sandbox_stop`, `sandbox_reset`, `sandbox_copy_in` (file or folder picker), `sandbox_save` (folder picker)); event `SandboxChanged` (`events.sandboxChanged`); `save_settings` reconfigures the sandbox; the reaper starts with the app; quitting stops all sandboxes (15 s cap).
- **Frontend:** `features/sandbox/` (`queries.ts`: queries, mutations, app-wide `sandboxChanged` listener in `AppLayout`, `useShowSandbox`; `SandboxSection.tsx`: Settings → **Sandbox** with Docker's status, **Get Docker**, **Check again**, images with **Download images**, limits, internet access, approvals, where workspaces live; `SandboxPanel.tsx`: the session panel's **Sandbox** section with state, image/network/workspace, running commands, file browser with breadcrumbs and **Save** per entry, **Copy in a file/folder**, **Stop**, **Reset** (confirm), **Start and show files** for a stopped one, Docker's message). “+” menu **Sandbox** switch (when on in Settings). Connectors lists the built-in **sandbox** file server. Tool lines (“Running in the sandbox …”, “Wrote in the sandbox app/m.py”, copies), the approval card shows the command and what a copy does, “Used in this session” has a **Sandbox** row, grants read “Sandbox files: write file”. Vitest `features/sandbox/Sandbox.test.tsx` against `src/test/fakeSandbox.ts`.

Deviations from PLAN.md (and why):
- **The file server isn't an MCP-manager entry.** Manager servers are shared by every chat and their tools go to every chat; each chat's file server must see only that chat's workspace. It runs per chat through `oxide-mcp`'s client, and Connectors shows it as a read-only “Built in” card.
- **Approvals:** commands and file changes inside the sandbox ask by default (as the plan says), with a switch in Settings → Sandbox to stop asking, since they can't reach the computer; copies in and out always ask. Read-only file tools never ask (they only read the sandbox).
- **Copies from the panel** (Copy in, Save) go through the system picker, which is the approval; the model's copies are confined to shared folders.
- **Default image `python:3.12-bookworm`** (Debian + Python 3.12, pip, gcc, make, git, curl) as the “small image with common toolchains”; about 400 MB to download. With the network off, the model can't `pip install`; turn on Internet access for that.
- **“Sandbox tab”** is a section of the session panel (the panel is sections, not tabs).
- **`PYTHONDONTWRITEBYTECODE=1`** in sandboxes: found by the chat test — a fix written in the same second as the file it replaces, at the same size, ran Python's stale bytecode (it checks only mtime and size).

Known issues / TODO carried forward:
- Changing the image, limits or network replaces a chat's container the next time it's used: files in `/workspace` stay, packages installed elsewhere don't (said in Settings).
- Changing where workspaces live applies to new sandboxes; existing ones keep theirs until reset.
- `docker cp` to Windows fails on names Windows doesn't allow (e.g. `:` in a file name); the error is passed on.
- If an image sets a non-root user, commands run as that user while the file server runs as root (it can always write; the command user may not change those files).
- `src/bindings/index.ts` was edited by hand; `phase12b.ps1` regenerates it.

Notes for Phase 13:
- The browser agent could run inside a sandbox later (Playwright in a container); for now Phase 13 uses the managed Python venv as planned.
- `Sandboxes` is in `AppState` and `Tools::sandboxes()`; real-Docker tests can be run in the cloud container: start `dockerd`, build local images (no Docker Hub), and set `OXIDE_SANDBOX_IMAGE` / `OXIDE_SANDBOX_FILES_IMAGE`.

Verified: fmt ✅ clippy ✅ tests ✅ (cargo + Vitest) typecheck ✅ lint ✅ vite build ✅ real Docker ✅ (Windows, phase12b.cmd). tauri dev on Windows ✅ (manual exit check by the user).

## Phase 12 — Managed Python (uv) & code execution — 2026-11-08
Status: done. Every exit check passed by hand on Windows (2026-11-08, Strata `swift-iq2_xs`) after the fix `e9455ff phase 12: Settings shows Python as ready after a run installed it`. `b8fbacc phase 12: managed Python (uv) and run_python` on the third run of `scripts\win\phase12.cmd` (2026-11-08 11:36): fetch uv, `uv lock` (CPython 3.12.14), bindings, fmt, prettier, clippy, cargo test, typecheck, lint, Vitest, vite build and the real-uv exit check (no Python → chart, with the sidecar) all passed. Agent-side: Rust fmt, clippy and tests on Linux (core 60, store 47, llm 45, agent 107, mcp 46 + 21 integration, pyenv 13 + 1 ignored real-uv test); tsc, eslint and prettier on the device VM.

First Windows run (2026-11-08 11:17): not committed. The bindings, cargo test, typecheck, lint, vite build and the real-uv exit check passed (the exit check used the uv on the PATH, since the sidecar was missing). Three failures, fixed:
- `fetch uv`: GitHub answered 504, so there was no sidecar and no `uv.lock`. `fetch-uv.mjs` now retries network errors and 5xx answers (3 retries, 2/4/6 s).
- clippy: `PNG_1X1` in `chat.rs` is only used by a `#[cfg(unix)]` test, so it was dead code on Windows; it is `#[cfg(unix)]` now.
- Vitest, 2 of the new `Python.test.tsx` tests (test-side): `renderApp` resets the fake backend, so the `fakePythonReady()` call before it was lost (it now sends the event after rendering); and the failure test failed setup before the setup command's first status arrived (it now waits for the step list).

Second run (11:30): Node's download still got 504 every time while Chrome downloaded the same URL fine, so something between Node and GitHub (probably a proxy that Node uses and the browser doesn't) answered instead of GitHub; not investigated further. `fetch-uv.mjs` now uses `Downloads\<archive>` or `--archive <file>` first, falls back to PowerShell's `Invoke-WebRequest` (Windows proxy settings) when Node fails, and prints the proxy variables Node saw. Third run (11:34): it used the zip the user had downloaded in Chrome; **its SHA-256 matched the release's own `.sha256`**, so the Windows x64 pin (`f65744f9…cac2`) is verified. The other five targets in `uv-pins.json` are still `null` (pinned on first download).

What the phase script does (`scripts\win\phase12.ps1`):
1. Stops a running dev app.
2. `node scripts\fetch-uv.mjs`: downloads the pinned uv (0.12.10) for the host into `src-tauri\binaries\uv-<triple>.exe`.
3. `uv lock --project python --python 3.12` when `python\uv.lock` is missing or older than `pyproject.toml`.
4. `npm run bindings` (replaces the hand-edited `src/bindings/index.ts`).
5. `check.ps1` (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest), then `vite build`.
6. The exit check against the real uv: `cargo test -p oxide-pyenv --test real_uv -- --ignored` (temporary profile, no Python → CPython + packages → `chart.png` + `table.csv`; downloads Python and the packages).
7. Commits with `logs\commit-msg-phase12.txt` only if all of that passed, then starts the app.

Things to know before the first run:
- **uv checksums:** Windows x64 is pinned and matched the release's `.sha256`; other targets are `null` until their first download (which also compares with the release's `.sha256` when readable).
- **`python\uv.lock` is generated by the script**, not written by the agent (no uv here). The commit includes it.
- If the first `uv lock` or the real-uv test fails because of the network, nothing is committed; fix and rerun.

Exit criteria (check by hand in `cargo tauri dev` once the script has committed; ideally on a fresh Windows profile):
- [x] **No Python → working `run_python`, no manual steps.** (2026-11-08, Strata `swift-iq2_xs`: the chart came back into the chat.) New chat with a tools model: “Make a bar chart of 3, 5, 2 with matplotlib and save it.” An approval card shows the code; Allow once. The tool line shows **Setting up Python** the first time; the chart appears under the call without opening it.
- [x] **Progress UI.** (2026-11-08: **failed first** — after check 1 had installed Python, Settings → Advanced still said “Not set up yet”. Cause: the last event after setup was built from the live state, which never says “ready”, and the status query is cached forever. Fix (`scripts\win\phase12-fix.cmd`): setup and reset end with the full status read from disk (`emit_settled`), the status query has `staleTime: 0`, and the pyenv test now checks the last event (it fails on the old code). Rechecked after `e9455ff`: Ready, then Reset → Set up Python ticked off the steps.) Settings → Advanced → Reset, then **Set up Python**: the steps tick off (Find uv, Install Python, Create the environment, Install the packages, Check that they import), then Ready with Python 3.12.x, the four packages and the disk space.
  - Also found by the fix run (11:44): `oxide-mcp`'s Phase 10 test `install_configure_toggle_update_uninstall` failed once on Windows with os error 32 (the old extension folder was still in use). `drop_extension_server` only signalled the server to stop, and the folder swap's 5 s of retries could run out before the process had exited (2 s grace + kill + antivirus). It is now async and waits (up to 10 s) for the server's task, which ends only after the process has exited; install and uninstall await it.
- [x] **Repair and reset.** Repair rebuilds the venv (disabled while a script runs); Reset asks first and removes the environment, the downloaded Pythons and the cache.
- [x] **Files.** “Write a CSV of 5 rows with pandas.” → the CSV opens as an artifact card; the chart is a thumbnail.
- [x] **Granted folder.** With a folder added to the chat, a script writing a file there writes into it (the working dir is the first folder, or `cwd` inside one); without folders it runs in `<app data>\python\work\<chat id>` and files persist between runs.
- [x] **Time limit.** `import time; time.sleep(999)` with the limit at 5 s (Settings → Permissions → Python) ends with “timed out” and no leftover `python.exe`.
- [x] Python off in Settings → Permissions removes the tool from new replies.

Done:
- **`oxide-core` (`python.rs`):** constants (`RUN_PYTHON`, `PYTHON_VERSION` 3.12, `PYTHON_PACKAGES`, timeouts 120 s default / 3,600 max, limits: 6 images ≤ 8 MB, 4 artifacts ≤ 200 KB, code ≤ 200k characters), `PythonSettings { enabled (on), timeoutSecs }` in `AppSettings.python` (clamped), `PythonState` (missing, installing, ready, failed), `PythonStep` (uv, python, venv, packages, check), `PythonStatus`, `PythonPackage`, `OutputFile`/`classify_output` (image / text artifact / other by extension), `code_label`. `ToolUsageKind::Python` (stored as `"python"`).
- **`oxide-pyenv` (new crate, no Tauri):**
  - `PythonEnv::{ensure, status, ready, repair, reset, run, work_dir, forget_chat, running_scripts, find_uv, venv_python}`. Layout under `<app data>\python\`: `pythons\` (uv-managed CPython), `uv-cache\`, `venv\`, `project\` (the embedded `pyproject.toml` + `uv.lock`), `runner\oxide_run.py`, `mpl\`, `work\<chat id>\`, `state.json`.
  - Bootstrap = `uv python install 3.12`, `uv venv --python 3.12`, `uv sync --project … --no-install-project` (`--locked` when a lock is embedded), then an import check of the packages. uv is configured only through environment variables (`UV_PYTHON_INSTALL_DIR`, `UV_CACHE_DIR`, `UV_PROJECT_ENVIRONMENT`, `UV_PYTHON_PREFERENCE=only-managed`, `UV_PYTHON_INSTALL_BIN=0`, `UV_PYTHON_INSTALL_REGISTRY=0`, `UV_NO_CONFIG`, …) so no flag has to exist in the pinned version. `state.json` records the project hash: a changed `pyproject.toml`/`uv.lock` (after an update) shows as “missing” and reinstalls. Concurrent `ensure` calls share one install; progress goes to a `PythonSink`.
  - `uv.rs`: `find` (explicit path, then the app's folder as `uv`/`uv-<triple>`, then PATH; an empty placeholder file doesn't count). `process.rs`: output capture (4 MB), timeout, and whole-process-tree kill (process groups on Unix, `taskkill /T` on Windows). `files.rs`: before/after snapshots find files a script wrote. `oxide_run.py`: launcher that sets the Agg backend, runs the code, and saves open matplotlib figures as `figure-N.png` when the script saved none.
  - `build.rs` embeds `python/pyproject.toml` and `python/uv.lock` (warns when the lock is missing; an empty lock still works).
- **`oxide-agent`:** `tools/python.rs`: `run_python { code, cwd? }`, offered when the setting is on and the chat hasn't switched tools off. **Always asks** (the code isn't sandboxed and runs as the user); “Allow always” applies to the chat only (grant key `builtin/run_python`). Working dir: the first granted folder or `cwd` inside one (same `Sandbox` path checks as the file tools), else the chat's private `work\` folder. Result text = exit code, stdout, stderr (30k characters, start and end kept), timing, and a list of changed files; images become image blocks (so the model and the chat see the chart), small text files (csv, json, md, svg, html) become artifacts (`ArtifactRef`), others are listed. Deleting a chat removes its work folder. System prompt paragraph. 5 new tests (approval, folder confinement, files, timeout, setting off).
- **`oxide-mcp`:** extensions' Python servers use the managed uv first (`Extensions::set_uv`), and the “no uv” warning only shows when neither exists.
- **`src-tauri`:** `commands/python.rs`: `python_status`, `python_setup`, `python_repair` (refuses while a script runs), `python_reset`, `open_python_folder`; event `pythonChanged`. `externalBin: ["binaries/uv"]`. `build.rs` warns when the sidecar for the target is missing, creates an empty placeholder so dev builds work, and **fails release builds** until `npm run fetch-uv` ran.
- **Scripts:** `scripts/fetch-uv.mjs` + `scripts/uv-pins.json` (6 targets), `npm run fetch-uv`, `src-tauri/binaries/README.md`. Tested offline with a faked download: extracts, pins the hash, skips when present, refuses a changed archive.
- **Frontend:** `features/python/` (`queries.ts`: status query + app-wide `pythonChanged` listener mounted in `AppLayout`, setup/repair/reset mutations; `PythonSection.tsx`: Settings → Advanced with state, step list, uv log, versions, packages, disk, Set up / Try again / Repair / Reset (confirm) / Show folder). Settings → Permissions → **Python** (switch, time limit). Tool lines: `Braces` icon, “Running Python / Ran Python” with the first line of code, the approval card shows the **code** and a “not in a sandbox” warning, a **Setting up Python** badge while the environment installs, and charts and artifact links appear under the call without opening it. “Used in this session” lists Python.
- **Tests:** Rust as above, plus `crates/oxide-pyenv/tests/real_uv.rs` (ignored; needs `OXIDE_UV` and network). Vitest `features/python/Python.test.tsx` (setup steps and finish, failure + retry, repair/show folder/reset with confirm, permissions switch and limit, approval with code → setup badge → chart + artifact, helpers) against `src/test/fakePython.ts`.

Deviations from PLAN.md (and why):
- **Venv at `<app data>\python\venv`** (not `.venv`): uv's `UV_PROJECT_ENVIRONMENT` points there and a visible name is easier to find with **Show folder**.
- **Checksums are pinned on first download** (see above) because the release checksums couldn't be read from the agent side.
- **`run_python` always asks**, even for read-only scripts; there is no allowlist.
- **“Generated files attach to the reply and can open in Artifacts”** reuses the artifact tool's `ArtifactRef` blocks: images are shown as images, text files as artifacts, other files are only listed (no download button yet).
- Skill scripts (Phase 10) can't be run by `run_python` unless the model copies the code; no change to `load_skill`.

Known issues / TODO carried forward:
- Unix timeout kills the process group; a script that makes its own session may survive. Windows kills the tree.
- Only CPython 3.12 and four packages: no `pip install` for the model (no network install step is offered); extra packages mean editing `python/pyproject.toml` and regenerating the lock.
- `src/bindings/index.ts` was hand-edited; the phase script regenerates it.

Notes for Phase 13:
- The browser agent's Python package goes in `python/` next to `pyproject.toml`; add its dependencies to the same lock (the project hash in `state.json` makes the app reinstall).
- `PythonEnv::ensure` is the way to get a ready environment; `venv_python()` is the interpreter for sidecar processes.

Verified: fmt ✅ clippy ✅ tests ✅ (cargo + Vitest) typecheck ✅ lint ✅ vite build ✅ real uv ✅ (Windows, phase12.cmd). tauri dev on Windows ✅ (manual exit check by the user, after `e9455ff`).

## Phase 11 — Agentic tasks (Cowork-style) — 2026-11-07
Status: done. Committed `f312ced phase 11: agentic tasks (folder grants, task list, session panel, shell)` on the second run of `scripts\win\phase11.cmd` (Vitest 186/186), then `11cbe6b` (notification fixes); every exit check passed by hand on Windows. `e5822b0` (deleted chats' approvals, stop race) after the recheck; the user confirmed everything works. Agent-side checks passed: Rust fmt, clippy `-D warnings` and tests on Linux (core 56, store 47, llm 45, agent 98, mcp 46 + 21 integration); typecheck, lint and prettier on the device VM. Vitest and `src-tauri` (bindings export) run only on Windows.

First Windows run (2026-11-07): fmt, prettier, clippy, cargo test, typecheck, lint and the vite build passed; 4 of 186 Vitest tests failed, all in the new `Workspace.test.tsx` and all test-side:
- The “+” menu was opened with `pointerDown`, which Radix ignores under jsdom; it now opens with Enter on the trigger, like the other tests.
- “Allowed in this chat” expected a new approval to refresh the list, but the app refreshes it when an approval is answered. The test now answers a card with “Always allow this command”, and the fake backend stores real grant keys (`server/tool`, `builtin/tool`, `builtin/run_command:<command>`).
- The shell approval test didn't send the `toolUse` event that opens a tool block, and didn't wait for the held reply; both added.

Exit criteria (check by hand in `cargo tauri dev`; `phase11.ps1` makes `Documents\Oxide sample folder` with ten mixed files the first time it runs):
- [x] **Organize a folder end to end.** (2026-11-07, Strata `swift-iq2_xs`: a 4-step plan ticked off live with the pill at “4 of 4 done”; files sorted into `data/`, `documents/`, `images/`, `code/`, `config/`, `logs/` with a `summary.md`; Used in this session showed Files ×22; Allowed in this chat listed move file and write file. The model also tried the Phase 5 `filesystem` connector's `list_directory_with_sizes` once, which failed (probably outside that server's allowed directories; not checked); it went on with Oxide's own tools.) New chat → “+” → **Add folder** → `Documents\Oxide sample folder` (a folder chip shows above the text). Ask: “Organize this folder by file type and write a summary.md of what's in it.”
  - The title bar shows a **Step n of m** pill and the right-hand panel opens on **Progress** with the plan; steps tick off live (in progress → done).
  - `list_dir`/`read_file`/`search_files` run without cards. Every `move_file` and `write_file` shows an approval card (“Allow always for this chat” then covers the rest of that tool's moves).
  - **Used in this session** fills in: files and folders (with a count), the task list; clicking a row scrolls to the call. **Allowed in this chat** lists the “Allow always” choices; ✕ takes one back.
  - The folder ends up sorted into subfolders with a `summary.md`.
- [x] **Delete.** “Delete ideas.md.” → an approval card marked destructive; after Allow, the file is in `_to_delete\` inside the shared folder. Tick **Delete for good** in the panel's Folders section and repeat with another file: it's gone.
- [x] **Path escapes are blocked.** “Read C:\Windows\win.ini” and “Read ..\..\something” → the tool answers that the path is outside the shared folders (also covered by tests: `escapes_are_refused`, `links_out_of_the_grant_are_refused`, `workspace_tools_stay_in_granted_folders_and_ask_before_changes`).
- [x] **Shell (off by default).** Settings → Permissions → Shell commands → on; add `Get-ChildItem` to the allowlist. In the sample folder's chat: “List the files with Get-ChildItem, then run `New-Item test.txt`.” → the first runs without asking, the second asks; “Allow always” there allows only that exact command.
- [x] **Long-running.** Start the organize prompt, then hide Oxide (✕ to the tray): an approval card notifies (Phase 9), and the sidebar row shows **Needs input** while a card waits and a pulsing dot while it works. A finished reply notifies.
- [x] The panel's open/closed state is remembered per chat (close it in one chat, open another, come back).

Manual check by the user (2026-11-07, `f312ced`, Strata `swift-iq2_xs`): parts 1–3, the shell and the panel memory passed. **Failed: no notification for a waiting approval card**, although Oxide was already hidden when the card appeared (the sidebar showed “Needs input”). The log had no “couldn't show a notification” warning, and nothing said whether Oxide skipped it or Windows dropped it. Fix (`scripts\win\phase11-fix.cmd`, committed `11cbe6b`):
- `notify.rs` logs each decision (notifications off / Oxide in front / notifying) and each notification shown, so `logs\dev.log` tells which it was.
- Hiding the window to the tray while approvals wait sends a reminder (“A tool call is waiting for your approval.”), since the card's own notification went out while Oxide was in front.
- Settings → Desktop → **Send a test notification** (`test_notification` command) checks whether Windows shows Oxide's notifications at all (a dev build's may be attributed to PowerShell, see Phase 9).
- Also from the check: a waiting write call read “Wrote … Needs your approval”; waiting calls now use the present tense (“Writing …”).

Recheck (2026-11-07, `11cbe6b`): Windows had notifications turned off; with them on, the test notification and the approval notification both showed. [x] Long-running passed. New bug: after deleting the chat whose card was waiting, every ✕ to the tray still sent “A tool call is waiting…”: deleting a chat didn't stop its reply, so its approval stayed in memory. Fix (`scripts\win\phase11-fix2.cmd`, committed `e5822b0`; rechecked by the user: no reminder after deleting the chat):
- `ChatService::forget` (stop the reply, drop its waiting approvals at once), called by `delete_conversation`. Test: `forgetting_a_chat_drops_its_waiting_approvals`.
- Found while testing it: **Stop while a card waits could continue the reply.** Stopping ends the waiting call at once, so the step's work and the stop were ready together and `tokio::select!` picked one at random; on “finished” the loop sent another model request. `run_calls` now checks the stop flag after the select. This was the occasional failure of `stop_while_waiting_keeps_a_valid_reply` (about 1 in 6 runs before, 0 in 40 after).

Idea from the user (not planned yet): push notifications to the phone when Oxide needs input (instead of email); a candidate for Phase 15 (scheduled tasks run unattended).

Done:
- **`oxide-core`:** `workspace.rs`: tool names (`list_dir`, `read_file`, `search_files`, `write_file`, `edit_file`, `move_file`, `delete_file`, `task_list`, `run_command`), `FILES_SWITCH` (`disabledBuiltins: ["files"]` switches all file tools off in a chat), `TO_DELETE_DIR`, limits (20 folders, 50 tasks, 200-character titles); `ChatFolder`, `ChatWorkspace { folders, allowDelete }`, `TaskStatus` (forgiving `parse`), `TaskItem`, `TaskList` (`progress`, `current`), `clean_task_title`, `command_allowed` (allowlist matched word by word; commands with `;`, `&`, `|`, `>`, `<`, backticks, line breaks or `$(` always ask). `StartConversation.folders` (granted before the first reply). Settings: `shell { enabled (off), allowlist, timeoutSecs (120, 1–3600) }`. `ToolUsageKind::File | Shell | Tasks`.
- **`oxide-store`:** migration `0008_workspace.sql`: `chat_folders`, `chat_workspace` (`allow_delete`), `chat_tasks` (JSON items); all cascade with the chat. `chat_workspace`, `add_chat_folder` (≤ 20), `remove_chat_folder`, `set_chat_allow_delete`, `task_list`, `set_task_list`; `start_conversation` grants the new chat's folders.
- **`oxide-agent`:**
  - `tools/files.rs`: `Sandbox` (the chat's resolved roots). Every path is made absolute (relative paths start at the first folder), `..` is resolved, and the longest existing prefix is canonicalized, so links pointing out of a grant are refused; the result must sit inside a root. Writes, moves and deletes can't target a root itself. Tools: `list_dir` (depth 1–3, sizes, folders first, ≤ 1,000 entries), `read_file` (text only, binary refused, files ≤ 10 MB, 60k characters per call with `start_line`), `search_files` (name wildcards and/or text, skips `.git` and `_to_delete`, ≤ 200 matches), `write_file` (creates parent folders), `edit_file` (exactly one match, like the artifact tool), `move_file` (creates parent folders, won't overwrite), `delete_file` (moves into `<root>\_to_delete\` with a unique name, or deletes for good when the chat allows it). `resolve_grant` checks a picked folder. Labels for the usage log.
  - `tasks.rs`: the `task_list` tool (whole list per call; accepts strings, `content`/`name`/`step`, `completed: true`, `in progress`/`completed`), summary returned to the model (“1 of 3 done; now: …”).
  - `tools/shell.rs`: `Shell::detect` (Windows: `pwsh.exe` if on the PATH, else Windows PowerShell, `-NoProfile -NonInteractive`, UTF-8 output; elsewhere `sh -c`), `run_command { command, cwd? }` in a granted folder, no console window (`CREATE_NO_WINDOW`), time limit (kills the process tree with `taskkill /T` on Windows), stdout/stderr captured (4 MB read, 30k characters shown, start and end kept), exit code.
  - `ChatService::setup`: the task list is in every tools-capable chat; file tools when the chat has folders; `run_command` when the shell setting is on and there are folders; each can be switched off per chat. Approvals: reading never asks, changes always ask (unless “Allow always”), the task list never asks; shell commands ask unless allowlisted, and “Allow always” is per exact command (`grant_key` = tool + command; answering one card closes only cards with the same command). `EventSink::tasks_changed`. System prompt: `<shared_folders>` with what the tools may do and where deletes go, the shell, and when to keep a task list.
- **`src-tauri`:** `commands/workspace.rs`: `resolve_folder`, `chat_workspace`, `add_chat_folder`, `remove_chat_folder`, `set_chat_allow_delete`, `chat_task_list`, `open_chat_folder`; event `TaskListChanged` (`events.taskListChanged`); `start_conversation` resolves the draft's folders first.
- **Frontend:**
  - `features/workspace/`: `SessionPanel` (replaces the old details panel: **Progress** with live statuses and “n of m done”; **Folders** with open/remove, Add folder and “Delete for good”; **Used in this session**; **Allowed in this chat** with per-grant and “Revoke all”; **Details** with model/tools/files), `StepPill` (“Step n of m” in the title bar; opens the panel), `FolderChips` (composer), `folders.ts` (`useFolderControls`: a saved chat's folders or a new chat's draft), `store.ts` (draft folders; panel open state per chat, persisted, falling back to the last choice; opens by itself the first time a task list appears), `queries.ts`, `tasks.ts`, `grants.ts`.
  - “+” menu: **Add folder**. Sidebar rows: **Needs input** badge while an approval waits (live or from `pending_tool_approvals`), a pulsing dot while a reply runs. Tool lines and “Used in this session” know file, shell and task-list calls. Settings → Permissions: a note on folder permissions and a **Shell commands** section (switch, time limit, allowlist).
- **Tests:** Rust: path resolution, `..`/absolute/other-root escapes, links out of a grant, no folders, list/read/search, write/edit/move, delete to `_to_delete` vs for good, several folders, wildcards (files.rs); task parsing and summaries; shell output, time limit, folder checks, long output, PowerShell (Windows only); allowlist matching; store CRUD and cascades; chat service: workspace tools confined and asking only for changes, switches and no-folder chats, shell commands per command and allowlist (Unix). Vitest (Windows): `features/workspace/Workspace.test.tsx` (draft folders granted with a new chat, relative paths refused, panel management, live progress + pill, Used in this session, revoking grants, Needs input badge, one command at a time, Shell settings, helpers); `fakeBackend.ts` implements the new commands and `emitTaskList`.

Deviations from PLAN.md (and why):
- **Tool names:** `search_files`, `move_file` and `delete_file` instead of `search`, `move` (clearer next to MCP tools); delete is its own tool.
- **“Delete needs per-chat permission”** is the approval card (marked destructive) for every delete, plus the panel's **Delete for good** switch; without it deletes go to `_to_delete`.
- **Reading in a granted folder never asks** (the grant is the permission); every change asks unless allowed for the chat.
- **Granted folders aren't sent to MCP servers as roots:** servers are shared by all chats while folders are per chat. Roots stay empty.
- **Long-running mode reuses Phase 9's notifications** (reply finished, approval needed, while the window is hidden or unfocused); the “needs your input” badge is on the sidebar row.

Known issues / TODO carried forward:
- On Unix a timed-out command's grandchildren may survive (`kill -9` reaches only the shell); Windows kills the tree.
- `read_file` reads text only; PDFs and images in a shared folder can't be read yet (attach them instead).
- `move_file` uses a rename, so moving between two shared folders on different drives fails with an error.
- The allowlist is a prefix check, not a parser: anything with chaining/redirect characters always asks.
- `src/bindings/index.ts` was edited by hand on the agent side; `cargo test` (export_bindings) regenerates it on Windows.

Notes for Phase 12:
- `run_python` can reuse `Sandbox` (granted folder as the working dir) and `shell.rs`'s capture/time-limit code; generated files in a granted folder already show in “Used in this session” when written through the file tools.

Verified: fmt ✅ clippy ✅ tests ✅ (cargo + Vitest 186/186) typecheck ✅ lint ✅ vite build ✅ (Windows, phase11.cmd) tauri dev on Windows ✅ (manual exit check by the user).

## Phase 10 — Extensions (.mcpb) & Skills — 2026-11-07
Status: done. `scripts\win\phase10.cmd` committed `6ccbc48 phase 10: extensions (.mcpb) & skills` on the second run (fixes below); the user then ran the whole exit check by hand on Windows (Strata `swift-iq2_xs`): everything passed.

First Windows run (2026-11-07): every Rust check, typecheck, lint and the production build passed; 3 of 175 Vitest tests failed, all in the new `Extensions.test.tsx`:
- The out-of-range number never reached the backend: the number field's `min`/`max` made the browser's own form validation block the submit (jsdom does the same). The install and settings forms are now `noValidate`, so Oxide's own message (“Days” must be at most 7) shows, like every other error.
- The fake backend's staging ids didn't restart per test (`stage-2` instead of `stage-1`); `resetFakeExtensions` resets the counter.
- A test expected “This extension has no settings.” in the install dialog, which leaves the settings section out when there are none; it now checks that the section is absent.

Exit criteria (check by hand in `cargo tauri dev`; samples are in `docs\samples`, see its README):
- [x] **A sample `.mcpb` installs, configures and runs.**
  1. Settings → **Extensions** → **Install extension…** → `C:\AI Apps\Oxide\docs\samples\oxide-dice.mcpb` (or drag the file onto Oxide's window). The dialog shows the icon, “Version 1.0.0 · by Oxide contributors · 4 files”, the trust note, the three tools and four settings. **Install** stays disabled (“Fill in: API token”) until a token is typed. Type `my-test-token`, set Lucky number `3`, turn **Shout** on → Install. The card shows “Running · 3 tools”.
  2. Settings → **Connectors**: `oxide-dice` has an “Extension · Oxide Dice” chip and **Manage** (no Edit); Settings → **Developer**: `oxide_config.json` has `oxide.extensions.oxide-dice` with the values but no token. Windows Credential Manager → Generic credentials has the token (`extension:oxide-dice:api_token`).
  3. A chat with a tools model: “Use Oxide Dice: check its settings, roll 5 dice and greet Mikkel.” → `settings_check` says `token: set (13 characters)`, `shout: true`, `lucky number: 3`, and the folder under `…\dev.oxide.app\extensions\oxide-dice`; `greet` says `HELLO, MIKKEL!`.
  4. The card's **Settings**: the token field is empty with “Saved in the keychain…”; change Greeting to `Hej` → Save → the server restarts; greet again → `HEJ, MIKKEL!`.
  5. The switch turns it off (Connectors: Off) and on again. **Install extension…** with the same file again → “Update Oxide Dice”, “Version 1.0.0 → 1.0.0”, the settings kept → Update.
  6. **Uninstall** → it leaves Extensions and Connectors, and its folder is gone (**Open extensions folder**).
- [x] **A sample skill is discovered and used by the model.**
  1. Settings → **Skills** → **Add .zip or .skill…** → `docs\samples\release-notes.skill` (or **Add folder…** → `docs\samples\skills\release-notes`). It's listed with “+ 1 file”; **View and edit** shows the instructions and `template.md`.
  2. A new chat: “Write release notes for Oxide 0.10.0: added desktop extensions and skills; fixed Esc closing Settings while recording a shortcut; removed the old placeholder pages.” → a “Loaded skill · release-notes” line without an approval card (often a second one for `template.md`), and the notes end with `— Brewed in Oxide.`. The right-hand panel's “Used in this chat” lists Skills.
  3. Switch the skill off; the same prompt in a new chat loads nothing and has no sign-off.
  4. **New skill**: name `pirate`, description “Use when the user greets with ahoy”, instructions “Answer like a pirate.” → “Ahoy! What's 2 + 2?” loads it.

Manual check by the user (2026-11-07, `6ccbc48`, Strata `swift-iq2_xs`):
- Part 1 (extension): all steps passed. In step 4 the first “greet Mikkel again” failed: the 2-bit model wrote its tool call in its own tag format and the server passed it on as broken JSON (`{"name":"Mikkel\n</parameter>\n</APSHOT>…`). Oxide refused the arguments and asked the model to retry, which it then typed as text. A retry called `greet` correctly and got `HEJ, MIKKEL!` (the new greeting, Shout still on). Model behaviour, not an Oxide bug.
- Part 2 (skills): all steps passed. The model loaded `release-notes` and then `template.md` without approval cards and ended the notes with “— Brewed in Oxide.” (it put them in a document artifact). Switched off, nothing loaded; the `pirate` skill written in Settings was loaded on “Ahoy!”.
- The user didn't find “Used in this chat” at first: the details panel opens from the title bar's panel button (“Show chat details”, left of minimize). Worth making easier to find in Phase 11's session panel.

Done:
- **`oxide-core`:** `extension.rs`: `ExtensionServerType` (node, python, binary, uv), `ExtensionFieldType`, `ExtensionValue` (untagged bool | number | list | text), `ExtensionField`, `ExtensionItem`, `ExtensionInfo` (manifest details, fields, icon as a data URL), `ExtensionStatus` (ready, needsSetup, unsupported, broken), `Extension`, `ExtensionPreview`; `Skill`, `SkillDetail`, `SkillInput`, `LOAD_SKILL`, `skill_slug`, `platform_name`. `McpServerState.extension` (display name of the extension a server comes from). `ToolUsageKind::Skill`.
- **`oxide-mcp`:**
  - `archive.rs`: a zip reader (stored + deflate via `flate2`, Zip64 sizes/offsets, CRC checks with `crc32fast`, Unix exec bits), path safety (`..`, absolute paths, drive letters refused; `__MACOSX` skipped), limits (1 GB, 100k files), clear errors for encrypted entries and other compression methods; `write_zip` for tests and samples.
  - `extensions/manifest.rs`: MCPB manifest 0.1–0.4 (and `dxt_version`): required fields, server types, `mcp_config` + `platform_overrides`, `user_config` (types, defaults with `${HOME}`/`${DESKTOP}`/`${DOCUMENTS}`/`${DOWNLOADS}`, multiple, sensitive, min/max), compatibility (platforms, runtimes), tools/prompts, icon/icons. `launch`: required settings (else `MissingSetting`), `${__dirname}`, `${user_config.KEY}` (a lone placeholder of a list becomes one argument per entry; unset optional ones disappear, also from `env`), `${/}`/`${pathSeparator}`; Python servers (`python`/`python3`/`py`) run as `uv run --no-project --quiet [--python <spec>] python …` when uv is found; `uv` servers without `mcp_config` run `uv run --directory <dir> --quiet <entry>`; relative binaries resolve inside the bundle; a summary with sensitive values masked. Names like `@org/tool` become `org-tool` (folder and connector name).
  - `extensions/mod.rs`: `Extensions` (data dir `extensions\`, staging in `extensions\.staging\`): `preview` (unpack in `spawn_blocking`, check manifest and entry point, refuse a name an `mcpServers` entry has, warnings for another OS and a missing Node.js/uv/Python, defaults or the installed version's values), `install` (checks values, stops the old server, swaps folders with retries for Windows file locks, saves settings, refreshes), `configure`, `set_enabled`, `uninstall` (stops it, removes entry, keychain entries and folder), `discard`, `list`/`get` (status and problem per extension, icon), `refresh` (builds every extension's server and hands them to the manager), stale staging clean-up after an hour. Sensitive settings go through the manager's `TokenStore` (the OS keychain) as `extension:<name>:<key>`; the config only lists which ones are set.
  - `config.rs`: `ExtensionEntry { enabled, version, installedAt, values, secrets }` under `oxide.extensions.<name>`; `extensions`, `extension`, `is_extension`, `set_extension`, `remove_extension`, `has_server`.
  - `manager.rs`: `ExtensionServer` and `set_extension_servers` / `drop_extension_server`; `apply` merges config servers with extension servers (enabled from the entry; a config entry of the same name wins); extension servers run in their own folder and show their masked summary and `extension` name; `set_enabled` works for both; `remove_server`/`save_server` refuse extension names; `config_snapshot` and `edit` are public for the host.
- **`oxide-agent`:** `skills.rs`: `Skills` (data dir `skills\`): `list`/`usable`, `detail`, `create`/`update` (writes `SKILL.md` with quoted front matter where needed), `add_folder` (the folder or its only subfolder; ≤ 2,000 files, 50 MB), `add_archive` (`.zip`/`.skill`), `set_enabled` (`.oxide-skills.json`), `remove`, `load` (instructions in `<skill>` plus the file list, or one text file in `<file>`, confined to the skill folder, symlinks refused, 100k characters). A front matter parser for what skills use (plain/quoted values with comments, `>` and `|` blocks, continuation lines, nested maps skipped). `Builtin::LoadSkill` (`load_skill { name, path? }`): in the tool set when skills are usable (and the chat hasn't switched it off), never asks, logged as `skill` with the name (and path). `ToolSet.skills` → `PromptContext.skills`: a `<skills>` list with when-to-load guidance, before the project instructions.
- **`src-tauri`:** `AppState.extensions` / `AppState.skills`; extensions refresh after the MCP manager starts. Commands (`commands/extensions.rs`): `pick_paths(kind, multiple)` (extension, skill archive, folder, file), `list_extensions`, `preview_extension`, `install_extension`, `discard_extension`, `configure_extension`, `set_extension_enabled`, `uninstall_extension`, `open_extensions_folder`, `list_skills`, `get_skill`, `create_skill`, `update_skill`, `add_skill` (folder or zip), `set_skill_enabled`, `remove_skill`, `open_skills_folder`.
- **Frontend (`features/extensions/`):** `ExtensionsSection` (install button, open folder, cards with icon, version · author, status from the connector, problem, switch, Details (runs as, license, manifest, tools, prompts, links), Settings dialog, Folder, Uninstall with confirmation; the install/update dialog with warnings, a trust note, details and the settings form; Install is disabled until required settings are filled), `ExtensionForm` (text, password for sensitive values with “Saved in the keychain…”, numbers with range, switches, folder/file with Browse, multi-folder lists with Add and typed paths), `fields.ts`, `SkillsSection` (new skill, add folder, add zip, open folder; rows with description, file count, problem, switch, view/edit dialog with the file list, remove), `queries.ts`, `store.ts` (a dropped `.mcpb` waits for the Extensions page). The composer's window-wide drop sends `.mcpb`/`.dxt` files to Settings → Extensions instead of attaching them. Connectors marks extension servers and offers **Manage**. Tool lines “Loaded skill · name”, a Sparkles icon, and “Skills” in “Used in this chat”.
- **Samples (`docs\samples`):** `oxide-dice.mcpb` (Node.js server without dependencies, legacy handshake, three tools, four settings incl. a sensitive token) with its source, and the `release-notes` skill as a folder and as `release-notes.skill`. `.gitattributes` marks `*.mcpb`, `*.skill`, `*.zip` binary; `.prettierignore` skips `docs/samples` so the sources keep matching the bundles.
- **Tests:** Rust (Linux): core 51, store 43, llm 45, agent 80, mcp 46 + 21 integration. New: zip round trip, unsafe paths, limits, damage, encrypted/unknown methods (archive); manifest parsing, launch lines for node/python/uv/binary, error messages, bundle files (manifest); value checks, platform names (host); extension entries in the config; end to end with `oxide-mcp-mock` packed as an extension (preview, range error, needs setup, configure → running, key only in the keychain, toggle, update keeps settings, uninstall removes everything) and bad bundles (name clash, no manifest, zip slip, not a zip, other OS, discard); the real `docs\samples\oxide-dice.mcpb` installed and run with Node (`settings_check`, `greet`); skills front matter (read and write), create/list/toggle/load/remove, folders and zips, the sample skill; the chat service loading a skill without approval and logging it; the system prompt's skills list. Vitest (run on Windows): `features/extensions/Extensions.test.tsx` (install with settings and a range error, cancel discards, warnings + update + toggle + folder + uninstall, editing settings keeps the stored key, a dropped bundle opens the dialog, helpers; skills: create, edit, switch, remove; add from zip/folder with the duplicate error and the file list) against `src/test/fakeExtensions.ts`.

Deviations from PLAN.md (and why):
- **Extensions' managed entries live under `oxide.extensions`, not in `mcpServers`:** their command line comes from the bundle's manifest (install folder, `${…}` variables) and sensitive values from the keychain, so it's built when Oxide starts rather than pasted. `mcpServers` stays 1:1 with Claude desktop. PLAN.md's decisions table records it.
- **Oxide's own zip reader** (`archive.rs` on `flate2`) instead of the `zip` crate, which isn't in the offline Linux vendor bundle.
- **Python extensions use the uv on the PATH** (Oxide's own uv sidecar comes in Phase 12), falling back to the system Python with a warning.
- **Runtime versions** (`compatibility.runtimes`, e.g. `node >=16`) are shown, not checked; only whether Node.js/uv/Python is there. `compatibility.claude_desktop` and manifest localization are ignored; remote (`https://`) icons aren't loaded.
- **Skills' on/off state is kept in the skills folder** (`.oxide-skills.json`), not in the app settings, so the settings form can't overwrite it with a stale copy.
- **Skill scripts can't run yet** (code execution is Phase 12): `load_skill` lists them and says to use them as reference.
- Folder settings with `multiple` are picked one folder at a time (or typed); there's no per-chat switch for skills in the “+” menu yet (`disabledBuiltins: ["load_skill"]` works).

Known issues / TODO carried forward:
- `pick_paths` and the Tauri glue compile only on Windows here; the Vitest suite also runs only there.
- An extension server's first start after install can take a moment (Node start-up); the card shows “Starting…”.
- Phase 18's trust prompt should also cover extensions (today the install dialog's note is the only warning).
- Small local models (Strata `swift-iq2_xs`) sometimes send malformed tool-call arguments; Oxide returns an error the model can recover from, but it may give up. A retry usually works.

Notes for Phase 11:
- `Extensions::refresh` is the way to rebuild extension servers after anything changes; the MCP manager treats them like any other server, so folder grants (roots) apply to them too.
- `Skills::load` already confines paths to a folder; the same `archive::safe_path` check is a start for the file tools' sandbox.

Verified: fmt ✅ clippy ✅ tests ✅ (cargo + Vitest 175/175) typecheck ✅ lint ✅ vite build ✅ (Windows, phase10.cmd) tauri dev on Windows ✅ (manual exit check by the user).

## Phase 9 — Desktop integration — 2026-11-07
Status: done. `scripts\win\phase9.cmd` committed `5013c0b phase 9: desktop integration` on the second run (fix below); the user then ran the whole exit check by hand on Windows: everything passed.

First Windows run (2026-11-07): all Rust checks passed (the new plugins and xcap compiled first time), the production build passed, and 1 of 167 Vitest tests failed: **Esc while recording a shortcut also closed Settings** (a real bug: Radix's dialog catches Esc in the document capture phase, before the field's handler). The recorder now takes keys through a window capture listener while recording, which runs first and stops them.

Exit criteria (checked by hand in `cargo tauri dev` on 2026-11-07; all passed):
- [x] **Quick entry works system-wide.** With another app in front (Notepad, a browser), press **Alt+Space**: a small box appears centred on that screen, in front, with the cursor in it. Type “What is 17 × 23?” and press Enter: the box hides, Oxide's window comes forward on the new chat and the reply streams. Alt+Space again, then Esc → it hides; Alt+Space, then click elsewhere → it hides; Alt+Space twice → shown, then hidden.
- [x] **Screenshot:** Alt+Space → camera button: the box disappears for a moment and comes back with a “Screenshot” chip. Send “What's on my screen?” (with a vision model): the chat shows the image.
- [x] **Shortcut setting:** Settings → Desktop → Shortcut → Change → press Ctrl+Shift+Space → “Active”; the new shortcut opens quick entry and Alt+Space no longer does. A shortcut another app holds shows the red “couldn't be registered” line. Reset to Alt + Space.
- [x] **Deep links open the right chat.** On a chat, title-bar menu (or the sidebar row's ⋯) → **Copy link**; open another chat; press Win+R, paste the `oxide://chat/<id>` link and run it: Oxide comes forward on that chat. `oxide://new?q=Hello%20there` → a new chat with “Hello there” in the composer. `oxide://settings/desktop` → Settings → Desktop. Close the window (to the tray) and try one again: the window comes back.
- [x] **Single instance.** With Oxide running, start `target\debug\oxide.exe` again (or run a deep link): no second window or tray icon; the running one comes forward.
- [x] **Tray:** the Oxide icon is in the notification area. Left click shows/hides the window; right click → Show Oxide, New chat, Quick entry, Quit Oxide all work. The window's ✕ hides it (first time: a “still running” notification); with “Keep Oxide in the tray…” off, ✕ quits.
- [x] **Notifications:** ask something long, then switch to another app before it finishes: a notification with the chat's title and the start of the reply appears. A tool that needs approval notifies too.
- [x] **Window state:** move and resize the window, quit from the tray, start again: same place and size (maximized stays maximized).
- [x] **Start at login:** Settings → Desktop → on: Task Manager → Startup apps lists Oxide (the dev binary in this build). Turn it off again.

Done:
- **`oxide-core`:** `desktop.rs`: `DesktopAction` (`openChat`, `newChat { text }`, `openSettings { section }`), `parse_deep_link` (`oxide://chat/<id>`, `oxide://new?q=…`, `oxide://settings[/<section>]`; also `oxide:x` and trailing slashes; ids limited to letters/digits/`-`/`_`, ≤ 64; percent-decoding with `+` as space; text ≤ 10,000 chars), `deep_link_in_args` (skips the program path), `clean_shortcut` (canonical modifier names and order, one key, at least one modifier), `reply_preview` (one line, ≤ 140 chars). Settings: `DesktopSettings { closeToTray (on), quickEntry (on), quickEntryShortcut ("Alt+Space"), launchAtLogin (off), notifications (on) }`; `normalized()` cleans the shortcut and falls back to the default.
- **`oxide-computer`:** `capture_screen_png(point)`: the screen containing a point (else the primary one) via `xcap` 0.9, re-encoded as PNG with the workspace `image` crate.
- **`src-tauri`:** new `desktop/` module:
  - `mod.rs`: `DesktopState` (prefs the window handlers read synchronously, shortcut status, start-at-login error, queued actions until the main window is listening), `apply_settings` (shortcut + start at login; at startup and on every `save_settings`), `show_main`/`toggle_main`/`main_in_front`, `dispatch` (shows the main window, then emits `DesktopRequest` to it or queues the action), `open_url`, `quit` (saves window state, exits), `on_window_event` (main ✕ → hide to tray, or quit when that's off; quick entry hides on blur and on Alt+F4).
  - `quick.rs`: the quick entry window (`quick`, `index.html?window=quick`, 680×148, frameless, shadow, always on top, no taskbar button), created hidden at startup; `show` (centred), `hide`, `toggle`, blur with a 400 ms grace, `begin_capture`/`end_capture`.
  - `shortcut.rs`: the global-shortcut handler (pressed → toggle), `apply` (unregister, parse, register; errors kept for Settings; skipped when nothing changed).
  - `tray.rs`: tray icon (the app icon) with Show Oxide / New chat / Quick entry / Quit Oxide; left click toggles the window.
  - `notify.rs`: notifications for `done` (chat title + reply preview), `failed` and `toolApproval` when the main window isn't visible, unminimized and focused; a one-time “still running” hint when ✕ first hides to the tray.
  - `menu.rs` (macOS only): Oxide (About, Settings… ⌘,, Services, Hide, Quit), File (New Chat ⇧⌘O, Quick Entry, Close), Edit (undo … select all), View (full screen), Window.
  - Plugins: single-instance (first; a second launch's deep link is dispatched, otherwise the window comes forward), deep-link (`oxide` scheme in `tauri.conf.json`; registered at runtime on Windows/Linux; the start-up link via `get_current`, later ones via `on_open_url`), notification, autostart (`--hidden` argument → Oxide starts in the tray), global-shortcut, window-state (size, position, maximized, full screen; not visibility or decorations; quick entry excluded). `tauri` gains the `tray-icon` feature. The main window is `visible: false` in the configs and shown by setup unless started with `--hidden`. macOS: Dock click (`RunEvent::Reopen`) shows the window.
  - Commands (`commands/desktop.rs`): `desktop_status`, `take_desktop_actions`, `show_quick_entry`, `hide_quick_entry`, `open_quick_entry_chat`, `capture_screenshot` (hides quick entry, waits 250 ms, captures the screen it was on in `spawn_blocking`, brings it back, prepares the PNG as an attachment). Event `DesktopRequest` (`events.desktopRequest`). `save_settings` applies desktop settings. Capabilities cover `main` and `quick`.
- **Frontend:**
  - `main.tsx` loads either the app or `QuickEntryRoot` (`?window=quick`) with dynamic imports, so the small window doesn't parse the whole app.
  - `features/quick-entry/`: `QuickEntryRoot` (query client, theme, tooltips; no router and no settings sync, so it never writes the main window's UI cache; it re-reads it on focus), `QuickEntry` (serif textarea, Enter sends / Shift+Enter new line / Esc hides, screenshot button and chip, model line, errors; sends with the new-chat model and composer options, then `openQuickEntryChat`). The bottom bar is a drag region.
  - `features/desktop/`: `useDesktopActions` (in `AppLayout`: listens to `desktopRequest`, then fetches queued actions; opens chats (refreshing chat lists), new chats with text (`/new` location state → `HomePage` fills its composer, also when already open), Settings), `DesktopSection` (Settings → Desktop: tray, quick entry + shortcut recorder with Active/error/Reset, start at login (+ error), notifications, the link formats, a dev-build note), `shortcut.ts` (`shortcutFromKeys` from `KeyboardEvent.code`, `formatShortcut` with ⌃⌥⇧⌘ on macOS), `queries.ts` (`useDesktopStatus`; refreshed after every settings save).
  - Chat menus (sidebar row and title bar): **Copy link** copies `oxide://chat/<id>`.
  - `lib/window.ts`: `onWindowFocusChanged`.
- **Tests:** Rust (Linux): core 48 (new: chat/new/settings links, links in arguments, shortcut cleaning, previews, action JSON, desktop settings), store 43, llm 45, agent 72, mcp 51; `oxide-computer` has a PNG round-trip test (Windows only: xcap isn't in the Linux vendor bundle). Vitest (new, run on Windows): `shortcut.test.ts`, `Desktop.test.tsx` (actions from outside, a queued start-up link, Copy link, Settings switches, recording a shortcut with a taken-shortcut error and reset, Esc cancels recording without closing Settings), `QuickEntry.test.tsx` (Enter starts a chat and hands it over, Shift+Enter, screenshot attach/send/remove/error, Esc hides). The fake backend implements the desktop commands and event (`emitDesktopRequest`, `setDesktopFaults`, `desktopActions` seed).

Deviations from PLAN.md (and why):
- **The quick entry window is created hidden at startup**, not on first use: creating a webview from the shortcut handler can deadlock on Windows, and a ready window shows instantly.
- **Screen capture only** (the screen quick entry is on); no single-window capture yet.
- **Notifications don't open the chat when clicked**: `tauri-plugin-notification` has no click callback on desktop. They show the chat title and the reply's start.
- **`oxide://new?q=` fills the composer** rather than sending at once, so a link can't send messages on the user's behalf.
- Quick entry uses the model and thinking options last picked in the main window; it has no model picker of its own.

Known issues / TODO carried forward:
- `menu.rs` and the `Reopen` handler compile only on macOS and haven't been built yet (Phase 17's CI matrix will).
- In a dev build, start at login and `oxide://` links point at `target\debug\oxide.exe`, which needs the Vite dev server; Windows notifications from a dev build may be attributed to PowerShell (no installed app id). Installers (Phase 17) register both properly.
- Alt+Space is Windows' own window-menu shortcut; Oxide's registration takes it over while Oxide runs.

Notes for Phase 10:
- `desktop::dispatch(app, DesktopAction::…)` is how backend code brings the window forward and navigates; add a variant (and a `useDesktopActions` case) for new targets such as an extension's settings page.

Verified: fmt ✅ clippy ✅ tests ✅ (cargo + Vitest 167/167) typecheck ✅ lint ✅ vite build ✅ (Windows, phase9.cmd) tauri dev on Windows ✅ (manual exit check by the user).

## Phase 8 — Artifacts — 2026-11-07
Status: done. `scripts\win\phase8.cmd` committed `4605170 phase 8: artifacts` on the second run (the first failed clippy on an unused `Manager` import in `artifact_frame.rs`, and one Vitest test clicked “Preview” before the next artifact had loaded; both fixed). The user then ran the exit check by hand on Strata `swift-iq2_xs`: everything passed.

Exit criteria (check by hand in `cargo tauri dev`; any model with tools, ideally Anthropic or a larger local model):
- [x] **Each type renders.** (Checked: React counter with Tailwind + lucide icon and working buttons; Markdown with a table; Python code; SVG; Mermaid flowchart.) Ask for each as an artifact, e.g. “Write a short Markdown guide to brewing coffee as an artifact”, “a Python script that renames photos by date”, “an SVG of a sun”, “a Mermaid flowchart of a pull request review”, “a React counter app with a lucide icon, styled with Tailwind”. While the model writes, the panel opens beside the chat and shows the source live (“Writing…”); when it's saved it switches to the preview. Each reply has a card in the thread that reopens the artifact.
- [x] **Updates create versions.** “Make the guide's title shorter.” → the same card kind shows “Edited version 2”, the panel says “Version 2 of 2”, ‹ › switches between versions. Code ↔ Preview, Copy, Download (save dialog) and Full screen (Esc leaves) work; the panel's left edge resizes it.
- [x] **HTML can't reach Tauri.** Ask: “Create an html artifact with exactly this content:” and paste
  `<!doctype html><pre id=o></pre><script>let p;try{p=String(!!parent.document)}catch(e){p='blocked ('+e.name+')'}o.textContent='__TAURI__: '+typeof window.__TAURI__+'\n__TAURI_INTERNALS__: '+typeof window.__TAURI_INTERNALS__+'\nparent.document: '+p;fetch('https://example.com').then(()=>o.textContent+='\nfetch: allowed (bad)',()=>o.textContent+='\nfetch: blocked')</script>`
  Expected: `undefined`, `undefined`, `blocked (SecurityError)`, `fetch: blocked`.
  **Result (2026-11-07):** `__TAURI__: undefined`, `parent.document: blocked (SecurityError)`, `fetch: blocked`, but `__TAURI_INTERNALS__: object`. Follow-up probes:
  - WebView2 injects Tauri's all-frames scripts into the sandboxed frame. `__TAURI_INTERNALS__` there is non-configurable and non-writable (the frame runtime can't remove it) and holds only `plugins`: no `invoke`. Tauri's IPC script (with the invoke key) is main-frame only.
  - `window.ipc` and `window.chrome.webview` exist in the frame. A hand-made `add_memory` message posted through `window.ipc.postMessage` (no invoke key) was **ignored**: Settings → Memory stayed at 0 items. The fetch-based IPC (`http://ipc.localhost`) is blocked by the frame's CSP anyway.
  - Conclusion: the frame can't call Oxide's commands. Passed.
- [x] `/artifacts` lists them across chats (filter by type, search); clicking one opens its chat with the panel; delete works.
- [x] The “+” menu's **Artifacts** switch turns the tool off for a chat.

Done:
- **`oxide-core`:** `artifact.rs`: `ArtifactKind` (markdown, code, html, svg, mermaid, react; forgiving `parse`), `Artifact`, `ArtifactVersion`, `ArtifactDetail`, `ArtifactSummary`, `ARTIFACT_TOOL`, `MAX_ARTIFACT_CHARS` (1M), `clean_key`/`key_from_title`/`clean_title`, `file_name` + extensions per language, `apply_update` (exactly one match; forgives surrounding whitespace and CRLF). `ToolUsageKind::Artifact`. `StreamEvent::ToolInputDelta { id, partialJson }`.
- **`oxide-store`:** migration `0007_artifacts.sql` (`artifacts` with `UNIQUE(conversation_id, key)`, `artifact_versions`; both cascade with the chat). `artifacts.rs`: `save_artifact_version` (one transaction: create or bump the version), `artifact_by_key`, `conversation_artifacts`, `list_artifacts` (with chat title and an excerpt), `get_artifact`, `artifact_version`, `delete_artifact`.
- **`oxide-agent`:** `artifacts.rs`: the `artifact` tool (schema ordered command, id, type, title, language, content, old_str, new_str so the title arrives first; description says when to use artifacts and what each type may do), `run_artifact` (guesses the command when missing, strips a wrapping code fence, keeps the type/title/language of earlier versions, readable errors for unknown ids, ambiguous `old_str`, missing type). The result is a text line plus an `ArtifactRef` block (id, title, version). `Builtin::Artifact`: in every tool set unless the chat switches it off (`disabledBuiltins: ["artifact"]`), never asks for approval, runs in `ChatService` like `project_search`, logged as `artifact` with the title. The system prompt says when to make one (`PromptContext.artifacts`). Tool-call arguments are now sent to the UI while they stream (`ToolInputDelta`, buffered with text every 40 ms).
- **`src-tauri`:** commands `list_artifacts`, `conversation_artifacts`, `get_artifact`, `delete_artifact`, `save_artifact_file` (save dialog). `artifact_frame.rs`: the `oxide-artifact` URI scheme (Windows: `http://oxide-artifact.localhost/`) serves `artifact-frame.html` and its assets with the frame's own CSP (inline scripts and `unsafe-eval` allowed, https scripts/styles/images allowed, `connect-src 'none'`, `form-action 'none'`, `frame-src 'none'`), `Access-Control-Allow-Origin: *` (module scripts from the opaque-origin frame are CORS requests) and any CSP `<meta>` stripped. Debug builds proxy the Vite dev server (`build.devUrl`, `reqwest`); release builds read the bundled assets. The app CSP gains `frame-src` for it.
- **Frontend:**
  - `features/artifacts/`: `ArtifactPanel` (beside the chat, resizable from its left edge, width remembered; header with kind icon, title, kind line, “Writing…/Updating…”, version switcher, Preview/Code, Copy, Download, Full screen, Close), `ArtifactCard` (inline card; “Writing…” while streaming; “Edited version n”), `ArtifactsPage` (gallery: type chips, filter, delete with confirmation, open → chat + panel), `store.ts` (Zustand: target = chat + artifact/version or a live call; dismissed live call isn't reopened), `partial.ts` (reads string fields of streaming JSON, unescaping as it goes), `kinds.ts`, `KindIcon.tsx`, `queries.ts`.
  - Renderers (`views/`): Markdown (the chat's renderer), code (`CodeView`, Shiki), SVG as an `<img>` data URL (scripts never run), Mermaid (bundled `mermaid`, `securityLevel: "strict"`, follows the theme), HTML and React in `FrameView`: `<iframe sandbox="allow-scripts allow-forms allow-modals allow-downloads">` (no `allow-same-origin`), a `ready → render → rendered | error` postMessage protocol checked against the frame's window, errors shown as a note.
  - `src/frame/` + `artifact-frame.html` (second Vite entry): removes any `__TAURI*` globals, writes HTML artifacts into the document, compiles React artifacts with bundled `@babel/standalone` (TypeScript + JSX → CommonJS), evaluates them with a `require` limited to `react`, `react-dom`, `react-dom/client`, `lucide-react`, picks the default export (or the first exported function, or an unexported `App`), styles with `@tailwindcss/browser`, and wraps it in an error boundary.
  - `useChatEvents`: opens the panel when an artifact's content starts streaming and switches it to the saved artifact on the tool result (`settle`); invalidates artifact queries. `streams.ts` appends `toolInputDelta` to the call's arguments. `ToolCallBlock` draws artifact calls as cards (failed ones as a normal line with the error). “+” menu: **Artifacts** switch. “Used in this chat” lists Artifacts.
- **Tests:** Rust (Linux): core 40, store 43, agent 72, llm 45, mcp 51. New: kinds/keys/file names/updates (core); versions per key, gallery order, cascade, limits (store); create/update/rewrite, fence stripping, errors (agent `artifacts.rs`); `artifacts_are_made_without_asking_and_stream_their_input` (chat service: no approval card, input deltas before the call, version saved with the reply's id, logged); system prompt; tool sets. `src-tauri` (Windows only): frame CSP, CSP meta stripping, allowed paths. Vitest: `partial.test.ts`, `store.test.ts`, `src/frame/run.test.ts`, `Artifacts.test.tsx` (live panel → saved preview, versions/code/copy/download/full screen, SVG/Mermaid/HTML frame protocol and sandbox, failed calls, “+” switch, gallery). The fake backend implements the artifact commands (`saveArtifact` helper).

Deviations from PLAN.md (and why):
- **The frame is served by a custom protocol, not `srcdoc`:** a `srcdoc` frame inherits the app's CSP (no inline scripts in release builds), so artifacts couldn't run their own scripts. A separate origin with its own CSP also blocks network requests. PLAN.md's decisions table now records it.
- **SVG is shown as an `<img>`** rather than sanitized markup: an SVG loaded as an image can't run scripts or load anything, which needs no sanitizer.
- **HTML artifacts may load scripts, styles and images from https URLs** (CDN libraries), but can't make requests (`connect-src 'none'`).
- **Artifacts are on by default in every chat with a tools-capable model**; the “+” menu switches them off per chat. There's no global setting yet.

Known issues / TODO carried forward:
- `__TAURI_INTERNALS__` (with only `plugins`), `window.ipc` and `chrome.webview` are visible inside the artifact frame on Windows (WebView2 injects them into every frame); commands sent through them are rejected (see the exit check). Phase 18's security review should re-test this after Tauri upgrades.
- React artifacts can import only `react`, `react-dom` and `lucide-react` (the tool description says so). Charts libraries (e.g. Recharts) aren't bundled.
- Artifacts made in a branch that was later regenerated stay in the chat's list (versions aren't tied to the visible branch).
- Errors inside HTML artifacts aren't reported back (writing the page replaces the frame's listeners); React errors are.

Notes for Phase 9:
- The artifact frame is a pattern for other untrusted content: a URI scheme handler in `src-tauri` with its own CSP, and a sandboxed `<iframe>` without `allow-same-origin`.

Verified: clippy ✅ tests ✅ (cargo + Vitest 153/153) typecheck ✅ lint ✅ vite build ✅ (Windows, phase8.cmd) tauri dev on Windows ✅ (manual exit check by the user on Strata).

## Phase 7 — Projects & memory — 2026-11-07
Status: done. `scripts\win\phase7.cmd` committed `402259b phase 7: projects & memory` on the third run (fixes below); the user then ran the exit check by hand (all five parts passed), and the fixes from that check were committed as `phase 7: fixes from the manual check`.

Fixes after the first Windows run (2026-11-07): everything passed except 2 of 133 Vitest tests.
- **“Moved to …” toast never showed** (a real bug): the menu item that started the move unmounts when the menu closes, and TanStack Query skips per-call `mutate` callbacks of unmounted components. `ChatMenuItems` now uses `mutateAsync().then(toast)`. **Don't put `onSuccess` on `mutate()` calls started from menu items.**
- A test read “No files yet” before the knowledge query had loaded; it now waits (`findByText`). The same test closes the file viewer with its Close button (Escape didn’t close it under jsdom).

Manual check by the user (2026-11-07, `402259b`, Strata `swift-iq2_xs`):
- Part 1 (instructions): the project chat answered in Danish **with thinking on**, but in English **with thinking off** (twice each); the chat outside the project answered in English. Small local models without thinking drop instructions buried mid-prompt. Fix: `<project_instructions>` moved to the very end of the system prompt (after memory), with firmer wording (every reply, including language/tone/format; only the user's own messages override them). Test: `system_prompt_carries_project_search_and_memory` checks the order.
  - Recheck with thinking off: still English with “Always answer in Danish.”; always Danish (thinking on and off) with “Regardless of the language in which the user's message is written always answer in Danish”. The model treated the English question as a reason to switch, which the first fix's wording (“only the user's own messages … can override them”) may have encouraged. The closing line now says only an *explicit* request overrides the instructions, and the language a message is written in isn't one. Rechecked after that change: the short “Always answer in Danish.” still gets English with thinking off, so this model (2-bit Qwen) needs the explicit wording; no further prompt changes. Takeaway: small local models without thinking need explicit instructions. [x] Part 1 passed.
- Part 2 (knowledge in full): all steps OK. [x] Part 2 passed.
- Part 3 (search fallback): all steps OK. **Bug:** after setting the budget back to 40,000 the Knowledge card still said “More than the knowledge budget (8.7k of 1k tokens)” (chats already used the new budget; only the card's cached query was stale). Fix: saving settings invalidates every project's knowledge query (`projectKeys.allKnowledge`, in the settings mutation's own `onSuccess` so it also runs after Settings closes). Test: the search-mode test raises the budget in Settings and checks the card. [x] Part 3 passed; card fix rechecked by the user.
- **UX (user feedback):** it wasn't clear that the budget saves on Enter. New `components/NumberSetting.tsx`: a number field that saves on Enter, blur or a **Save** button that appears with “Unsaved change · Enter saves, Esc undoes” as soon as the value is edited. Used for the knowledge budget, the thinking budget (Settings → Models) and the tool step limit (Settings → Permissions). Test in `Memory.test.tsx`.
- Part 4 (memory): all steps OK. **Glitch:** opening “Delete all memory?” showed the ✕ button's “Close” tooltip at once: Radix focuses the first tabbable element (the ✕) when nothing inside takes focus, and tooltips open on focus. Fix in `ProjectDialogs.tsx` `Modal`: `onOpenAutoFocus` focuses an element marked `data-autofocus` (Cancel in destructive confirmations), keeps a field that focused itself, else focuses the dialog. Test: the confirm opens on Cancel with no tooltip. [x] Part 4 passed.
- Part 5 (moving, archiving): all OK. [x] Part 5 passed.

Exit criteria (check by hand in `cargo tauri dev`):
- [x] **Instructions change answers:** in a project, Edit instructions → “Always answer in Danish.” A new chat there answers in Danish; a chat outside the project doesn't.
- [x] **Knowledge changes answers:** Knowledge → “+” → Add text content: “The office cat is called Ferris.” In a project chat, “What is the office cat called?” → Ferris. Outside the project the model doesn't know. (Upload a `.md`/`.pdf` too; click it to see the stored text.)
- [x] **Large knowledge falls back to search:** Settings → Memory → Knowledge budget `1000`; add `PLAN.md` to a project. The card says “Chats search the files…”. Ask “What does Phase 12b add?” → the reply shows “Searched project knowledge · …” (no approval card) and answers from the file.
- [x] **Memory can be viewed and deleted:** Settings → Memory → turn on “Remember things across chats”. In a chat: “I'm Mikkel and I build Oxide on Windows.” After the reply a “Memory updated” toast appears and the items show in Settings → Memory. A new chat: “What do you know about me?” uses them. Edit one, delete one, then Delete all.
- [x] Move a chat into a project (sidebar or title-bar menu → Move to project) and out again (project page → remove button); archive and unarchive a project.

Done:
- **`oxide-core`:**
  - `project.rs`: `Project.archived`, `ProjectPatch.archived`, `MAX_INSTRUCTIONS_CHARS` (20,000). Knowledge types: `KnowledgeKind` (text | pdf | note), `KnowledgeFile` (chars, estimated tokens, pages, note), `KnowledgeText`, `KnowledgeMode` (empty | full | search), `ProjectKnowledge` (files, total, budget, mode), `KnowledgeHit`. `estimate_tokens` (≈ 4 chars per token), `knowledge_mode`.
  - `memory.rs`: `MemoryItem` (project_id `None` = all chats; the chat it came from), `NewMemory`, `memory_prompt` (bullet list), `clean_memory_text` (one line, ≤ 500 chars).
  - Settings: `projects.knowledgeBudget` (40,000 tokens, 1,000–400,000), `memory { enabled (off), model }`. `ToolUsageKind::ProjectSearch`; `PROJECT_SEARCH` = `project_search`.
- **`oxide-store`:** migration `0006_projects_memory.sql`: `projects.archived`; `project_files` (extracted text per file); `knowledge_chunks` + FTS5 table `knowledge_fts` (external content, triggers, `unicode61 remove_diacritics 2`); `memories` (project items cascade with the project).
  - `knowledge.rs`: `chunk_text` (≈ 1,500-character passages, 200 overlap, breaking at blank lines, lines, sentences or spaces; char-safe), `any_word_query` (quoted words OR-joined, prefix match from 4 letters, English/Danish stop words dropped unless nothing else is left), `add_knowledge_file` (one transaction: file + passages; ≤ 500 files per project, ≤ 2M characters per file), `knowledge_files`, `knowledge_texts`, `knowledge_file`, `delete_knowledge_file`, `search_knowledge` (ranked by `bm25()`, scoped to the project).
  - `memory.rs`: `list_memory`, `memory_for(project)` (global first, then the project's), `add_memory` (≤ 300 items), `update_memory`, `delete_memory`, `clear_memory`.
  - `set_conversation_project` (doesn't bump `updatedAt`); `update_project` handles `archived` and the instructions limit.
- **`oxide-llm`:** `ChatRequest.system_context`: stable text ahead of the system prompt. Anthropic gets it as the first system block carrying the system breakpoint (still 4 breakpoints in total), so knowledge stays cached while the prompt after it (which has the time) changes; OpenAI-compatible servers get it joined before the system prompt.
- **`oxide-agent`:**
  - `knowledge.rs`: `read_knowledge_file` (through `AttachmentStore`: text/code as UTF-8, PDFs by their extracted text, images refused; the prepared copy is removed), `chat_budget` (the setting, at most a quarter of the model's context window), `plan_knowledge` → `Full(<project_knowledge> with <document index name>)` or `Search(file names)`, `project_search_entry`, `run_project_search`, `format_hits` (`<passage file part>`).
  - `memory.rs`: the memory prompt (what's known, project scope, what not to keep: secrets, health, passing details) and a lenient JSON parser (code fences, strings or objects, de-duplicated, ≤ 3 per exchange).
  - `ChatService`: `setup` loads the project's knowledge (full → `system_context`; search → `project_search` first in the tool list and the file names in the system prompt), and memory items when memory is on. `project_search` runs in the chat service (it knows the project), never asks for approval, and is logged as `projectSearch`. After every reply that ends normally, `learn_memory` asks the memory model (configured, or the same small-model choice as titles; 60 s timeout), saves new items and calls `EventSink::memory_changed(added)` (default no-op). Titles and memory share `quick_text`.
  - `context.rs`: `PromptContext.project_name`, `knowledge_search`; the memory text now says it's what Oxide remembers from earlier chats.
- **`src-tauri`:** commands `set_conversation_project`, `project_knowledge`, `add_project_file(projectId, source)`, `add_project_text`, `get_knowledge_file`, `delete_knowledge_file`, `list_memory`, `add_memory`, `update_memory`, `delete_memory`, `clear_memory`; event `MemoryChanged { added }` (`events.memoryChanged`).
- **Frontend:**
  - Project page: “← All projects”, options menu (Edit details, Archive/Unarchive, Delete project with confirmation), archived banner, Instructions card with an editor dialog (character count), Knowledge card (budget meter, “in full” vs “searched” line, files with icons and token counts, click to view the text, remove; “+” → Upload files via the native picker or a file input in the browser, Add text content), per-chat “Remove from project”.
  - Projects page hides archived projects behind “Archived (n)”.
  - Chat menus (sidebar row and title bar): “Move to project” submenu with a check on the current project and “Remove from project”.
  - Settings → **Memory** (no longer a placeholder): on/off, memory model, items grouped by All chats / project (edit inline, delete, link to the chat it came from), add with a scope, Delete all; Project knowledge budget.
  - `useChatEvents` listens to `memoryChanged` (refetch + “Memory updated” toast). Tool lines and “Used in this chat” know `project_search` (“Searched project knowledge · query”, folder-search icon).
  - Shared dialogs in `features/projects/ProjectDialogs.tsx` (`Modal`, `ConfirmDialog`, …); `readBase64`/`baseName` exported from `useAttachments`.
- **Tests:** Rust (Linux): core 36, store 39, llm 45, agent 68, mcp 51. New: chunking (paragraph breaks, overlap, dense non-ASCII), stop-word queries, knowledge CRUD/search/cascade, long-file passage search, memory CRUD/scope/cascade, moving chats, archive, knowledge as Anthropic system block / OpenAI join, file reading (text, PDF, image refused), full vs search plans, memory prompt + parsing, and chat-service tests: knowledge in full, search without approval (logged), memory learned and given to the next reply, memory off sends nothing. Vitest: `features/projects/Projects.test.tsx` (instructions, knowledge add/upload/view/remove, search mode, archive, delete, move chats, helpers), `features/memory/Memory.test.tsx` (toggle, add/edit/delete, background items + toast, Delete all, budget clamp, grouping), a `project_search` case in `Tools.test.tsx`. The fake backend implements the new commands (`learnMemory` emits `memoryChanged`).

Deviations from PLAN.md (and why):
- **BM25 through SQLite FTS5 (`bm25()`), not `tantivy`:** tantivy isn't in the offline Linux vendor bundle, and FTS5 is already in the database (no new crate, one transaction for file + index, cascades on delete). PLAN.md updated.
- **Memory items are saved directly**, not held as proposals for the user to accept: they're listed (with their source chat) in Settings → Memory for review, editing and deletion, and a toast says when something was learned.
- **Knowledge files store extracted text only** (no copy of the original file); images aren't accepted as knowledge.
- **The knowledge budget lives in Settings → Memory** (with memory), and a chat also caps it at a quarter of the model's context window.
- **Drag and drop onto the Knowledge card isn't supported yet** (the window-wide drop goes to the composer); upload uses the picker.

Known issues / TODO carried forward:
- Memory learning runs after every reply that ends normally. With a local model (Strata) it uses the chat model, so the server is busy for a moment after each reply. Set a memory model in Settings → Memory to move it elsewhere.
- The system prompt's time (minutes) still changes the Anthropic cache key of the dynamic prompt each minute; knowledge sits before it in its own cached block, so only the short prompt is re-read.
- Token counts are estimates (4 characters per token), not the provider's tokenizer.
- `src/bindings/index.ts` was edited by hand on the agent side; `cargo test` (export_bindings) regenerates it on Windows.

Notes for Phase 8:
- `Builtin` + a branch in `run_calls` is the pattern for built-in tools that need app state (`project_search` runs in `ChatService`); the artifact tool can follow it.
- `features/projects/ProjectDialogs.tsx` has a reusable `Modal` and `ConfirmDialog`.

Verified: clippy ✅ tests ✅ (cargo + Vitest 134/134) typecheck ✅ lint ✅ (Windows, phase7.cmd) tauri dev on Windows ✅ (manual exit check by the user on Strata; fixes above).

## Phase 6 — Agent loop, approvals & built-in tools — 2026-11-06
Status: done. `scripts\win\phase6.cmd` passed every check on Windows on the second run and committed `144a11d phase 6: agent loop, approvals & built-in tools`. The first run failed typecheck only: the generated bindings mark `ToolUse.server`/`tool` optional (`server?: string | null`, from `#[serde(default)]`), unlike the hand-edited copy; `toolView.ts` now uses `b.server ?? null`.

Manual check by the user on 2026-11-06 (screen recording), Strata `swift-iq2_xs`, prompt “please get me the current Copenhagen time and show me the files in C:/AI Apps/Oxide/docs, and finally give me a summary of this web page https://maxroll.gg/d4”:
- The model called `time` → `get_current_time`, `filesystem` → `list_directory` and `web_fetch` in one step. Two approval cards appeared (arguments, descriptions, three buttons); `web_fetch` ran without asking. After approval the calls ran.
- It chained two more steps on its own: listed the `ui-parity` subfolder (the “Allow always” choice let that run without a second card) and fetched maxroll.gg again, noticing the page is mostly JavaScript-rendered. Six tool calls over three steps, then a full answer: 21:16 Copenhagen time (UTC+01:00), the docs listing with the four PNGs, and a summary of the page.
- The chat was titled automatically (“Copenhagen Time Files and D4 Summary”), tool lines collapsed to one row each (“Read maxroll.gg”, “List Directory C:/AI Apps/Oxide/docs · filesystem”).
- Before the first output the reply showed “Working” for about 30 s (Qwen thinking on a local model; not a tool-loop delay).
- Cosmetic fix after the check: while a call's arguments stream, its line showed the raw `filesystem__list_directory`; it now shows the tool part until the server is known.
- **Bug found in the check:** clicking “Allow always for this chat” on the first card seemed to do nothing. The answer was received, but the loop collects every approval of a step before running the calls, and a card only closed when its call started (`toolRunning`), i.e. after the *last* card of the step was answered. Cards now close as soon as they're answered (`useStreams.markAnswered`, status `approved`, shown as “Allowed” until the call runs). Fixed by `scripts\win\phase6-fix.cmd`.

Exit criteria (check by hand in `cargo tauri dev`):
- [x] A model chains several MCP and web tools with approvals in one turn (Strata; see above). Suggested prompt with the `docs\mcp-test-servers.json` servers: “What time is it in Copenhagen, which files are in C:/AI Apps/Oxide/docs, and what does https://modelcontextprotocol.io say MCP is? Use your tools.” Expect approval cards for `time` and `filesystem`, a web fetch without asking, and a final answer.
  - [x] On **Strata**. [ ] On an **LM Studio** model (not run: its server wasn't started).
  - On **Anthropic** once the account has API credits (the Anthropic path is covered by tests, incl. server-side web search and `pause_turn`).
- [x] **Stop mid-loop** (checked by the user on 2026-11-06: “the stop test was a success”) while an approval card waits, and while a tool runs: the reply is saved as “Stopped.”, the call shows “Didn’t run”, and the next message works.
- [x] Settings → Permissions: “Run read-only connector tools without asking” skips the card for `readOnlyHint` tools (checked by the user on 2026-11-06). [ ] The step limit stops a reply with a note (skipped for now; covered by tests).
- [ ] Settings → Web search: pick a service (Brave/Tavily key, or a SearXNG URL), **Test search**, then ask something current. Without a service, models get `web_fetch` only (no `web_search`).
- [x] The “+” menu’s Web search switch removes both web tools from that chat (checked by the user on 2026-11-06); the right-hand panel lists “Used in this chat”.

Done:
- **`oxide-core`:**
  - `ContentBlock::ToolUse` gains `server` / `tool` (filled by the loop; older rows load with `null`). New `ServerToolUse` and `ServerToolResult { rawJson }` for provider-run tools (Anthropic web search), kept verbatim because they must go back unchanged.
  - `StopReason::PauseTurn`, `StopReason::StepLimit`. `StreamEvent::ToolCall` (arguments complete, with server/tool/title), `ToolApproval { request }`, `ToolRunning`, `ToolResult { content, isError, durationMs }`.
  - `tools.rs`: `ToolApprovalRequest`, `ApprovalDecision` (allowOnce, allowChat, deny), `ToolUsage` + `ToolUsageKind`, `WebSearchKeys`, `SearchKeyKind` (keychain names `websearch:brave|tavily`), `WebSearchTest`, constants `web_search` / `web_fetch` / `anthropic`.
  - Settings: `tools { autoAllowReadOnly (off), askBeforeWeb (off), maxSteps (25, 1–200) }`, `webSearch { provider none|brave|tavily|searxng, searxngUrl, anthropicNative (on), maxResults (5, 1–10) }`. `ChatTools.disabledBuiltins`.
- **`oxide-store`:** migration `0005_tool_loop.sql`: `chat_tool_grants` (“Allow always for this chat”, keys `server/tool` or `builtin/<name>`) and `tool_usage` (every call: kind, server, tool, label, error, reply id). `grant_chat_tool`, `chat_tool_grants`, `revoke_chat_tools`, `log_tool_use`, `tool_usage`.
- **`oxide-llm`:** `ChatRequest.server_tools` (raw JSON definitions, Anthropic only). Anthropic decoder: `server_tool_use` → `ServerToolStart` (input as `ToolUseDelta`s), any `*_tool_result` block → `ServerToolResult` (raw JSON), `pause_turn`. Request body sends `server_tool_use` / result blocks back unchanged. OpenAI-compatible: images in tool results follow the `tool` messages in a user message (tool messages are text only).
- **`oxide-agent`:**
  - `tools/mod.rs`: `McpTools` trait (implemented for `McpManager`: running servers’ tools, `call_tool` with the call id as progress token), `Tools::tool_set(chat, settings, kind)` → `ToolSet` (built-ins first, then MCP tools sorted, names `server__tool` cleaned to `[A-Za-z0-9_-]{1,64}` and de-duplicated, descriptions prefixed `[server]`). `normalize_schema`: top-level `object` with `properties`, no `$schema`/`$id`, top-level `anyOf/oneOf/allOf` folded into `properties` (both APIs reject them), `required` kept only for known properties. MCP results → blocks (text, images stored through `AttachmentStore`, resources inlined, links noted, structured content as a fallback), 100k-character cap per result. `usage_label` for the log.
  - `tools/web.rs`: `WebTools` (reqwest client, keys in the keychain). Brave (`X-Subscription-Token`), Tavily (bearer, `search_depth: basic`), SearXNG (`/search?format=json`, a readable hint when JSON is off); readable errors (401/403 key refused, 429). `web_fetch`: http/https only, 5 MB cap, HTML → Markdown, text/JSON/XML/CSV as is, PDFs through pdf-extract, images returned as image blocks; 40k characters per call with a `start` offset to read on.
  - `tools/html.rs`: forgiving HTML parser + Markdown renderer (largest `<article>`, else `<main>`, else `<body>`; drops nav/header/footer/aside/forms/scripts/hidden/cookie banners; headings, lists with nesting and `start`, links resolved against the final URL, inline code, fenced code with language, quotes, tables, images with alt text, entities).
  - `chat.rs` (the loop): one reply = one assistant message whose blocks keep the order (thinking, text, tool calls, results, more text). Each step sends history + the reply so far (`context::request_messages`). Calls are resolved, announced (`ToolCall`), checked for approval (grants, `autoAllowReadOnly` for `readOnlyHint`, `askBeforeWeb` for built-ins), then run: all read-only → in parallel, otherwise one at a time. Deny → an error result telling the model not to retry. Unknown tools and invalid JSON → error results the model can recover from. Local models that call tools but finish with `stop` still get results. `pause_turn` continues. Step limit → error results + `StepLimit`. Stop while waiting or running → open calls get “stopped” results, so the saved reply is a valid history. Usage is summed over steps. Every call (and Anthropic searches) is logged.
  - `context.rs`: `build_messages` splits earlier replies into assistant / tool-result turns, keeps call/result pairs, drops unmatched ones, drops provider tool blocks and thinking of earlier replies, puts tool results first when a user turn merges. `reply_turns` keeps signed thinking and provider blocks for the current reply. `system_prompt(PromptContext)`: weekday, local date and time with zone (from the window), OS, user name, tools available (search / fetch / connector names) with usage guidance, `<project_instructions>` (from the chat’s project), `<memory>` slot (Phase 7).
  - `ChatService::new(store, registry, sink, tools)`; `pending_approvals`, `answer_approval`, `set_client_clock`, `tools()`.
- **`src-tauri`:** `AppState` builds the MCP manager first and gives `Tools` the manager, the keychain and the attachments store. New commands in `commands/tools.rs`: `answer_tool_approval`, `pending_tool_approvals`, `tool_usage`, `chat_tool_grants`, `revoke_chat_tool_grant`, `set_client_clock`, `web_search_keys`, `set_web_search_key`, `test_web_search`.
- **Frontend:**
  - `features/chat/streams.ts`: a live reply is an ordered list of blocks (text, thinking with its own timing, tool calls with status streaming → ready → awaitingApproval → running → done/error). `liveText` / `liveThinking` helpers.
  - `features/tools/`: `ToolCallBlock` (one line: “Searched the web · query”, “Read example.com”, connector tool + server chip; status: preparing, running with MCP progress, needs approval, failed / didn’t run, duration; opens to Request and Response with images), `ApprovalCard` (tool, server, description, destructive warning, pretty arguments, Deny / Allow always for this chat / Allow once), `toolView.ts` (saved replies → segments, provider search summaries, headlines, “Used in this chat” rows), queries, `PermissionsSection`, `WebSearchSection`.
  - Saved replies render their blocks in order; stop notes for `stepLimit`. “Working” shows again after a tool finishes while the model continues.
  - Chat page: waiting approvals the window didn’t see live (after a reload) show as cards; the details panel shows the chat’s tools and “Used in this chat” (collapsed per kind with counts; click scrolls to the call).
  - “+” menu: Web search switch (per chat; switches web search and web fetch together).
  - Settings: **Permissions** (no longer a placeholder) and a new **Web search** section.
  - `useChatEvents` sends the window’s UTC offset and IANA zone (`set_client_clock`) at start and on focus.
- **Tests:** Rust (Linux): core 33, store 35, llm 44, agent 57 (new: tool loop with approval, deny + allow-always, parallel read-only, stop while waiting, step limit with unknown tool and bad JSON, local models finishing with `stop`, provider tools and `pause_turn`; tool sets, names, schemas, MCP results; Brave/Tavily/SearXNG against a local HTTP server; fetch; HTML → Markdown; history with tool turns; system prompt), mcp 50. Vitest: `features/tools/Tools.test.tsx` (inline approval → running → result → saved reply, deny + orphan approvals, “Used in this chat”, clock, helpers, Permissions, Web search key + test, “+” menu switch), updated `streams.test.ts`. The fake backend implements the new commands (`askApproval`, `finishReplyWith`, `heldReplyId`, `logToolUse`).

Deviations from PLAN.md (and why):
- **web_fetch’s readability is Oxide’s own** (`tools/html.rs`), not a crate: no HTML parser is in the offline Linux vendor bundle, and a small converter tuned for model input is testable here. Pages that need JavaScript come back nearly empty (the tool says so).
- **Built-in web tools run without asking by default** (Settings → Permissions → “Ask before web search and web fetch”). They only read public pages, like Claude desktop’s web search.
- **Approvals, changed after the user's test:** the first build asked one card at a time and ran nothing until every card of the step was answered (the web fetch waited behind two cards). Now every call of a step goes its own way at once: all cards of the step show together and can be answered in any order; calls that need no approval start right away; an allowed call starts when it's allowed. Read-only calls run side by side, others take turns (a `tokio::sync::Mutex`), so two writes never overlap. “Allow always for this chat” also answers that tool's other waiting cards (backend `answer_approval`, frontend `markToolAllowed`). Results go into the reply in call order; results that arrived before a Stop are kept. Test: `calls_of_a_step_wait_for_approval_side_by_side`. Checked by the user on 2026-11-06 (recording, light theme, `4cdc7a7`): both cards showed together, the maxroll.gg fetch ran and returned while they waited, the time call ran as soon as it was allowed while the folder card still waited, and the reply finished normally (it chained a second listing and fetch on its own).
- **Anthropic web search uses `web_search_20250305`** with `max_uses: 5` (basic search, every model; checked in the docs today). Dynamic filtering (`20260209`+) needs code execution and isn’t used. Earlier replies’ search blocks aren’t sent back (their text is), because the encrypted results and citations must otherwise be sent exactly as received.
- **Citations on Anthropic text blocks aren’t kept** (`ContentBlock::Text` has no citations yet).
- **Time zone comes from the window** (`set_client_clock`), since there is no time-zone crate in the backend.

Known issues / TODO carried forward:
- Anthropic `redacted_thinking` blocks aren’t stored, so a tool loop after redacted thinking may be rejected by the API (rare with summarized thinking).
- A reply that streams while the window reloads isn’t shown live again (as before); its waiting approvals are, as standalone cards.
- `web_fetch` has no domain allow/deny list and can reach local addresses (e.g. `http://127.0.0.1`); the user sees every fetch in the thread.
- The per-chat “Allow always” list can be read and cleared through `chat_tool_grants` / `revoke_chat_tool_grant`, but there’s no UI for it yet (Phase 11’s session panel is a good home).
- `src/bindings/index.ts` was edited by hand on the agent side; `cargo test` (export_bindings) regenerates it on Windows.

Notes for Phase 7:
- The system prompt already has slots: `PromptContext.project_instructions` (filled from the chat’s project) and `memory` (`None` today). Knowledge search can be a built-in tool next to `web_search` in `tools/mod.rs` (`Builtin` enum + `builtin_entry`).
- Tool calls are logged per chat in `tool_usage`; Phase 11’s session panel reads `tool_usage` and can extend `usageRows`.

Skipped for now by the user’s choice (covered by tests): the step limit, Settings → Web search with a real key/SearXNG, LM Studio, and Anthropic once there are API credits. Re-check them when convenient.

Verified: clippy ✅ tests ✅ typecheck ✅ lint ✅ (Windows, phase6.cmd) tauri dev on Windows ✅ (Strata tool chain with approvals, by the user).

## Phase 5 — MCP client — 2026-11-05/06
Status: done. `scripts\win\phase5.cmd` passed every check on Windows (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest) and committed `phase 5: MCP client` (`42328f1`, then `bf6cb29` with the fixes below). The user ran the exit check by hand the same evening.

Exit criteria, checked by hand in `cargo tauri dev` on 2026-11-06 (servers from `docs\mcp-test-servers.json`, pasted into Settings → Developer):
- [x] The reference "everything" server (`npx`): connects, lists tools, `echo` runs from the debug panel.
- [x] The filesystem server (`npx`): connects, lists tools, `list_directory` on `C:/AI Apps/Oxide/docs` runs.
- [x] A `uvx` Python server (`mcp-server-time`): connects over **MCP 2026-07-28** (`mcp-time 2026.11.4`), `get_current_time` returns the Copenhagen time.
- [x] A remote HTTP server: `https://mcp.deepwiki.com/mcp` connects over **MCP 2026-07-28** (DeepWiki 4.0.2) and lists its 3 tools; `ask_wiki_question` reaches the server (it validates the arguments).

Fixes after the first Windows runs (all in `bf6cb29`):
- **Bindings didn't load (Vite: "Declarations with the name `arguments` cannot be used in an ECMAScript module"):** three commands had a Rust parameter named `arguments`, which tauri-specta copies into the generated TypeScript, where `arguments` is reserved. Renamed to `input` (`mcp_call_tool`) and `values` (`mcp_get_prompt`, `mcp_attach_prompt`). **Never name a command parameter `arguments` (or another JS reserved word).**
- **Clicking Restart crashed the app** ("there is no reactor running" in `manager.rs`): `mcp_restart` was a synchronous command, which Tauri runs on the main thread outside the Tokio runtime, and `McpManager::restart` used `tokio::spawn`. The manager now keeps the runtime's handle (from `new`/`start`) and spawns through it; the command is async too. Test: `restart_works_from_a_thread_outside_the_runtime`.
- **Argument prefill for "either/or" schemas:** DeepWiki's `repoName` is `anyOf: [string, string[]]` and was prefilled as `{}`. `argumentSkeleton` now takes the first non-null `anyOf`/`oneOf` choice and skips `"null"` in type lists (`schema.test.ts`).
- **Windows paths in the JSON arguments** (`"C:\AI Apps"` with single backslashes) gave a bare "invalid escape" error from the backend. The runner now checks the JSON first and explains that backslashes must be doubled or written as `/`.
- Clippy on Windows: one collapsible `if` in the Job Object code (Windows-only, not compiled on Linux).
- A Vitest name clash ("Run echo" matched two buttons): the list button is now "Open the runner for …".

Done:
- **`oxide-core` (`mcp.rs`):** UI types for MCP. Config entry `McpServerConfig` (stdio command/args/env or http url/headers, enabled). `McpServerState` (status disabled|starting|running|error|needsAuth|stopped, era modern|legacy, protocol version, server info, capabilities, counts, restarts, signedIn). `McpTool`/`McpResource`/`McpResourceTemplate`/`McpPrompt`, `McpContent` (text, image, audio, resourceLink, resource), `McpToolResult`, `McpLogLine`, `McpInteraction` + `McpAsk` (elicitation form/URL, sampling) + `McpAnswer`, `McpEvent`, `ChatTools` (per-chat switches, `server/tool`), import preview types. JSON schemas travel as strings (like `ToolUse.inputJson`).
- **`oxide-mcp`:**
  - `jsonrpc`: messages, ids, batches, MCP error codes (-32020/-32021/-32022 mark modern servers).
  - `transport::stdio`: newline-delimited JSON on stdin/stdout, stderr to the server log, non-JSON stdout logged, pending requests failed when the process ends.
  - `transport::http`: Streamable HTTP. JSON or SSE answers (notifications before the response go to the client). Modern: `MCP-Protocol-Version`, `Mcp-Method`, `Mcp-Name` (encoded), `Mcp-Param-*` from `x-mcp-header` schema annotations (invalid tools hidden). Legacy: `Mcp-Session-Id` from `initialize`, GET stream for server messages, DELETE on close, 404 → session ended → reconnect. 401 / 403 `insufficient_scope` → one token refresh, then "sign-in required".
  - `client`: dual-era negotiation. `server/discover` first (stdio probe times out after 20 s → legacy; `UnsupportedProtocolVersion` lists → pick a common version or fall back to `initialize`; a process that dies on the probe is retried as legacy and remembered). Modern requests carry `_meta` protocol version, client info, capabilities and log level; multi round-trip (`input_required` → elicitation/sampling/roots answered → retried with `inputResponses` + `requestState`); `subscriptions/listen` for list changes. Legacy: `initialize` (2025-11-25 offered; 2025-06-18, 2025-03-26, 2024-11-05 accepted), `notifications/initialized`, `logging/setLevel info`, server→client requests (ping, roots, sampling, elicitation). Paged lists, progress tokens, cancellation (`notifications/cancelled` when a call is dropped or times out; modern HTTP closes the stream instead), ping. Timeouts: 60 s requests, 10 min tool calls.
  - `process`: command resolution through PATH + PATHEXT (`npx` → `npx.cmd`, never the extensionless script), usual install folders added to the child's PATH (Program Files\nodejs, %APPDATA%\npm, ~/.local/bin, …), friendly "install Node.js / uv" errors, `CREATE_NO_WINDOW`, Windows Job Object with KILL_ON_JOB_CLOSE (`process/windows.rs`, the only `unsafe`, allowed locally), working folder = home.
  - `log`: per-server ring buffer (1000 lines) + `<app log dir>\mcp-<name>.log` (restarted above 4 MB), forwarded to the UI.
  - `config`: `oxide_config.json` in the app config dir. `mcpServers` in Claude desktop's schema (remote: `"type": "http"`, `url`, `headers`); Oxide-only keys under top-level `oxide` (`disabledServers`, `servers.<name>.oauthClientId/oauthClientSecret`); unknown keys kept. Atomic writes; the file is polled every second for outside edits (a broken file keeps the last good servers and shows the error). Claude desktop import reads `%APPDATA%\Claude\…`, the Microsoft Store package folder, `~/Library/Application Support/Claude/…` or `~/.config/Claude/…`, read-only, with new/same/changed per entry.
  - `oauth`: OAuth 2.1 + PKCE. Protected resource metadata (from `WWW-Authenticate` or well-known), authorization server metadata (RFC 8414 / OIDC paths), dynamic client registration (or a client id from the config), loopback redirect on `127.0.0.1`, `state` + `resource` (RFC 8707), token exchange, refresh with rotation. Tokens in the OS keychain through `TokenStore`.
  - `manager`: one supervisor per server: start, run, restart with backoff (1 s doubling to 60 s, 6 failed starts in a row → error until Restart), needs-auth waits for sign-in; stop on disable/remove/quit; lists refreshed on start and on change notices; user questions (elicitation, sampling approval) with a 10-minute timeout; sampling runs through the app; config edits applied live (only changed entries restart).
  - `src/bin/oxide-mcp-mock.rs`: a stdio test server (modern, legacy, legacy that crashes or stays silent before `initialize`).
- **`oxide-store`:** migration `0004_chat_tools.sql` (`chat_tools` table) with `chat_tools`/`set_chat_tools`; `set_large`/`get_large`/`delete_large` split long secrets over several keychain entries (Windows Credential Manager caps one at 2560 bytes; OAuth tokens can be longer).
- **`oxide-agent`:** `run_sampling(registry, model, params)`: an MCP `sampling/createMessage` through Oxide's providers (text in/out).
- **`src-tauri`:** `AppState.mcp` (`McpManager`), started in the background at launch and shut down on exit. `mcp_bridge.rs`: `TauriMcpSink` (event `mcpChanged`), `AppAgent` (sampling with the chosen or first usable model, links via the opener plugin), `KeychainTokens`. Commands: `mcp_servers`, `mcp_server_lists`, `mcp_logs`, `mcp_server_config`, `mcp_save_server`, `mcp_remove_server`, `mcp_set_enabled`, `mcp_restart`, `mcp_config`, `mcp_save_config`, `mcp_open_config`, `mcp_import_preview`, `mcp_import`, `mcp_call_tool`, `mcp_read_resource`, `mcp_get_prompt`, `mcp_attach_prompt`, `mcp_attach_resource` (prompts/resources become text/image/PDF attachments through `AttachmentStore`), `mcp_pending_asks`, `mcp_answer`, `mcp_sign_in`, `mcp_sign_out`, `get_chat_tools`, `set_chat_tools`.
- **Frontend (`features/mcp/`):**
  - Settings → **Connectors**: server cards (status dot, summary, counts, protocol, errors), enable switch, Restart, Edit/Remove, Sign in/out, Add server form (local command with one argument per line and `KEY=value` env, or remote URL with `Name: value` headers), **Import from Claude desktop** dialog, details with Tools (debug runner: JSON arguments prefilled from the schema, progress, result incl. images and structured output), Resources (read, templates by URI), Prompts (try with arguments), Log.
  - Settings → **Developer**: config file editor (validates JSON, Save / Discard / Open in editor) and every server's log.
  - `McpAskDialog`: elicitation forms (string/number/integer/boolean/enum/multi-select, required checks), URL elicitation (shows the host before opening), sampling approval with model choice.
  - Composer "+" menu: Connectors submenu with real servers, per-chat switch per server and per tool (`useToolSwitches`; new chats keep a draft that is saved when the chat is created); "Add a prompt" (arguments dialog) and "Add from a connector" (resources) attach as files.
  - `useMcpEvents` (app-wide) keeps server states and logs current from `mcpChanged`.
- **Tests:** oxide-mcp 50 on Linux (33 unit, 6 stdio end-to-end against the mock in both eras incl. MRTR, server requests, probe fallback, crash-on-probe retry, cancellation; 5 HTTP against a scripted server: modern headers + SSE, legacy session + DELETE, version fallback, 401, old HTTP+SSE message; 2 OAuth end-to-end with a fake authorization server; 4 manager: start/list/call/stop, asks, live config edits, retry limit). Store 34, core 30, llm 43, agent 33. Vitest: `features/mcp/Mcp.test.tsx` (connectors list, sign-in, add server, URL validation, debug run, import, config editor, logs, elicitation, sampling, per-chat switch, attach prompt) against `src/test/fakeMcp.ts`.

Deviations from PLAN.md (and why):
- **Config file watching polls** (once a second, by size and modified time) instead of a file-system watcher: the `notify` crate isn't in the offline Linux vendor bundle, and polling one small file is cheap.
- **Oxide rewrites `oxide_config.json` with keys in alphabetical order** (serde_json without `preserve_order`; that feature needs a crate missing from the vendor bundle). Values and unknown keys are kept.
- **Process trees on macOS/Linux:** only the direct child is killed. Signalling the process group (`kill -TERM -<pgid>`) killed the shell that ran the tests in the Linux workspace, so it was removed; Windows uses the Job Object as planned.
- **The deprecated HTTP+SSE transport (2024-11-05 remote servers) isn't supported**; such servers get a message pointing to their Streamable HTTP endpoint.
- **OAuth Client ID Metadata Documents aren't used** (Oxide has no hosted client metadata URL); dynamic client registration or a configured client id instead.
- **Roots:** Oxide offers the capability and answers with an empty list until folder grants exist (Phase 11).
- **Sampling** answers with text only (no images/tools in sampling yet).

Known issues / TODO carried forward:
- Windows code paths the Linux workspace can't compile (`process/windows.rs`, `CREATE_NO_WINDOW`, `#[cfg(windows)]` blocks) are only checked by the Windows run; keep them small.
- Tools aren't given to models yet (Phase 6 builds the tool loop and uses `ChatTools`).
- Web search in the "+" menu is still a placeholder (Phase 6).
- A first `npx -y` download can take longer than the 20 s stdio probe; Oxide then treats the server as legacy for that start, and a 2026-07-28-only server fails once and is retried.
- MCP servers start with no trust prompt; Phase 18 adds a first-launch confirmation.

Notes for Phase 6:
- `McpManager::call_tool(server, tool, args, progress_token)` runs a tool; `lists(name)` has every tool's JSON schema (as a string); `ChatTools::allows(server, tool)` filters per chat; `get_chat_tools` is in the store.
- Progress arrives as `McpEvent::Progress { token }` — use the tool call id as the token.
- `McpToolAnnotations.readOnlyHint` feeds the auto-allow setting in Settings → Permissions.
- Agent-side workflow: Linux tests that start processes must run in their own session (`setsid cargo test …`), and parallel test builds can exhaust the workspace's memory; use `-j2`.

Verified: clippy ✅ tests ✅ typecheck ✅ lint ✅ (Windows, phase5.cmd) tauri dev on Windows ✅ (manual exit check by the user; fixes above).

## Phase 4 — Chat UX completeness — 2026-11-05
Status: code done and checked on Windows. The first `scripts\win\phase4.cmd` run passed fmt, prettier, clippy, cargo test (including `src-tauri` with both new plugins) and typecheck; lint and 90 of 93 Vitest tests also passed. The 3 failures were test setup only: `Markdown.test.tsx` rendered code blocks (which have tooltip buttons) without `Tooltip.Provider`. The test now wraps them; the second run passed everything (Vitest 93/93) and committed `ffa1146 phase 4: chat UX completeness`. The user tested the app the same evening (results below).

Exit criteria, checked by hand in `cargo tauri dev` on 2026-11-05 (the user: "everything worked well and looks nice", apart from the two issues under "Fixes after the manual check"):
- [x] Settings → About → **Create demo chat** (development builds only) makes a 50-message chat with code in several languages, inline and display math, tables, task lists, generated images and a PDF. It must render correctly and scroll smoothly; "Jump to latest" appears when scrolled up.
- [x] Attach a screenshot (paste), a large photo (resized, note on the chip), a PDF and a code file (drag and drop and the "+" menu). Send to Strata and to Anthropic.
- [x] Effort menu on an Anthropic model; thinking switch on Strata (off should stop Qwen's long thinking).
- [x] Branch navigator after an edit and after Retry; thumbs up/down; usage tooltip; export as Markdown and JSON from the title-bar menu; links open in the browser.
- [x] Vitest covers Markdown edge cases (`prepare.test.ts`, `Markdown.test.tsx`).

Done:
- **Local thinking test first** (`scripts\win\local-thinking.ps1`, run by `phase4-prep.cmd`; results in `logs\local-thinking.log`). Strata `swift-iq2_xs`, prompt "What is 17 + 25?":

  | Switch | Reasoning chars | Completion tokens |
  |---|---|---|
  | baseline | 80 | 37 |
  | `reasoning_effort: low` / `high` | 66 / 67 | 34 / 35 |
  | `reasoning_effort: none` | 0 | 3 |
  | `chat_template_kwargs.enable_thinking: false` | 0 | 3 |
  | `/no_think` in the prompt | 164 | 68 |

  So Strata gets on/off only. LM Studio wasn't running (untested, no thinking control).
- **`oxide-core`:**
  - `Effort` (low, medium, high, xhigh, max; `as_str` = API value). `ModelCapabilities.effort` (levels, lowest first) and `defaultEffort`. `effective_effort(requested)` falls back to the nearest lower level the model has.
  - `ThinkingStyle::Switch`: local reasoning models that can only be switched on/off through the chat template.
  - `GenerateOptions.effort: Option<Effort>`.
  - `attachment.rs`: `Attachment` (id, name, kind image|pdf|text, mediaType, stored `path`, size, originalSize, width/height, pages, `thumbnail` data URL, `note`), `AttachmentSource` (path | base64 bytes), `AttachmentLimits` + `ATTACHMENT_LIMITS` (20 files; images 30 MB in, fit 1568 px; PDFs 32 MB / 100 pages; text 1 MB), `TEXT_EXTENSIONS`, `to_block()`.
  - `Message.feedback: Option<Feedback { rating: good|bad, note, createdAt }>`. `StartConversation.attachments`.
- **`oxide-store`:** migration `0003_feedback.sql` (`messages.feedback`). `set_feedback`. `start_conversation` takes attachments (files first, then text; a files-only message is titled after the first file). `edit_user_message` keeps the original's attachments and allows empty text when files remain.
- **`oxide-llm`:**
  - `ChatRequest.effort`, `.images`, `.native_pdfs`.
  - Anthropic: `output_config: {effort}`; images replaced by a note for models without vision; PDFs sent as extracted text when the model can't read PDFs.
  - OpenAI-compatible: `reasoning_effort` (OpenAI only, low/medium/high); `chat_template_kwargs: {enable_thinking: false}` for local servers when thinking is off; PDFs as `<file name=… type="pdf, extracted text">` (replaces the Phase 3 placeholder).
  - Capabilities: Anthropic effort levels from the Models API `capabilities.effort` (default medium on Opus 5.5 / Haiku 5.5, else high, per the effort docs); OpenAI reasoning models (o1, o3, o4, gpt-5 non-chat) get low/medium/high; Strata models are `Switch`.
  - `media`: `pdf_pages` (pdf-extract on a blocking thread, panics caught), `join_pages` (`[Page n]` markers), `pdf_text_sidecar` (`<file>.pdf.txt`), `sample_pdf` (a tiny valid PDF for tests and the demo).
- **`oxide-agent`:**
  - `AttachmentStore` (`attachments.rs`): sniffs content (PDF magic, image formats, UTF-8 text) before the extension; size checks with readable errors that name the file and the limit; images kept as is when small, else resized (Lanczos3) and re-encoded (JPEG q85 on white, or PNG when really transparent; JPEG fallback above 5 MB); 160 px thumbnails; PDFs counted and their text saved in a sidecar (kept with a note if no text can be extracted); text BOM stripped, invalid UTF-8 replaced with a note. `blocks()` only accepts paths directly inside the attachments folder. `image_data_url(path, max_side)` for previews.
  - `plan_request(…, options: GenerateOptions, …)`: effort (clamped to the model's levels), no `thinking: disabled` at xhigh/max (400 on several models), 64k output cap at xhigh/max, `images`/`native_pdfs` from capabilities (Strata images left to the server), Strata thinking switch. Titles on Strata are written with thinking off.
  - `export.rs`: Markdown of one thread (thinking left out, files and tool calls listed), JSON of every branch (`format: "oxide.conversation"`, `version: 1`), safe file names.
  - `demo.rs`: the 50-message demo chat.
  - `path_to` is public (export uses it).
- **`src-tauri`:**
  - Plugins `tauri-plugin-dialog` and `tauri-plugin-opener`, used only from Rust (no JS permissions needed).
  - New commands: `pick_files`, `prepare_attachment`, `attachment_preview`, `attachment_limits`, `set_message_feedback`, `export_conversation(id, leafId, format)` (save dialog), `create_demo_conversation(model)`, `open_external(url)` (http, https, mailto only).
  - `send_message` takes `attachments`; `start_conversation` checks them. `AppState.attachments` = `<app data>\attachments`.
- **Frontend:**
  - `components/markdown/`: `prepare.ts` (LaTeX `\( \)`/`\[ \]` → `$`/`$$`, Pandoc dollar rule so "$5 and $10" stays text, code untouched; while streaming: open fences closed, unclosed `$$`/`\[` and inline code held back, half table rows held back), `Markdown.tsx` (react-markdown + GFM + math + KaTeX, `useDeferredValue` while streaming, links via `openExternal`, remote images shown as links, tables in a scroll region, no raw HTML), `CodeBlock.tsx` (language label, copy, wrap toggle; plain text first, Shiki colours after 120 ms with identical text so nothing moves), `highlight.ts` (lazy Shiki, JS regex engine, per-language chunks, LRU cache).
  - Syntax colours `--ox-shiki-*` in `tokens.css` (light/dark); prose styles for headings, ordered lists, task lists, links, blockquotes, tables, KaTeX and footnotes. KaTeX CSS imported in `main.tsx`.
  - Messages: Markdown replies; branch navigator ‹ n / m › on edited messages and regenerated replies (`siblingsOf`, `newestLeafUnder`; the chosen leaf per chat lives in `features/chat/branches.ts`, persisted, cleared after send/edit/retry); thumbs up/down with an optional "what went wrong" note; token usage tooltip; attachments above the user bubble (image thumbnails with a full-size view, file chips).
  - Chat page: the live reply subscribes to the stream store itself and message components are memoized, so older messages don't re-render 25×/s; stick-to-bottom via `ResizeObserver`; "Jump to latest"; `role="log"` thread and a polite status line.
  - Composer: attachments via the native picker, paste and drag-and-drop (`features/attachments/`: `useAttachments` prepares files at once; chips show thumbnail, size/pages, "Preparing…", errors and model warnings; Send waits for files). Limits in the "+" menu and the drop overlay. The Phase 1 "Add from a project" mock is removed (Phase 7).
  - Effort menu (Default / Low / Medium / High / Extra / Max, only the model's levels) and a thinking toggle that shows "always thinks" models as on and locked.
  - Export as Markdown / JSON in the chat menus (title bar and sidebar), with a toast (`lib/toast.ts`, `components/ui/Toaster.tsx`).
  - Settings → About: "Create demo chat" in development builds.
- **Tests:**
  - Rust library crates (Linux): core 28, store 31, llm 43, agent 33. New: effort parsing and fallback, request bodies (effort, switch, media fallbacks), real PDF extraction, attachments (resize, transparency, text, limits, PDFs, ownership), feedback and edits with files, export, demo.
  - Vitest: `prepare.test.ts` (dollars, math delimiters, code, streaming), `Markdown.test.tsx` (GFM, KaTeX, errors, code blocks, links, raw HTML, streaming), `ChatPhase4.test.tsx` (branches, feedback, usage, export, attachments, effort, jump to latest), plus updated thread/model tests. The fake backend implements every new command.

Fixes after the manual check (`scripts\win\phase4-fix.cmd`, commits when green):
- **"+" menu:** the two-line limits text overlapped the divider because menu rows had a fixed 32 px height. Rows now grow to fit (`min-h-8 py-1.5` in `components/ui/Menu.tsx`; also helps the Effort menu's two-line items).
- **"I can't translate the whole PDF in one reply":** a 2-page PDF to Qwen on Strata (128k context) got a "sample" translation. That's the model's choice, not a limit: Oxide sends no output cap to local servers. The system prompt now says attached files are read in full and translate/rewrite/convert/summarize requests are done completely unless the user asks for less. `scripts\win\pdf-check.ps1` (run by `phase4-fix.cmd`) logs the extracted text's size and shape for the latest PDFs and Strata's `/props` to `logs\pdf-check.log`, to rule out garbled extraction or a small server-side output default. Result (`fb6276d`, retested by the user in a new chat): the whole PDF was translated, and the menu fix works. `logs\pdf-check.log` showed the extraction was clean (2 pages, 3,605 characters, about 900 tokens of readable text), so the system prompt was the fix. Strata has no llama.cpp-style `/props` (it serves its web UI there), so its default output length is unknown; if a reply ever stops short, consider sending an explicit `max_tokens` to local servers.

Deviations from PLAN.md (and why):
- **Local servers get on/off only**, and only Strata (tested); see the table above.
- **Release builds unwind on panic** (`panic = "abort"` removed from `[profile.release]`): pdf-extract can panic on malformed PDFs, and the panic is caught instead of closing the app.
- **Two Tauri plugins** (dialog, opener), called from Rust commands so the frontend still goes through `@/bindings`. PLAN.md decisions table updated.
- **Shiki uses the JavaScript regex engine** (the CSP has no `wasm-unsafe-eval`) and a CSS-variables theme. PLAN.md updated.
- **Text attachments to Anthropic** stay `document` blocks with a title (as in Phase 3) rather than inline text; OpenAI-compatible models get `<file name="…">` blocks.
- **Search (Ctrl+K)** already existed from Phase 2; nothing changed.
- **"Add from a project"** mock removed from the "+" menu until Phase 7.

Known issues / TODO carried forward:
- Attachment files aren't deleted when a chat (or an unsent draft) is removed. Phase 17's data delete should sweep `<app data>\attachments` for files no message references.
- LM Studio: thinking control untested; re-run `scripts\win\local-thinking.ps1` with it running.
- OpenAI `xhigh`/`none`/`minimal` reasoning levels aren't offered (curated list is low/medium/high).
- Markdown footnote links scroll to the first matching id on the page (ids can repeat across messages).
- Edits keep the original's attachments; adding or removing files while editing isn't possible yet.
- `src/bindings/index.ts` was edited by hand on the agent side to typecheck before Windows could regenerate it; `cargo test` (export_bindings) overwrites it with the real output.

Notes for Phase 5:
- The "+" menu's Web search and Connectors items are still placeholders; wire Connectors to the MCP server manager.
- `open_external` exists for OAuth's system-browser step.
- Agent-side workflow this phase: edit in the container mirror, `tar` changed sources to `/mnt/user-data/outputs/sync-N.tgz`, commit it to `logs\`, and unpack on the device with `tar xzf … --overwrite` (plain `tar` can't replace files without delete permission). Run prettier on the device, then copy `src` back. Don't include `package.json`, `PLAN.md`, `PROGRESS.md` or `scripts` in the tarball unless the mirror's copy is current.

Verified: clippy ✅ tests ✅ (cargo + Vitest 93/93) typecheck ✅ lint ✅ (Windows, phase4.cmd) tauri dev on Windows ✅ (manual check by the user; two fixes above).

## Phase 3 — LLM layer & streaming chat — 2026-11-05
Status: done. `scripts\win\phase3.cmd` passed every check on Windows (fmt, prettier, clippy, cargo test 119/119, typecheck, lint, vitest 57/57) and committed `phase 3: LLM layer & streaming chat`. Before that, the first run showed one flaky Vitest test, which checked the thinking toggle before the model list had loaded; the test now waits for it. The manual exit checks below are still open.

Exit criteria, checked by hand in `cargo tauri dev` on 2026-11-05:
- [x] **Strata** with Qwen 3.8 Flash Next (`swift-iq2_xs`, 128k context):
  - It passed the tool probe and appears in the picker.
  - Streaming, Stop (partial text kept), Retry and edit-and-resend all work.
  - Its own reasoning stream shows as "Thought for Ns".
- [x] **Automatic titles:** "Hello there!" became "Hello Greeting" after the first reply.
- [~] **Anthropic** (partly checked):
  - Bearer auth, API version 2026-10-21 and the model list work, and the request reaches the Messages API.
  - Replies are blocked by billing: the API answered 400 "credit balance is too low". The user's claude.ai Pro plan doesn't include API usage; Console credits are separate.
  - Oxide showed this as a failed reply with Retry. Retrying the same message on Strata worked.
  - Streaming and thinking on Anthropic are still to check once there are credits.
- [ ] **OpenAI and LM Studio:** not run (no key / server this session). They use the same OpenAI-compatible client that Strata verified, and its tests cover their differences: `max_completion_tokens`, `stream_options` and LM Studio's `/api/v0/models`.

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

Fix after the first Windows run:
- **Claude Opus 5.5 always thinks:** it answers 400 to `thinking: {"type": "disabled"}`. Oxide sent that whenever the thinking toggle was off, so every Opus 5.5 chat would have failed.
  - `ModelCapabilities.thinkingOff` now comes from the Models API (`thinking.types.disabled.supported`). `disabled` is only sent when the model accepts it; otherwise the model's default applies.
  - The title request follows the same rule.
  - Source: the effort docs, https://platform.claude.com/docs/en/build-with-claude/effort.

User request (for Phase 4):
- **A thinking-effort control** like Claude desktop's (Low / Medium / High / Extra / Max). Qwen on Strata thinks for a long time on simple prompts. Added to PLAN.md Phase 4.

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
- **Effort control** (see PLAN.md Phase 4):
  - Anthropic: `output_config.effort` (`low|medium|high|xhigh|max`). Supported levels per model are in the Models API `capabilities.effort`. Defaults are `high`, or `medium` on Opus 5.5 and Haiku 5.5. Some combinations return 400: thinking `disabled` at `xhigh`/`max` (Opus 5, Haiku 5.5), and Sonnet 5.5's `between_tools` above `high`.
  - OpenAI reasoning models: `reasoning_effort`.
  - Local servers: first test what Strata and LM Studio honor (`reasoning_effort`, Qwen's `chat_template_kwargs.enable_thinking`, or a `/no_think` prompt switch). Offer only what works; at least on/off.
  - The thinking toggle's tooltip should also say when a model always thinks (`thinking: adaptive` without `thinkingOff`).
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

Verified: clippy ✅ tests ✅ typecheck ✅ lint ✅ (Windows, phase3.cmd). tauri dev on Windows ✅ (Strata end to end; Anthropic up to billing).

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

Fix after the first Windows run:
- **Claude Opus 5.5 always thinks:** it answers 400 to `thinking: {"type": "disabled"}`. Oxide sent that whenever the thinking toggle was off, so every Opus 5.5 chat would have failed.
  - `ModelCapabilities.thinkingOff` now comes from the Models API (`thinking.types.disabled.supported`). `disabled` is only sent when the model accepts it; otherwise the model's default applies.
  - The title request follows the same rule.
  - Source: the effort docs, https://platform.claude.com/docs/en/build-with-claude/effort.

User request (for Phase 4):
- **A thinking-effort control** like Claude desktop's (Low / Medium / High / Extra / Max). Qwen on Strata thinks for a long time on simple prompts. Added to PLAN.md Phase 4.

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

Fix after the first Windows run:
- **Claude Opus 5.5 always thinks:** it answers 400 to `thinking: {"type": "disabled"}`. Oxide sent that whenever the thinking toggle was off, so every Opus 5.5 chat would have failed.
  - `ModelCapabilities.thinkingOff` now comes from the Models API (`thinking.types.disabled.supported`). `disabled` is only sent when the model accepts it; otherwise the model's default applies.
  - The title request follows the same rule.
  - Source: the effort docs, https://platform.claude.com/docs/en/build-with-claude/effort.

User request (for Phase 4):
- **A thinking-effort control** like Claude desktop's (Low / Medium / High / Extra / Max). Qwen on Strata thinks for a long time on simple prompts. Added to PLAN.md Phase 4.

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

Fix after the first Windows run:
- **Claude Opus 5.5 always thinks:** it answers 400 to `thinking: {"type": "disabled"}`. Oxide sent that whenever the thinking toggle was off, so every Opus 5.5 chat would have failed.
  - `ModelCapabilities.thinkingOff` now comes from the Models API (`thinking.types.disabled.supported`). `disabled` is only sent when the model accepts it; otherwise the model's default applies.
  - The title request follows the same rule.
  - Source: the effort docs, https://platform.claude.com/docs/en/build-with-claude/effort.

User request (for Phase 4):
- **A thinking-effort control** like Claude desktop's (Low / Medium / High / Extra / Max). Qwen on Strata thinks for a long time on simple prompts. Added to PLAN.md Phase 4.

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
