import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ContentBlock } from "@/bindings";
import {
  emitStream,
  fakeState,
  finishReplyWith,
  heldReplyId,
  saveArtifact,
} from "@/test/fakeBackend";
import { fakeMcpState } from "@/test/fakeMcp";
import { renderApp } from "@/test/renderApp";

vi.mock("mermaid", () => ({
  default: {
    initialize: vi.fn(),
    render: vi.fn(async () => ({ svg: "<svg><text>Diagram drawn</text></svg>" })),
  },
}));

async function send(text: string) {
  const box = screen.getByRole("textbox", { name: "Message" });
  fireEvent.change(box, { target: { value: text } });
  fireEvent.keyDown(box, { key: "Enter" });
  expect(await screen.findByText(text)).toBeInTheDocument();
}

/** A held reply in c2, ready for tool events. */
async function heldReply(text = "Make me something") {
  renderApp("/chat/c2", { backend: { replies: "hold" } });
  // The demo chat can take a while to appear when every test file runs at once.
  await screen.findByText(/warm neutral palette/, undefined, { timeout: 5000 });
  await send(text);
  const live = await screen.findByLabelText("Reply in progress");
  await within(live).findByText("Partial");
  return { live, rid: heldReplyId("c2") };
}

function toolUse(id: string, input: object): ContentBlock {
  return {
    type: "toolUse",
    id,
    name: "artifact",
    inputJson: JSON.stringify(input),
    server: null,
    tool: "artifact",
  };
}

function toolResult(id: string, ref: ContentBlock): ContentBlock {
  return {
    type: "toolResult",
    toolUseId: id,
    content: [{ type: "text", text: "Created artifact" }, ref],
    isError: false,
  };
}

/** Ends c2's held reply with one artifact call per (input, ref). */
function finishWith(calls: Array<[string, object, ContentBlock]>) {
  const content: ContentBlock[] = [{ type: "text", text: "Here it is." }];
  for (const [id, input, ref] of calls) content.push(toolUse(id, input), toolResult(id, ref));
  act(() => {
    finishReplyWith("c2", content);
  });
}

const panel = () => screen.findByRole("complementary", { name: "Artifact" });

