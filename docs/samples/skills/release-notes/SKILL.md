---
name: release-notes
description: Writes release notes for a software version in Oxide's house format. Use when the user asks for release notes, a changelog entry or "what's new" text for a version.
license: MIT
---

# Release notes

Write release notes in this exact format. First read `template.md` from this skill (load_skill
with path `template.md`) and follow its layout.

Rules:

1. The title is `## <Product> <version> — <one-line theme>`.
2. Group changes under the headings from the template, in that order. Leave out empty groups.
3. One line per change, starting with a verb in the past tense ("Added", "Fixed", "Removed").
4. Mention breaking changes under **Heads-up** with what the user must do.
5. Keep it under 200 words.
6. End with the line `— Brewed in Oxide.` on its own, exactly like that.
