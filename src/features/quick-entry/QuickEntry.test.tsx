import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeState, resetFakeBackend, setDesktopFaults } from "@/test/fakeBackend";
import { defaultUiData, useUi } from "@/stores/ui";
import { QuickEntryRoot } from "./QuickEntryRoot";

function renderQuickEntry() {
  useUi.setState({ ...defaultUiData, settingsMigrated: true });
  resetFakeBackend();
  return render(<QuickEntryRoot />);
}

describe("quick entry", () => {
  it("starts a chat on Enter and hands it to the main window", async () => {
    renderQuickEntry();
    const input = await screen.findByRole("textbox", { name: "Ask Oxide" });
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    fireEvent.change(input, { target: { value: "What's the capital of Denmark?" } });
    fireEvent.keyDown(input, { key: "Enter", code: "Enter" });
    await waitFor(() => expect(fakeState().desktop.quickChats).toHaveLength(1));
    const id = fakeState().desktop.quickChats[0];
    const chat = fakeState().conversations.find((c) => c.id === id);
    expect(chat?.title).toContain("capital of Denmark");
    expect(input).toHaveValue("");
  });

  it("Shift+Enter adds a line instead of sending", async () => {
    renderQuickEntry();
    const input = await screen.findByRole("textbox", { name: "Ask Oxide" });
    fireEvent.change(input, { target: { value: "Line one" } });
    fireEvent.keyDown(input, { key: "Enter", code: "Enter", shiftKey: true });
    expect(fakeState().desktop.quickChats).toHaveLength(0);
  });

  it("attaches a screenshot and sends it with the message", async () => {
    renderQuickEntry();
    fireEvent.click(await screen.findByRole("button", { name: "Attach a screenshot" }));
    expect(await screen.findByText("Screenshot")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retake screenshot" })).toBeInTheDocument();
    // A screenshot alone can be sent.
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(fakeState().desktop.quickChats).toHaveLength(1));
    const id = fakeState().desktop.quickChats[0];
    const first = fakeState().messages.find((m) => m.conversationId === id && m.role === "user");
    expect(first?.content.some((b) => b.type === "image")).toBe(true);
    expect(screen.queryByText("Screenshot")).not.toBeInTheDocument();
  });

  it("removes a screenshot and shows capture errors", async () => {
    renderQuickEntry();
    fireEvent.click(await screen.findByRole("button", { name: "Attach a screenshot" }));
    fireEvent.click(await screen.findByRole("button", { name: "Remove screenshot" }));
    expect(screen.queryByText("Screenshot")).not.toBeInTheDocument();

    setDesktopFaults({ screenshotFails: true });
    fireEvent.click(screen.getByRole("button", { name: "Attach a screenshot" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("couldn't be taken");
  });

  it("hides on Esc", async () => {
    renderQuickEntry();
    await screen.findByRole("textbox", { name: "Ask Oxide" });
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(fakeState().desktop.quickHidden).toBe(1));
  });
});