describe("artifacts in a chat", () => {
  it("open in the panel while they're written, then show the saved artifact", async () => {
    const { live, rid } = await heldReply("Write meeting notes");
    act(() => {
      emitStream("c2", { type: "toolUse", id: "a1", name: "artifact" });
      emitStream("c2", {
        type: "toolInputDelta",
        id: "a1",
        partialJson: '{"command":"create","id":"notes","type":"markdown","title":"Meeting notes",',
      });
      emitStream("c2", {
        type: "toolInputDelta",
        id: "a1",
        partialJson: '"content":"# Agenda\\n- budget',
      });
    });
    const card = await within(live).findByRole("button", { name: "Open artifact Meeting notes" });
    expect(card).toHaveTextContent("Writing…");
    const p = await panel();
    expect(within(p).getByRole("heading", { name: "Meeting notes" })).toBeInTheDocument();
    expect(p).toHaveTextContent("Writing…");
    // Still arriving: shown as source.
    expect(within(p).getByLabelText("markdown source")).toHaveTextContent("# Agenda - budget");

    const ref = saveArtifact({
      conversationId: "c2",
      key: "notes",
      kind: "markdown",
      title: "Meeting notes",
      content: "# Agenda\n\n- budget\n- hiring",
      messageId: rid,
    });
    act(() => {
      emitStream("c2", {
        type: "toolInputDelta",
        id: "a1",
        partialJson: '\\n- hiring"}',
      });
      emitStream("c2", {
        type: "toolCall",
        id: "a1",
        name: "artifact",
        server: null,
        tool: "artifact",
        title: "Artifact",
        inputJson:
          '{"command":"create","id":"notes","type":"markdown","title":"Meeting notes","content":"# Agenda\\n- budget\\n- hiring"}',
      });
      emitStream("c2", { type: "toolRunning", id: "a1" });
      emitStream("c2", {
        type: "toolResult",
        id: "a1",
        content: [{ type: "text", text: "Created artifact" }, ref],
        isError: false,
        durationMs: 3,
      });
    });
    // Saved: the panel switches to the stored artifact's preview.
    expect(await within(p).findByRole("heading", { name: "Agenda" })).toBeInTheDocument();
    expect(within(p).getByText("hiring")).toBeInTheDocument();
    expect(p).not.toHaveTextContent("Writing…");
    expect(within(p).getByRole("button", { name: "Preview" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await waitFor(() => expect(card).toHaveTextContent("Document"));

    // Closing doesn't reopen it; the card does.
    fireEvent.click(within(p).getByRole("button", { name: "Close artifact" }));
    expect(screen.queryByRole("complementary", { name: "Artifact" })).not.toBeInTheDocument();
    fireEvent.click(card);
    expect(await panel()).toBeInTheDocument();
  });

  it("switch versions, show code, copy and download", async () => {
    const { rid } = await heldReply();
    const input = { command: "create", id: "main", type: "code", language: "rust" };
    saveArtifact({
      conversationId: "c2",
      key: "main",
      kind: "code",
      language: "rust",
      title: "main.rs",
      content: "fn main() { one(); }",
      messageId: rid,
    });
    const v2 = saveArtifact({
      conversationId: "c2",
      key: "main",
      kind: "code",
      language: "rust",
      title: "main.rs",
      content: "fn main() { two(); }",
      messageId: rid,
    });
    finishWith([["a2", { ...input, command: "rewrite", title: "main.rs" }, v2]]);

    const card = await screen.findByRole("button", { name: "Open artifact main.rs" });
    expect(card).toHaveTextContent("Code · Rust · Rewrote version 2");
    fireEvent.click(card);
    const p = await panel();
    expect(await within(p).findByText("Version 2 of 2")).toBeInTheDocument();
    expect(within(p).getByLabelText("rust source")).toHaveTextContent("two();");
    // Code has no preview toggle.
    expect(within(p).queryByRole("button", { name: "Preview" })).not.toBeInTheDocument();
    expect(within(p).getByRole("button", { name: "Next version" })).toBeDisabled();
    fireEvent.click(within(p).getByRole("button", { name: "Previous version" }));
    expect(await within(p).findByText("Version 1 of 2")).toBeInTheDocument();
    expect(within(p).getByLabelText("rust source")).toHaveTextContent("one();");

    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    fireEvent.click(within(p).getByRole("button", { name: "Copy" }));
    expect(writeText).toHaveBeenCalledWith("fn main() { one(); }");

    fireEvent.click(within(p).getByRole("button", { name: "Download" }));
    await waitFor(() =>
      expect(fakeState().artifactDownloads).toEqual([{ id: v2.artifactId, version: 1 }]),
    );
    expect(await screen.findByText(/Saved to C:/)).toBeInTheDocument();

    fireEvent.click(within(p).getByRole("button", { name: "Full screen" }));
    expect(within(p).getByRole("button", { name: "Exit full screen" })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(within(p).getByRole("button", { name: "Full screen" })).toBeInTheDocument();
  });

  it("render documents, SVG, Mermaid, and HTML in a sandboxed frame", async () => {
    const { rid } = await heldReply();
    const mk = (key: string, kind: "markdown" | "svg" | "mermaid" | "html", content: string) =>
      saveArtifact({
        conversationId: "c2",
        key,
        kind,
        title: `${kind} art`,
        content,
        messageId: rid,
      });
    const svg = mk(
      "pic",
      "svg",
      '<svg xmlns="http://www.w3.org/2000/svg"><script>x()</script></svg>',
    );
    const mm = mk("flow", "mermaid", "graph TD; A-->B");
    const page = mk("page", "html", "<h1>Hello</h1><script>parent.x = 1</script>");
    finishWith([
      ["s1", { id: "pic", type: "svg" }, svg],
      ["s2", { id: "flow", type: "mermaid" }, mm],
      ["s3", { id: "page", type: "html" }, page],
    ]);

    fireEvent.click(await screen.findByRole("button", { name: "Open artifact svg art" }));
    let p = await panel();
    const img = await within(p).findByRole("img", { name: "svg art" });
    // As an image, the SVG's scripts never run.
    expect(img.getAttribute("src")).toMatch(/^data:image\/svg\+xml;charset=utf-8,/);
    fireEvent.click(within(p).getByRole("button", { name: "Code" }));
    expect(within(p).getByLabelText("xml source")).toHaveTextContent("<script>x()</script>");

    fireEvent.click(screen.getByRole("button", { name: "Open artifact mermaid art" }));
    p = await panel();
    fireEvent.click(await within(p).findByRole("button", { name: "Preview" }));
    expect(await within(p).findByText("Diagram drawn")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Open artifact html art" }));
    p = await panel();
    const frame = (await within(p).findByTitle("html art")) as HTMLIFrameElement;
    expect(frame.getAttribute("src")).toBe("/artifact-frame.html");
    const sandbox = frame.getAttribute("sandbox") ?? "";
    expect(sandbox).toContain("allow-scripts");
    expect(sandbox).not.toContain("allow-same-origin");
    expect(sandbox).not.toContain("allow-top-navigation");

    // The frame says it's ready; the app sends it the page. Errors come back as a note.
    const win = frame.contentWindow as Window;
    const post = vi.spyOn(win, "postMessage").mockImplementation(() => {});
    act(() => {
      window.dispatchEvent(
        new MessageEvent("message", {
          data: { source: "oxide-artifact", type: "ready" },
          source: win,
        }),
      );
    });
    expect(post).toHaveBeenCalledWith(
      {
        source: "oxide",
        type: "render",
        kind: "html",
        content: "<h1>Hello</h1><script>parent.x = 1</script>",
      },
      "*",
    );
    // Messages from anything but the frame are ignored.
    act(() => {
      window.dispatchEvent(
        new MessageEvent("message", {
          data: { source: "oxide-artifact", type: "error", message: "spoofed" },
        }),
      );
    });
    expect(within(p).queryByText("spoofed")).not.toBeInTheDocument();
    act(() => {
      window.dispatchEvent(
        new MessageEvent("message", {
          data: { source: "oxide-artifact", type: "error", message: "x is not defined" },
          source: win,
        }),
      );
    });
    expect(await within(p).findByRole("alert")).toHaveTextContent("x is not defined");
  });

  it("show failed calls as a normal tool line with the error", async () => {
    await heldReply();
    act(() => {
      finishReplyWith("c2", [
        toolUse("e1", { command: "update", id: "nope", old_str: "a", new_str: "b" }),
        {
          type: "toolResult",
          toolUseId: "e1",
          content: [{ type: "text", text: "There's no artifact with id “nope” in this chat." }],
          isError: true,
        },
        { type: "text", text: "Sorry." },
      ]);
    });
    const line = await screen.findByRole("button", { name: /^Artifact/ });
    expect(line).toHaveTextContent("Failed");
    fireEvent.click(line);
    expect(await screen.findByText(/no artifact with id/)).toBeInTheDocument();
  });

  it("can be switched off for a chat in the + menu", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    const trigger = await screen.findByRole("button", { name: "Add files, context and tools" });
    fireEvent.keyDown(trigger, { key: "Enter" });
    const item = await screen.findByRole("menuitemcheckbox", { name: "Artifacts" });
    expect(item).toHaveAttribute("aria-checked", "true");
    fireEvent.click(item);
    await waitFor(() =>
      expect(fakeMcpState().chatTools.get("c2")?.disabledBuiltins).toEqual(["artifact"]),
    );
  });
});

describe("the artifacts page", () => {
  it("lists, filters, opens and deletes artifacts", async () => {
    const { location, client } = renderApp("/artifacts");
    expect(await screen.findByText("No artifacts yet")).toBeInTheDocument();
    act(() => {
      saveArtifact({
        conversationId: "c2",
        key: "plan",
        kind: "markdown",
        title: "Launch plan",
        content: "# Plan\nShip it",
      });
      saveArtifact({
        conversationId: "c4",
        key: "lt",
        kind: "code",
        language: "rust",
        title: "lifetimes.rs",
        content: "fn longest<'a>() {}",
      });
    });
    await act(() => client.invalidateQueries({ queryKey: ["artifacts"] }));
    const list = await screen.findByRole("list", { name: "Artifacts" });
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(2));
    expect(list).toHaveTextContent("Design tokens for light and dark");
    expect(list).toHaveTextContent("Code · Rust");

    fireEvent.click(screen.getByRole("button", { name: "Code" }));
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(1));
    fireEvent.click(screen.getByRole("button", { name: "All" }));
    fireEvent.change(screen.getByRole("searchbox", { name: "Filter artifacts" }), {
      target: { value: "ship" },
    });
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(1));
    expect(list).toHaveTextContent("Launch plan");

    fireEvent.click(screen.getByRole("button", { name: "Delete Launch plan" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete artifact?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(fakeState().artifacts).toHaveLength(1));

    fireEvent.change(screen.getByRole("searchbox", { name: "Filter artifacts" }), {
      target: { value: "" },
    });
    fireEvent.click(await screen.findByRole("button", { name: /^lifetimes\.rs/ }));
    await waitFor(() => expect(location.pathname).toBe("/chat/c4"));
    const p = await panel();
    expect(await within(p).findByLabelText("rust source")).toHaveTextContent("fn longest");
  });
});
