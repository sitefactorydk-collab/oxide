import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SkillSavePreview, ToolApprovalRequest } from "@/bindings";
import type { ToolCallView } from "@/features/chat/streams";
import { callHeadline, savedSkillFolder, skillSlug } from "@/features/tools/toolView";
import { askApproval, emitStream, fakeState, heldReplyId } from "@/test/fakeBackend";
import { fakeSkill } from "@/test/fakeExtensions";
import { fakeMcpState } from "@/test/fakeMcp";
import { renderApp } from "@/test/renderApp";

async function send(text: string) {
  const box = screen.getByRole("textbox", { name: "Message" });
  fireEvent.change(box, { target: { value: text } });
  fireEvent.keyDown(box, { key: "Enter" });
  expect(await screen.findByText(text)).toBeInTheDocument();
}

const NEW_SKILL: SkillSavePreview = {
  name: "weekly-report",
  folder: "weekly-report",
  isNew: true,
  skillMd:
    "---\nname: weekly-report\ndescription: Use for the weekly report\n---\n\n1. Read notes.md.\n",
  diff: null,
  files: [
    {
      path: "templates/report.md",
      bytes: 12,
      isNew: true,
      content: "# Week\n- …\n",
      diff: null,
    },
  ],
  notes: [],
};

const EDIT: SkillSavePreview = {
  name: "haiku",
  folder: "haiku",
  isNew: false,
  skillMd: "---\nname: haiku\ndescription: Haiku\n---\n\nThree lines.\nNo rhymes.\n",
  diff: "--- SKILL.md\n+++ SKILL.md\n@@ -5,1 +5,2 @@\n Three lines.\n+No rhymes.\n",
  files: [],
  notes: ["You added this skill from a folder. Saving changes Oxide's copy in the skills folder."],
};

function request(
  messageId: string,
  skill: SkillSavePreview,
  inputJson: string,
  toolUseId = "k1",
): ToolApprovalRequest {
  return {
    conversationId: "c2",
    messageId,
    toolUseId,
    server: null,
    tool: "save_skill",
    title: "Save skill",
    description: "Create a new skill for the user, or change one of their skills.",
    inputJson,
    readOnly: false,
    destructive: false,
    skill,
    computer: null,
    requestedAt: Date.now(),
  };
}

function startCall(id: string, inputJson: string) {
  emitStream("c2", { type: "toolUse", id, name: "save_skill" });
  emitStream("c2", {
    type: "toolCall",
    id,
    name: "save_skill",
    server: null,
    tool: "save_skill",
    title: "Save skill",
    inputJson,
  });
}

describe("models saving skills", () => {
  it("shows the whole new skill and offers no “Allow always”", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    await send("Turn what we just did into a skill");
    const live = await screen.findByLabelText("Reply in progress");
    const rid = heldReplyId("c2");
    const input = '{"name":"Weekly Report","description":"Use for the weekly report"}';
    act(() => {
      startCall("k1", input);
      askApproval(request(rid, NEW_SKILL, input));
    });
    const card = await screen.findByRole("group", { name: "Approve Save skill" });
    expect(card).toHaveTextContent("Save a new skill weekly-report?");
    expect(within(card).getByLabelText("SKILL.md")).toHaveTextContent("1. Read notes.md.");
    expect(card).toHaveTextContent("templates/report.md");
    expect(within(card).queryByRole("button", { name: /Allow always/ })).not.toBeInTheDocument();
    expect(within(live).getByText("Creating skill")).toBeInTheDocument();
    fireEvent.click(within(card).getByRole("button", { name: "Allow once" }));
    await waitFor(() =>
      expect(fakeState().answers).toEqual([
        { conversationId: "c2", toolUseId: "k1", decision: "allowOnce" },
      ]),
    );

    // Saved: the line links to the skill in Settings.
    fakeSkill("weekly-report", "Use for the weekly report", "1. Read notes.md.");
    act(() =>
      emitStream("c2", {
        type: "toolResult",
        id: "k1",
        content: [{ type: "text", text: 'Created the skill "weekly-report".' }],
        isError: false,
        durationMs: 5,
      }),
    );
    expect(await within(live).findByText("Created skill")).toBeInTheDocument();
    fireEvent.click(
      within(live).getByRole("button", { name: "Open weekly-report in Settings → Skills" }),
    );
    const dialog = await screen.findByRole("dialog", { name: "weekly-report" });
    expect(within(dialog).getByLabelText("Instructions (Markdown)")).toHaveValue(
      "1. Read notes.md.",
    );
  });

  it("shows what a change does, with notes, and can be denied", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    await send("Also: no rhymes");
    const rid = heldReplyId("c2");
    const input = '{"skill":"haiku","instructions":"Three lines.\\nNo rhymes."}';
    act(() => {
      startCall("k2", input);
      askApproval(request(rid, EDIT, input, "k2"));
    });
    const card = await screen.findByRole("group", { name: "Approve Save skill" });
    expect(card).toHaveTextContent("Change the skill haiku?");
    expect(card).toHaveTextContent("You added this skill from a folder.");
    const diff = within(card).getByLabelText("Changes to SKILL.md");
    const added = within(diff).getByText("+No rhymes.");
    expect(added).toHaveAttribute("data-diff", "add");
    fireEvent.click(within(card).getByRole("button", { name: "Deny" }));
    await waitFor(() =>
      expect(fakeState().answers).toEqual([
        { conversationId: "c2", toolUseId: "k2", decision: "deny" },
      ]),
    );
  });

  it("can be switched off for a chat in the + menu", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    const trigger = await screen.findByRole("button", { name: "Add files, context and tools" });
    fireEvent.keyDown(trigger, { key: "Enter" });
    const item = await screen.findByRole("menuitemcheckbox", { name: /Save skills/ });
    expect(item).toHaveAttribute("aria-checked", "true");
    fireEvent.click(item);
    await waitFor(() =>
      expect(fakeMcpState().chatTools.get("c2")?.disabledBuiltins).toEqual(["save_skill"]),
    );
  });

  it("names the line by what the call does", () => {
    const call = (inputJson: string, status: ToolCallView["status"]): ToolCallView =>
      ({
        id: "x",
        name: "save_skill",
        server: null,
        tool: "save_skill",
        title: "Save skill",
        inputJson,
        status,
        result: null,
        isError: status === "error",
        durationMs: null,
        approval: null,
      }) as unknown as ToolCallView;
    expect(callHeadline(call('{"name":"Weekly Report!"}', "done"))).toEqual({
      action: "Created skill",
      detail: "weekly-report",
    });
    expect(callHeadline(call('{"skill":"haiku"}', "running"))).toEqual({
      action: "Updating skill",
      detail: "haiku",
    });
    expect(callHeadline(call('{"skill":"haiku"}', "error")).action).toBe("Save skill");
    expect(skillSlug("  --PDF__tools-- ")).toBe("pdf-tools");
    expect(savedSkillFolder('{"name":"Æøå"}')).toBe("");
  });
});
