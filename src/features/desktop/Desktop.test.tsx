import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { emitDesktopRequest, fakeState, setDesktopFaults } from "@/test/fakeBackend";
import { renderApp } from "@/test/renderApp";

describe("desktop actions", () => {
  it("opens chats, new chats with text, and settings when asked from outside", async () => {
    const { location } = renderApp("/new");
    await screen.findByRole("textbox", { name: "Message" });

    act(() => emitDesktopRequest({ type: "openChat", conversationId: "c1" }));
    await waitFor(() => expect(location.pathname).toBe("/chat/c1"));

    act(() => emitDesktopRequest({ type: "newChat", text: "Hello from a link" }));
    await waitFor(() => expect(location.pathname).toBe("/new"));
    expect(await screen.findByRole("textbox", { name: "Message" })).toHaveValue(
      "Hello from a link",
    );

    // Also while the new chat page is already open.
    act(() => emitDesktopRequest({ type: "newChat", text: "Second link" }));
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Message" })).toHaveValue("Second link"),
    );

    act(() => emitDesktopRequest({ type: "openSettings", section: "desktop" }));
    await waitFor(() => expect(location.pathname).toBe("/settings/desktop"));
    expect(await screen.findByRole("dialog", { name: "Settings" })).toBeInTheDocument();
  });

  it("runs actions that arrived before the window was listening (a start-up link)", async () => {
    const { location } = renderApp("/new", {
      backend: { desktopActions: [{ type: "openChat", conversationId: "c1" }] },
    });
    await waitFor(() => expect(location.pathname).toBe("/chat/c1"));
    expect(fakeState().desktop.pending).toHaveLength(0);
  });
});

describe("chat links", () => {
  it("copies an oxide:// link to a chat from its menu", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    renderApp("/new");
    const sidebar = screen.getByRole("complementary", { name: "Sidebar" });
    await within(sidebar).findByRole("link", { name: "SQLite FTS5 vs tantivy" });
    fireEvent.keyDown(within(sidebar).getByRole("button", { name: /More options for SQLite/ }), {
      key: "Enter",
    });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Copy link" }));
    const id = fakeState().conversations.find((c) => c.title === "SQLite FTS5 vs tantivy")?.id;
    expect(writeText).toHaveBeenCalledWith(`oxide://chat/${id}`);
    expect(await screen.findByText("Link copied")).toBeInTheDocument();
  });
});

describe("Settings → Desktop", () => {
  it("saves the switches", async () => {
    renderApp("/settings/desktop");
    const tray = await screen.findByRole("switch", {
      name: "Keep Oxide in the tray when its window is closed",
    });
    expect(tray).toHaveAttribute("aria-checked", "true");
    fireEvent.click(tray);
    await waitFor(() => expect(fakeState().settings.desktop.closeToTray).toBe(false));

    fireEvent.click(screen.getByRole("switch", { name: "Start Oxide when you log in" }));
    await waitFor(() => expect(fakeState().settings.desktop.launchAtLogin).toBe(true));

    fireEvent.click(screen.getByRole("switch", { name: "Notifications" }));
    await waitFor(() => expect(fakeState().settings.desktop.notifications).toBe(false));

    fireEvent.click(screen.getByRole("switch", { name: "Quick entry" }));
    await waitFor(() => expect(fakeState().settings.desktop.quickEntry).toBe(false));
    expect(screen.queryByRole("button", { name: "Quick entry shortcut" })).not.toBeInTheDocument();
  });

  it("records a new shortcut, shows when it can't be registered, and resets it", async () => {
    renderApp("/settings/desktop");
    const box = await screen.findByRole("button", { name: "Quick entry shortcut" });
    expect(box).toHaveTextContent("Alt + Space");
    expect(await screen.findByText("Active")).toBeInTheDocument();

    setDesktopFaults({ shortcutTaken: true });
    fireEvent.click(box);
    const recording = screen.getByRole("button", { name: "Press the new shortcut" });
    // Modifiers alone don't finish it; Esc would cancel.
    fireEvent.keyDown(recording, { key: "Control", code: "ControlLeft", ctrlKey: true });
    expect(recording).toHaveTextContent("Press keys…");
    fireEvent.keyDown(recording, { key: "K", code: "KeyK", ctrlKey: true, shiftKey: true });
    await waitFor(() =>
      expect(fakeState().settings.desktop.quickEntryShortcut).toBe("Ctrl+Shift+K"),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Ctrl+Shift+K couldn't be registered",
    );

    setDesktopFaults({ shortcutTaken: false });
    fireEvent.click(screen.getByRole("button", { name: "Reset to Alt + Space" }));
    await waitFor(() => expect(fakeState().settings.desktop.quickEntryShortcut).toBe("Alt+Space"));
    expect(await screen.findByText("Active")).toBeInTheDocument();
  });

  it("cancels recording with Esc", async () => {
    renderApp("/settings/desktop");
    fireEvent.click(await screen.findByRole("button", { name: "Quick entry shortcut" }));
    const recording = screen.getByRole("button", { name: "Press the new shortcut" });
    fireEvent.keyDown(recording, { key: "Escape", code: "Escape" });
    expect(screen.getByRole("button", { name: "Quick entry shortcut" })).toBeInTheDocument();
    // The settings dialog stays open.
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();
    expect(fakeState().settings.desktop.quickEntryShortcut).toBe("Alt+Space");
  });
});
