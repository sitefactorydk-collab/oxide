# Phase 18 — work in progress (paused 2026-11-15 11:55 at the user's weekly usage limit)

Resume: "Continue Phase 18 of PLAN.md for Oxide. Read PHASE18-WIP.md first."
Nothing is committed yet; all files below are in the working tree on Windows.

## Scope agreed with the user
Hardening + all three stretch items (usage dashboard, HTML export, voice dictation via an
OpenAI-compatible /audio/transcriptions endpoint). Danish: scaffolding + main shell only.

## Done (agent side; Linux: clippy -D warnings + every library crate's tests green; tsc, ESLint, Prettier green on the device)
- core: `redact.rs` (secret redaction: key prefixes, Bearer/Basic, JWT, secret-named pairs), `usage.rs` (UsageReport by provider/model/day, local days), settings `appearance.language` (UiLanguage system|en|da) and `dictation` (DictationSettings), `McpStatus::NeedsTrust` + `McpServerState.trustChanged`.
- mcp: `trust.rs` (SHA-256 fingerprint of command/args/env per name in `<data>/mcp-trust.json`; first start seeds existing servers), manager holds untrusted/changed stdio commands in NeedsTrust; Settings saves/imports/editor saves auto-trust; extensions trusted; `McpManager::trust`. ServerLog lines redacted. `process::own_memory_bytes()` (Windows K32GetProcessMemoryInfo, Linux VmRSS; windows-sys feature Win32_System_ProcessStatus added). Test `new_or_changed_local_commands_wait_until_trusted`.
- store: Linux Secret Service behind feature `secret-service` (sync-secret-service + crypto-rust; keyutils fallback; old keyutils keys migrate on read) — enabled by src-tauri; can't be compiled in the Linux harness (dbus crates not vendored) → first real build is CI Ubuntu. `usage_records()`.
- llm: `transcribe.rs` (multipart by hand) + tests.
- agent: `markdown_html.rs` (safe MD→HTML) + `export_html` (self-contained, CSP meta, data: images); fuzz test `generated_paths_never_leave_the_grant`.
- bridge: fuzz tests (ws frames, 130 garbage clients then pairing still works).
- src-tauri: `logging.rs` (redacting writer → stderr + `<logs>/oxide.log`, closed before Delete all), `chosen.rs` (paths from pickers/drops; resolve_folder / preview_extension / add_skill confirm others in a native dialog), commands `mcp_trust_server`, `usage_report`, `app_ready`, `app_metrics`, `transcribe_audio`, ExportFormat::Html; capabilities/default.json updated. NOT compiled yet (Windows only).
- frontend: `src/i18n` (en/da, useT, html lang), Sidebar/ProfileMenu/Home/Composer/Settings nav translated, Appearance → Language; Connectors "Trust and start" card; Settings → Voice + composer mic (`features/dictation`); Settings → Usage (`features/usage`); About start-up time + memory; chat menu "Export as web page"; long chats mount the newest 40 messages + content-visibility (`features/chat/messageWindow.ts`). Bindings edited by hand. Tests: `features/usage/Phase18.test.tsx`, Mcp/ChatPhase4 additions, messageWindow.test.ts — not run yet (Vitest is Windows only).

## Still to do
1. acl.rs test in a scratch crate (commands vs capabilities) — or just let Windows check it.
2. Write `scripts\win\phase18.cmd/.ps1` (stop dev app, npm run bindings, check.ps1, vite build, commit with logs\commit-msg-phase18.txt, dev.cmd); user double-clicks; fix failures.
3. docs/SECURITY.md (review: capabilities, chosen paths, trust prompts, redaction, fuzz tests).
4. Triage every unchecked box / known issue in PROGRESS.md (Phase 16 exit gaps, Phase 17 macOS/Linux deferred) as the exit requires; update PLAN.md decisions (Secret Service, i18n, trust file) and PROGRESS.md (repo + project), then commit "phase 18: hardening & polish".
5. Manual checks on Windows: trust card (add a server by editing oxide_config.json), Danish shell, Usage page, HTML export opens in a browser, dictation with OpenAI/Groq, About numbers / startup in logs\oxide.log, a 200-message chat scrolls smoothly (demo chat).
