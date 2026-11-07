# Oxide — Progress Log

The current phase and hand-off notes live here. Every phase chat reads this file first and appends to it last (template in PLAN.md §4).

## Status
- **Last completed:** Phase 10 — Extensions (.mcpb) & Skills (`6ccbc48`; exit check passed on Windows on 2026-11-07)
- **Current phase:** Phase 11 — Agentic tasks (Cowork-style): implemented; `scripts\win\phase11.cmd` and the manual exit check on Windows are pending
- **Next phase:** Phase 12 — Managed Python (uv) & code execution (after Phase 11's exit check)
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
- **Syncing many files into the linked folder** (added in Phase 8): without delete permission, `tar x` fails on existing files (it unlinks first); `tar xzf <file> --overwrite` writes them in place.
- **Deleting files:** `rm` in the linked folder needs delete permission (`device_request_delete_permission`). Never run git from the Linux VM; use `commit.ps1` on Windows.

## Phase 11 — Agentic tasks (Cowork-style) — 2026-11-07
Status: implemented; waiting for `scripts\win\phase11.cmd` (checks, Vitest, build, commit) and the exit check by hand on Windows. Agent-side checks passed: Rust fmt, clippy `-D warnings` and tests on Linux (core 56, store 47, llm 45, agent 98, mcp 46 + 21 integration); typecheck, lint and prettier on the device VM. Vitest and `src-tauri` (bindings export) run only on Windows.

First Windows run (2026-11-07): fmt, prettier, clippy, cargo test, typecheck, lint and the vite build passed; 4 of 186 Vitest tests failed, all in the new `Workspace.test.tsx` and all test-side:
- The “+” menu was opened with `pointerDown`, which Radix ignores under jsdom; it now opens with Enter on the trigger, like the other tests.
- “Allowed in this chat” expected a new approval to refresh the list, but the app refreshes it when an approval is answered. The test now answers a card with “Always allow this command”, and the fake backend stores real grant keys (`server/tool`, `builtin/tool`, `builtin/run_command:<command>`).
- The shell approval test didn't send the `toolUse` event that opens a tool block, and didn't wait for the held reply; both added.

Exit criteria (check by hand in `cargo tauri dev`; `phase11.ps1` makes `Documents\Oxide sample folder` with ten mixed files the first time it runs):
- [ ] **Organize a folder end to end.** New chat → “+” → **Add folder** → `Documents\Oxide sample folder` (a folder chip shows above the text). Ask: “Organize this folder by file type and write a summary.md of what's in it.”
  - The title bar shows a **Step n of m** pill and the right-hand panel opens on **Progress** with the plan; steps tick off live (in progress → done).
  - `list_dir`/`read_file`/`search_files` run without cards. Every `move_file` and `write_file` shows an approval card (“Allow always for this chat” then covers the rest of that tool's moves).
  - **Used in this session** fills in: files and folders (with a count), the task list; clicking a row scrolls to the call. **Allowed in this chat** lists the “Allow always” choices; ✕ takes one back.
  - The folder ends up sorted into subfolders with a `summary.md`.
- [ ] **Delete.** “Delete ideas.md.” → an approval card marked destructive; after Allow, the file is in `_to_delete\` inside the shared folder. Tick **Delete for good** in the panel's Folders section and repeat with another file: it's gone.
- [ ] **Path escapes are blocked.** “Read C:\Windows\win.ini” and “Read ..\..\something” → the tool answers that the path is outside the shared folders (also covered by tests: `escapes_are_refused`, `links_out_of_the_grant_are_refused`, `workspace_tools_stay_in_granted_folders_and_ask_before_changes`).
- [ ] **Shell (off by default).** Settings → Permissions → Shell commands → on; add `Get-ChildItem` to the allowlist. In the sample folder's chat: “List the files with Get-ChildItem, then run `New-Item test.txt`.” → the first runs without asking, the second asks; “Allow always” there allows only that exact command.
- [ ] **Long-running.** Start the organize prompt, then hide Oxide (✕ to the tray): an approval card notifies (Phase 9), and the sidebar row shows **Needs input** while a card waits and a pulsing dot while it works. A finished reply notifies.
- [ ] The panel's open/closed state is remembered per chat (close it in one chat, open another, come back).

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

Verified: fmt ✅ clippy ✅ tests ✅ (Linux) typecheck ✅ lint ✅ prettier ✅ (device VM) — Vitest, vite build and tauri dev on Windows pending (`phase11.cmd`).

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
