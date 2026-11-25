import { describe, expect, it } from "vitest";
import { editCounts, editDiffRows, fileEditsOf } from "./editBlocks";

describe("edit blocks as a diff", () => {
  it("reads edit_file, app_edit_file and artifact updates in their shapes", () => {
    const list = fileEditsOf({
      tool: "edit_file",
      server: null,
      inputJson: JSON.stringify({ path: "a.css", edits: [{ old_str: "a", new_str: "b" }] }),
    });
    expect(list).toEqual({ path: "a.css", blocks: [{ old: "a", new: "b", replaceAll: false }] });
    expect(
      fileEditsOf({
        tool: "edit_file",
        server: null,
        inputJson: JSON.stringify({ path: "a.css", old_str: "x", new_str: "y" }),
      })?.blocks,
    ).toEqual([{ old: "x", new: "y", replaceAll: false }]);
    expect(
      fileEditsOf({
        tool: "app_edit_file",
        server: null,
        inputJson: JSON.stringify({
          path: "App.tsx",
          edits: [{ old_text: "1", new_text: "2", replace_all: true }],
        }),
      })?.blocks,
    ).toEqual([{ old: "1", new: "2", replaceAll: true }]);
    expect(
      fileEditsOf({
        tool: "artifact",
        server: null,
        inputJson: JSON.stringify({ command: "update", id: "notes", edits: [{ old_str: "a" }] }),
      }),
    ).toEqual({ path: "notes", blocks: [{ old: "a", new: "", replaceAll: false }] });
    // Not edits: a rewrite, another server's tool, arguments still streaming.
    expect(
      fileEditsOf({ tool: "artifact", server: null, inputJson: '{"command":"rewrite","id":"x"}' }),
    ).toBeNull();
    expect(
      fileEditsOf({ tool: "edit_file", server: "fs", inputJson: '{"old_str":"a"}' }),
    ).toBeNull();
    expect(
      fileEditsOf({ tool: "edit_file", server: null, inputJson: '{"path":"a","ed' }),
    ).toBeNull();
  });

  it("shows only the changed lines with a little context", () => {
    const rows = editDiffRows([
      { old: "a\nb\nc\nd\ne\nf", new: "a\nb\nc\nD\ne\nf", replaceAll: false },
    ]);
    expect(rows).toEqual([
      { kind: "hunk", text: "Edit" },
      { kind: "same", text: "b" },
      { kind: "same", text: "c" },
      { kind: "remove", text: "d" },
      { kind: "add", text: "D" },
      { kind: "same", text: "e" },
      { kind: "same", text: "f" },
    ]);
    expect(
      editCounts([
        { old: "x", new: "y\nz", replaceAll: false },
        { old: "gone", new: "", replaceAll: true },
      ]),
    ).toEqual({ added: 2, removed: 2 });
    expect(editDiffRows([{ old: "q", new: "r", replaceAll: true }])[0]).toEqual({
      kind: "hunk",
      text: "Edit · every place",
    });
  });
});
