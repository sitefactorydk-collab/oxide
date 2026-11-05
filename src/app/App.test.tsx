import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useUi } from "@/stores/ui";
import { fakeState } from "@/test/fakeBackend";
import { renderApp } from "@/test/renderApp";

const ctrl = (key: string, extra: Partial<KeyboardEventInit> = {}) =>
  fireEvent.keyDown(window, { key, ctrlKey: true, ...extra });

const sidebar = () => screen.getByRole("complementary", { name: "Sidebar" });

describe("app shell", () => {
  it("redirects / to the new-chat screen with a greeting from stored settings", async () => {
    renderApp("/", { backend: { settings: { name: "Ada" } } });
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/, Ada$/),
    );
    expect(screen.getByRole("textbox", { name: "Message" })).toBeInTheDocument();
  });

  it("lists starred and recent chats from the backend", async () => {
    renderApp();
    expect(
      await within(sidebar()).findByRole("link", { name: "Tauri custom title bar on Windows" }),
    ).toBeInTheDocument();
    expect(within(sidebar()).getByText("Starred")).toBeInTheDocument();
    expect(within(sidebar()).getByText("Recents")).toBeInTheDocument();
  });

  it("shows an empty Recents section when there are no chats", async () => {
    renderApp("/new", { backend: { empty: true } });
    expect(await within(sidebar()).findByText("No chats yet")).toBeInTheDocument();
  });

  it("styles the active nav item (Tooltip must not break NavLink's className)", () => {
    renderApp("/new");
    const link = screen.getByRole("link", { name: "New chat" });
    expect(link.className).toContain("bg-active");
    expect(link.className).not.toContain("=>");
  });

  it("toggles the sidebar with Ctrl+B", () => {
    renderApp();
    expect(sidebar()).toBeInTheDocument();
    act(() => {
      ctrl("b");
    });
    expect(screen.queryByRole("complementary", { name: "Sidebar" })).not.toBeInTheDocument();
    act(() => {
      ctrl("b");
    });
    expect(sidebar()).toBeInTheDocument();
  });

  it("shows a conversation with user bubble, assistant prose and thinking block", async () => {
    renderApp("/chat/c1");
    expect(
      await screen.findByText(/Does double-click to maximize still work\?/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Thought for 6s/ })).toBeInTheDocument();
    expect(screen.getByText(/"decorations": false/)).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Message" })).toHaveAttribute(
      "placeholder",
      "Reply…",
    );
  });

  it("starts a chat from the home composer and streams the reply", async () => {
    const { location } = renderApp("/new");
    await screen.findByRole("button", { name: "Model: Claude Sonnet 5.5" });
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Hello Oxide" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(location.pathname).toMatch(/^\/chat\/c-/));
    expect(await within(sidebar()).findByRole("link", { name: "Hello Oxide" })).toBeInTheDocument();
    expect(await screen.findByText("Reply to: Hello Oxide")).toBeInTheDocument();
    const id = location.pathname.split("/")[2];
    const saved = fakeState().messages.filter((m) => m.conversationId === id);
    expect(saved.map((m) => m.role)).toEqual(["user", "assistant"]);
    // New chats use the default model: the listed Sonnet, not the stale stored id.
    expect(fakeState().conversations.find((c) => c.id === id)?.model).toBe(
      "anthropic/claude-sonnet-5-5",
    );
  });

  it("replies continue the visible thread", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Make it darker" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(await screen.findByText("Make it darker")).toBeInTheDocument();
    expect(await screen.findByText("Reply to: Make it darker")).toBeInTheDocument();
    const added = fakeState().messages.find((m) => m.role === "user" && m.parentId === "m6");
    expect(added?.content).toEqual([{ type: "text", text: "Make it darker" }]);
  });

  it("editing a message adds a branch and shows it", async () => {
    renderApp("/chat/c2");
    const original = await screen.findByText(/warm neutral palette/);
    const bubble = original.closest(".group") as HTMLElement;
    fireEvent.click(within(bubble).getByRole("button", { name: "Edit" }));
    const editor = screen.getByRole("textbox", { name: "Edit message" });
    fireEvent.change(editor, { target: { value: "Suggest a cool palette." } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Suggest a cool palette.")).toBeInTheDocument();
    expect(await screen.findByText("Reply to: Suggest a cool palette.")).toBeInTheDocument();
    // The old branch (and its reply) is hidden, but kept.
    expect(screen.queryByText(/warm neutral palette/)).not.toBeInTheDocument();
    expect(fakeState().messages.some((m) => m.id === "m5")).toBe(true);
  });

  it("Shift+Enter does not send", () => {
    renderApp("/new");
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "line one" } });
    fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
    expect(fakeState().conversations).toHaveLength(5);
  });

  it("opens search with Ctrl+K and finds chats by message text", async () => {
    const { location } = renderApp();
    await within(sidebar()).findByRole("link", { name: "Tauri custom title bar on Windows" });
    act(() => {
      ctrl("k");
    });
    const input = await screen.findByRole("textbox", { name: "Search chats" });
    fireEvent.change(input, { target: { value: "capability" } });
    const results = screen.getByRole("listbox", { name: "Results" });
    // Results arrive after the debounce; the matched term is highlighted in the snippet.
    expect(
      await within(results).findByText("capability", { selector: "mark" }),
    ).toBeInTheDocument();
    expect(within(results).getByText("Tauri custom title bar on Windows")).toBeInTheDocument();
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(location.pathname).toBe("/chat/c1"));
  });

  it("renames, stars and deletes a chat from the sidebar menu", async () => {
    renderApp("/new");
    const row = await within(sidebar()).findByRole("link", { name: "SQLite FTS5 vs tantivy" });
    // Radix menus open from the keyboard too, which jsdom handles more reliably than pointers.
    const openMenu = (name: RegExp) =>
      fireEvent.keyDown(within(sidebar()).getByRole("button", { name }), { key: "Enter" });

    openMenu(/More options for SQLite/);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Star" }));
    await waitFor(() =>
      expect(fakeState().conversations.find((c) => c.id === "c5")?.starred).toBe(true),
    );

    openMenu(/More options for SQLite/);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename" }));
    const field = await within(sidebar()).findByRole("textbox");
    fireEvent.change(field, { target: { value: "FTS notes" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(await within(sidebar()).findByRole("link", { name: "FTS notes" })).toBeInTheDocument();
    expect(fakeState().conversations.find((c) => c.id === "c5")?.title).toBe("FTS notes");

    openMenu(/More options for FTS/);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
    await waitFor(() => expect(row).not.toBeInTheDocument());
    expect(fakeState().conversations.some((c) => c.id === "c5")).toBe(false);
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

  it("shows a not-found state for unknown chats", async () => {
    renderApp("/chat/does-not-exist");
    expect(await screen.findByText("This chat doesn’t exist")).toBeInTheDocument();
  });
});

describe("projects", () => {
  it("creates a project and opens it", async () => {
    const { location } = renderApp("/projects", { backend: { empty: true } });
    expect(await screen.findByText("No projects yet")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "New project" }));
    const dialog = await screen.findByRole("dialog", { name: "New project" });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), {
      target: { value: "Garden" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Create project" }));
    await waitFor(() => expect(location.pathname).toMatch(/^\/project\/p-/));
    expect(await screen.findByRole("heading", { name: "Garden" })).toBeInTheDocument();
  });
});

describe("settings modal", () => {
  it("opens on /settings/appearance and switches theme (stored + applied)", async () => {
    renderApp("/settings/appearance");
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    expect(within(dialog).getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("radio", { name: "Dark" }));
    expect(useUi.getState().theme).toBe("dark");
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
    await waitFor(() => expect(fakeState().settings.appearance.theme).toBe("dark"));
  });

  it("applies the stored theme on start", async () => {
    renderApp("/new", { backend: { settings: { theme: "dark" } } });
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
  });

  it("keeps the background page and returns to it on close", async () => {
    const { location } = renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
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

  it("General edits the stored profile name", async () => {
    renderApp("/settings/general");
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Your name" }), {
      target: { value: "Mikkel" },
    });
    expect(useUi.getState().userName).toBe("Mikkel");
    await waitFor(() => expect(fakeState().settings.profile.name).toBe("Mikkel"));
  });

  it("imports Phase 1 preferences once when nothing is stored yet", async () => {
    renderApp("/new", { ui: { settingsMigrated: false, userName: "Old Name", theme: "dark" } });
    await waitFor(() => expect(fakeState().settings.profile.name).toBe("Old Name"));
    expect(fakeState().settings.appearance.theme).toBe("dark");
    expect(useUi.getState().settingsMigrated).toBe(true);
  });
});

describe("providers settings", () => {
  it("lists the four default endpoints", async () => {
    renderApp("/settings/providers");
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    for (const name of ["Anthropic", "OpenAI", "LM Studio", "Strata"]) {
      expect(await within(dialog).findByRole("region", { name })).toBeInTheDocument();
    }
    expect(within(dialog).getByText("http://127.0.0.1:47831/v1")).toBeInTheDocument();
  });

  it("saves a key without ever showing it, then tests the connection", async () => {
    renderApp("/settings/providers");
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    fireEvent.click(await within(dialog).findByRole("button", { name: "Edit Anthropic" }));
    const form = within(dialog).getByRole("form", { name: "Edit Anthropic" });
    fireEvent.click(within(form).getByRole("button", { name: "Test connection" }));
    expect(await within(form).findByText("Anthropic needs an API key.")).toBeInTheDocument();

    fireEvent.change(within(form).getByLabelText("API key"), {
      target: { value: "sk-ant-secret-0123456789wxyz" },
    });
    fireEvent.click(within(form).getByRole("button", { name: "Save" }));
    const card = await within(dialog).findByRole("region", { name: "Anthropic" });
    expect(await within(card).findByText("Key saved (••••wxyz)")).toBeInTheDocument();
    expect(dialog.textContent).not.toContain("0123456789");
    expect(fakeState().keys.get("anthropic")).toBe("sk-ant-secret-0123456789wxyz");

    fireEvent.click(within(card).getByRole("button", { name: "Edit Anthropic" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Test connection" }));
    expect(await within(dialog).findByText("Connected. 2 models available.")).toBeInTheDocument();
  });

  it("adds an OpenAI-compatible endpoint with defaults that follow the type", async () => {
    renderApp("/settings/providers");
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    fireEvent.click(await within(dialog).findByRole("button", { name: "Add endpoint" }));
    const form = within(dialog).getByRole("form", { name: "New endpoint" });
    fireEvent.change(within(form).getByLabelText("Type"), { target: { value: "lmstudio" } });
    expect(within(form).getByLabelText("Base URL")).toHaveValue("http://127.0.0.1:1234/v1");
    fireEvent.change(within(form).getByLabelText("Name"), { target: { value: "Desktop GPU" } });
    fireEvent.change(within(form).getByLabelText("Base URL"), {
      target: { value: "http://192.168.1.20:1234/v1/" },
    });
    fireEvent.click(within(form).getByRole("button", { name: "Save" }));
    const card = await within(dialog).findByRole("region", { name: "Desktop GPU" });
    expect(within(card).getByText("http://192.168.1.20:1234/v1")).toBeInTheDocument();
  });
});
