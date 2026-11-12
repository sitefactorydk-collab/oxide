import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { compactHeldReply, fakeState, finishReply } from "@/test/fakeBackend";
import { renderApp } from "@/test/renderApp";

async function send(text: string) {
  const box = screen.getByRole("textbox", { name: "Message" });
  fireEvent.change(box, { target: { value: text } });
  fireEvent.keyDown(box, { key: "Enter" });
  expect(await screen.findByText(text)).toBeInTheDocument();
}

describe("context management", () => {
  it("the composer's meter shows the fill and Compact now adds a divider", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/iron-oxide accent/);
    const meter = await screen.findByRole("button", {
      name: /^Context: \d+% of 32\.8K tokens used$/,
    });
    fireEvent.click(meter);
    const pop = await screen.findByRole("dialog");
    expect(within(pop).getByText("Context window")).toBeInTheDocument();
    expect(within(pop).getByText(/not reported, so Oxide assumes it/)).toBeInTheDocument();
    expect(within(pop).getByRole("meter", { name: "Context used" })).toBeInTheDocument();
    fireEvent.click(within(pop).getByRole("button", { name: "Compact now" }));

    const divider = await screen.findByRole("note", { name: "Conversation compacted" });
    expect(divider).toHaveTextContent("Summarized on request");
    expect(fakeState().compactions).toHaveLength(1);
    expect(fakeState().compactions[0]?.messageId).toBe("m6");
    await waitFor(() =>
      expect(screen.getByText("Earlier messages are summarized to save room.")).toBeInTheDocument(),
    );
    // The summary opens from the divider.
    fireEvent.click(within(divider).getByRole("button", { name: /Conversation compacted/ }));
    expect(await within(divider).findByText("The chat so far, summarized.")).toBeInTheDocument();
  });

  it("a reply compacted while it runs shows where, live and after it's saved", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    await send("Research everything");
    const live = await screen.findByLabelText("Reply in progress");
    expect(await within(live).findByText("Partial")).toBeInTheDocument();
    act(() => {
      compactHeldReply("c2");
    });
    const divider = await within(live).findByRole("note", { name: "Conversation compacted" });
    expect(divider).toHaveTextContent("98K → 6K tokens");
    act(() => finishReply("c2"));
    await screen.findByText("Partial Reply to: Research everything");
    expect(await screen.findByRole("note", { name: "Conversation compacted" })).toBeInTheDocument();
  });

  it("cleared tool results show as their own note", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    await send("Research more");
    const live = await screen.findByLabelText("Reply in progress");
    expect(await within(live).findByText("Partial")).toBeInTheDocument();
    act(() => {
      compactHeldReply("c2", "- web_fetch https://en.wikipedia.org/wiki/A", "cleared");
    });
    const note = await within(live).findByRole("note", { name: "Old tool results cleared" });
    expect(note).toHaveTextContent("Replaced by short notes to save room");
    fireEvent.click(within(note).getByRole("button", { name: /Old tool results cleared/ }));
    expect(await within(note).findByText(/web_fetch/)).toBeInTheDocument();
  });

  it("Settings → Models: compaction switch, threshold and a model's window", async () => {
    renderApp("/settings/models");
    const toggle = await screen.findByRole("switch", { name: "Compact long chats automatically" });
    fireEvent.click(toggle);
    await waitFor(() => expect(fakeState().settings.context.compaction).toBe(false));

    const threshold = screen.getByRole("spinbutton", {
      name: "Compact at (% of the context window)",
    });
    fireEvent.change(threshold, { target: { value: "150" } });
    fireEvent.keyDown(threshold, { key: "Enter" });
    await waitFor(() => expect(fakeState().settings.context.threshold).toBe(95));

    const list = screen.getByRole("region", { name: "LM Studio models" });
    fireEvent.click(within(list).getAllByRole("button", { name: "Context: unknown" })[0]!);
    const field = within(list).getByRole("spinbutton", {
      name: "Context window of qwen/qwen3-14b",
    });
    fireEvent.change(field, { target: { value: "131072" } });
    fireEvent.click(within(list).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(fakeState().settings.context.windows).toEqual([
        { model: "lmstudio/qwen/qwen3-14b", tokens: 131072 },
      ]),
    );
    expect(within(list).getByRole("button", { name: "131.1K context (set)" })).toBeInTheDocument();
  });
});
