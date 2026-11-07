# Samples for extensions and skills (Phase 10)

| File | What it is |
|---|---|
| `oxide-dice.mcpb` | A desktop extension: a dependency-free Node.js MCP server (`oxide-dice/`) with tools `roll_dice`, `greet` and `settings_check`, and one setting of each kind (text, number with a range, switch, sensitive token). Needs Node.js 16+. |
| `oxide-dice/` | Its source. `settings_check` reports which settings arrived (the token only by length). |
| `release-notes.skill` | A zipped skill (`skills/release-notes/`): release notes in a fixed format that ends with `— Brewed in Oxide.`, with a `template.md` the instructions tell the model to read. |
| `skills/release-notes/` | Its source; Settings → Skills → **Add folder…** takes this folder too. |

Rebuilding the bundles after editing the sources (any zip tool works; the files must sit at the
top of the archive for `.mcpb`, and in one folder for `.skill`):

```powershell
cd docs\samples
Compress-Archive -Force -Path oxide-dice\* -DestinationPath oxide-dice.zip; Move-Item -Force oxide-dice.zip oxide-dice.mcpb
Compress-Archive -Force -Path skills\release-notes -DestinationPath release-notes.zip; Move-Item -Force release-notes.zip release-notes.skill
```

The public `mcpb` CLI (`npx @anthropic-ai/mcpb pack oxide-dice`) makes the same `.mcpb`.
`crates/oxide-mcp/tests/extensions.rs` and `crates/oxide-agent/src/skills.rs` test both files.
