# Security in Oxide

What Oxide protects, how, and where the tests are. Written for the Phase 18 security review;
update it when a boundary changes.

## Threat model in one paragraph

Oxide runs models that call tools, and it runs programs (MCP servers, Python, a browser) with
the user's rights. The main risks are: a model (or a prompt injection in a page, file or tool
result) making Oxide do something the user didn't want; content in the web view (model output,
artifacts) reaching Oxide's IPC; another program on the computer changing Oxide's config to run
its own command; and secrets ending up in logs, exports or the web view. Oxide does **not**
defend against malware already running as the same user. That malware can read the keychain
and the data folder anyway.

## Boundaries

### 1. The web view and IPC
- **Per-window capabilities** (`src-tauri/capabilities/*.json`, checked by `src-tauri/src/acl.rs`
  tests). `build.rs` declares every command to Tauri, so each window may call only what its
  capability lists:
  - the main window: every Oxide command, plus the few core APIs the shell uses;
  - quick entry: five commands;
  - the computer-use bar: two commands.
- No window gets `core:default` or a plugin permission, and no remote origin gets IPC.
- **CSP** (`tauri.conf.json`, test `the_content_security_policy_stays_tight`): `script-src 'self'`,
  `object-src 'none'`, `worker-src 'none'`, `base-uri 'self'`, `form-action 'none'`.
- **Artifacts** run in an `<iframe>` sandboxed without `allow-same-origin`, from the
  `oxide-artifact` scheme with their own CSP (`connect-src 'none'`). Their opaque origin gets no
  IPC (Phase 8, 17).
- **Paths from the window** (`src-tauri/src/chosen.rs`): commands that grant a folder to models
  or install from disk (`resolve_folder`, `preview_extension`, `add_skill`) accept a path only
  if it came from Oxide's own picker or a drop on the window. Any other path gets a native
  confirmation dialog that page script can't click. Test: `chosen_folders_cover_their_contents_only`.
- Other commands still trust their arguments. Paths are checked inside the commands (see 2).

### 2. Files models can touch
- File tools (`crates/oxide-agent/src/tools/files.rs`) resolve every path through
  `Sandbox::resolve`: the existing part is resolved by the OS (`..`, symlinks, junctions,
  letter case), and the result must lie inside a folder the user granted that chat.
- Tests:
  - `escapes_are_refused` and `links_out_of_the_grant_are_refused`;
  - the fuzz test `generated_paths_never_leave_the_grant`. It builds thousands of paths from
    `..`, separators, absolute paths, a sibling folder's name, links and odd names (`con`,
    `%2e%2e`, `æ ø`). It checks that whatever resolves lies inside the grant, and that no
    write, move or delete through those paths touches anything outside.
- **Docker sandbox**: a container per chat, with no network by default and only `/workspace`
  mounted (Phase 12b).
- **Managed Python** (`run_python`) is **not sandboxed**. It always asks first.

### 3. Tools and approvals
- Tools that change things ask first. The approval card offers Allow once, Allow for this
  chat or Deny.
- Computer use asks twice: once for the chat (never skipped, even in scheduled runs), then per
  app at action time.
  - It refuses Oxide itself, password managers, and Windows' credential and UAC screens.
  - It never types into password, PIN or card fields, and never types card numbers or IBANs.
  - Ctrl+Alt+Esc and the bar's Stop end it at once (Phase 16).

### 4. MCP servers
- **Trust prompts for local commands** (`crates/oxide-mcp/src/trust.rs`):
  - A stdio server whose command line Oxide hasn't run before waits in `needsTrust`, and
    Settings → Connectors shows **Trust and start**. This covers servers added by editing
    `oxide_config.json` by hand or by another program, and servers changed since they were
    last trusted.
  - Oxide remembers a SHA-256 fingerprint of command, arguments and environment per server
    name, in `<data>/mcp-trust.json`. That file is outside the config, so a program that edits
    the config can't also mark its server trusted.
  - Saves in Settings, the Claude desktop import (after its confirmation) and desktop
    extensions (which have their own install dialog) are trusted as they are saved.
  - The first start without the file trusts the servers already configured, so upgrading
    doesn't stop existing servers.
  - Tests: `new_or_changed_local_commands_wait_until_trusted`,
    `fingerprints_cover_command_args_and_env`, and `a_damaged_file_trusts_nothing_and_no_file_trusts_all`.
  - Vitest: "holds a command added outside Oxide until it is trusted".
- Remote servers use OAuth. Their tokens are kept in the keychain.

### 5. The Chrome bridge
- It listens on localhost only and speaks WebSocket (its own RFC 6455 code) with **token
  pairing** (Phase 14):
  - A connection's first message must be `pair`, with the six-word code Oxide shows, or
    `hello`, with the 256-bit token a pairing gave. Anything else, or silence for 10 seconds,
    closes it.
  - A code is valid for 5 minutes and is withdrawn after 5 wrong tries.
- Fuzz tests (`crates/oxide-bridge`):
  - `random_and_mutated_frames_never_panic`;
  - `unmasked_oversized_and_broken_frames_are_refused`;
  - `garbage_from_many_clients_never_breaks_the_bridge`, where 130 clients send garbage, and
    afterwards pairing and calls still work.

### 6. Secrets
- API keys, MCP sign-ins and extension secrets live in the **OS keychain** (`keyring`), never
  in the database, the config or the web view's storage.
  - Windows: Credential Manager.
  - macOS: Keychain.
  - Linux: the **Secret Service** (D-Bus; GNOME Keyring or KWallet). It falls back to the
    kernel keyutils store when no Secret Service is running, and keyutils forgets keys at
    reboot. Keys saved by Oxide ≤ 0.1.1 in keyutils move to the Secret Service the next time
    they are read (Phase 18).
- **Redaction in logs** (`crates/oxide-core/src/redact.rs`, `src-tauri/src/logging.rs`):
  - Every log line, MCP server stderr lines included, goes through `redact_secrets` before it
    is written to stderr or `<logs>/oxide.log`.
  - It hides: known key prefixes (`sk-`, `sk-ant-`, `gsk_`, `AIza`, `ghp_`, …), `Bearer`/`Basic`
    credentials, JWTs, and values of secret-named keys (`api_key=`, `"token":`,
    `Authorization:`, `?access_token=`, `PASSWORD=`).
  - It errs on the side of hiding too much.
  - Tests are in both files.
- **Export all** replaces every `env` and `headers` value in the exported config and leaves
  out keychain contents and tokens (Phase 17). **Delete all** removes the keychain entries
  too.

### 7. Updates
- Update bundles are minisign-signed. The public key is in `tauri.conf.json`; the private key
  is only in the user profile and the GitHub secret. Local builds don't look for updates
  (Phase 17).
- Installers are **not code-signed** yet, so Windows SmartScreen warns and macOS needs
  right-click → Open.

## Known limitations
- `run_python` runs with the user's rights (it asks first, every time).
- A local MCP server runs with the user's rights once trusted. Trust covers its command line,
  not what the program does.
- Commands other than those in 1 trust their arguments from the main window. The main
  window's own script is trusted.
- Linux without a Secret Service (a bare window manager, a container) stores keys in keyutils,
  which forgets them at reboot.
- npm dependencies: see the triage in `PROGRESS.md` (Phase 18).

## Reporting
Open a private security advisory on GitHub (`sitefactorydk-collab/oxide` → Security →
Report a vulnerability) rather than a public issue.
