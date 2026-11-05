import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useMock } from "@/features/mock/store";
import { useUi } from "@/stores/ui";
import { renderApp } from "@/test/renderApp";

vi.mock("@/bindings", () => ({
  commands: {
    ping: vi.fn().mockResolvedValue({
      message: "pong, Oxide",
      appName: "Oxide",
      appVersion: "0.1.0",
      coreVersion: "0.1.0",
    }),
  },
}));

const ctrl = (key: string, extra: Partial<KeyboardEventInit> = {}) =>
  fireEvent.keyDown(window, { key, ctrlKey: true, ...extra });

describe("app shell", () => {
  it("redirects / to the new-chat screen with a greeting that uses the profile name", () => {
    renderApp("/", { userName: "Ada" });
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/, Ada$/);
    expect(screen.getByRole("textbox", { name: "Message" })).toBeInTheDocument();
  });

  it("lists starred and recent chats in the sidebar", () => {
    renderApp();
    const sidebar = screen.getByRole("complementary", { name: "Sidebar" });
    expect(within(sidebar).getByText("Starred")).toBeInTheDocument();
    expect(within(sidebar).getByText("Recents")).toBeInTheDocument();
    expect(
      within(sidebar).getByRole("link", { name: "Tauri custom title bar on Windows" }),
    ).toBeInTheDocument();
  });

  it("styles the active nav item (Tooltip must not break NavLink's className)", () => {
    renderApp("/new");
    const link = screen.getByRole("link", { name: "New chat" });
    expect(link.className).toContain("bg-active");
    expect(link.className).not.toContain("=>");
  });

  it("toggles the sidebar with Ctrl+B", () => {
    renderApp();
    expect(screen.getByRole("complementary", { name: "Sidebar" })).toBeInTheDocument();
    act(() => {
      ctrl("b");
    });
    expect(screen.queryByRole("complementary", { name: "Sidebar" })).not.toBeInTheDocument();
    act(() => {
      ctrl("b");
    });
    expect(screen.getByRole("complementary", { name: "Sidebar" })).toBeInTheDocument();
  });

  it("shows a conversation with user bubble, assistant prose and thinking block", () => {
    renderApp("/chat/c1");
    expect(screen.getByText(/Does double-click to maximize still work\?/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Thought for 6s/ })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Message" })).toHaveAttribute(
      "placeholder",
      "Reply…",
    );
  });

  it("sends from the home composer, shows the stop button, then the mock reply", async () => {
    vi.useFakeTimers();
    try {
      const { location } = renderApp("/new");
      const box = screen.getByRole("textbox", { name: "Message" });
      fireEvent.change(box, { target: { value: "Hello Oxide" } });
      fireEvent.keyDown(box, { key: "Enter" });
      expect(location.pathname).toMatch(/^\/chat\//);
      expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
      await act(async () => {
        vi.advanceTimersByTime(2500);
      });
      expect(screen.queryByRole("button", { name: "Stop" })).not.toBeInTheDocument();
      expect(screen.getByText(/placeholder reply/)).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("Shift+Enter does not send", () => {
    renderApp("/new");
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "line one" } });
    fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    expect(useMock.getState().streaming).toBeNull();
    expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
  });

  it("opens search with Ctrl+K and finds chats by message text", async () => {
    const { location } = renderApp();
    act(() => {
      ctrl("k");
    });
    const input = await screen.findByRole("textbox", { name: "Search chats" });
    fireEvent.change(input, { target: { value: "capability" } });
    const results = screen.getByRole("listbox", { name: "Results" });
    expect(within(results).getByText("Tauri custom title bar on Windows")).toBeInTheDocument();
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(location.pathname).toBe("/chat/c1"));
  });

  it("opens the shortcuts dialog with Ctrl+/", async () => {
    renderApp();
    act(() => {
      ctrl("/");
    });
    expect(await screen.findByRole("dialog", { name: "Keyboard shortcuts" })).toBeInTheDocument();
  });

  it("navigates to a new chat with Ctrl+Shift+O", () => {
    const { location } = renderApp("/chats");
    act(() => {
      ctrl("O", { shiftKey: true });
    });
    expect(location.pathname).toBe("/new");
  });

  it("shows a not-found state for unknown chats", () => {
    renderApp("/chat/does-not-exist");
    expect(screen.getByText("This chat doesn’t exist")).toBeInTheDocument();
  });
});

describe("settings modal", () => {
  it("opens on /settings/appearance and switches theme", async () => {
    renderApp("/settings/appearance");
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    expect(within(dialog).getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("radio", { name: "Dark" }));
    expect(useUi.getState().theme).toBe("dark");
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
  });

  it("keeps the background page and returns to it on close", async () => {
    const { location } = renderApp("/chat/c2");
    act(() => {
      ctrl(",");
    });
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    // The chat stays rendered behind the modal.
    expect(screen.getByText(/warm neutral palette/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Close settings" }));
    await waitFor(() => expect(location.pathname).toBe("/chat/c2"));
  });

  it("About shows the backend version from the ping command", async () => {
    renderApp("/settings/about");
    expect(await screen.findByTestId("about-version")).toHaveTextContent(
      "Version 0.1.0 · core 0.1.0",
    );
  });

  it("General edits the profile name used by the greeting", async () => {
    renderApp("/settings/general");
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Your name" }), {
      target: { value: "Mikkel" },
    });
    expect(useUi.getState().userName).toBe("Mikkel");
  });
});
