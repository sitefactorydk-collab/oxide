import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { MemoryItem } from "@/bindings";
import { fakeState, learnMemory } from "@/test/fakeBackend";
import { renderApp } from "@/test/renderApp";
import { groupMemory } from "./queries";

const saved = () => screen.findByRole("region", { name: "Saved memory" });

describe("Settings → Memory", () => {
  it("turns memory on and adds, edits and deletes items", async () => {
    renderApp("/settings/memory");
    const toggle = await screen.findByRole("switch", { name: "Remember things across chats" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    fireEvent.click(toggle);
    await waitFor(() => expect(fakeState().settings.memory.enabled).toBe(true));

    fireEvent.change(screen.getByRole("textbox", { name: "New memory" }), {
      target: { value: "Prefers answers in Danish" },
    });
    const scope = screen.getByRole("combobox", { name: "Where it applies" });
    await within(scope).findByRole("option", { name: "Oxide" });
    fireEvent.change(scope, { target: { value: "p1" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    const list = await saved();
    expect(await within(list).findByText("Prefers answers in Danish")).toBeInTheDocument();
    expect(within(list).getByRole("heading", { name: "Oxide" })).toBeInTheDocument();
    expect(fakeState().memory[0]?.projectId).toBe("p1");

    fireEvent.click(within(list).getByRole("button", { name: "Edit “Prefers answers in Danish”" }));
    const field = within(list).getByRole("textbox", { name: "Memory text" });
    fireEvent.change(field, { target: { value: "Prefers answers in English" } });
    fireEvent.click(within(list).getByRole("button", { name: "Save" }));
    expect(await within(list).findByText("Prefers answers in English")).toBeInTheDocument();
    expect(fakeState().memory[0]?.text).toBe("Prefers answers in English");

    fireEvent.click(
      within(list).getByRole("button", { name: "Delete “Prefers answers in English”" }),
    );
    await waitFor(() => expect(fakeState().memory).toHaveLength(0));
  });

  it("shows items learned in the background and deletes all after confirming", async () => {
    renderApp("/settings/memory");
    const list = await saved();
    act(() => learnMemory([{ text: "Uses Windows 11" }, { text: "Builds Oxide" }]));
    expect(await within(list).findByText("Uses Windows 11")).toBeInTheDocument();
    expect(await screen.findByText("Memory updated (2 items)")).toBeInTheDocument();
    expect(within(list).getAllByRole("link", { name: "The chat it came from" })).toHaveLength(2);
    expect(within(list).getByText(/Memory is off/)).toBeInTheDocument();

    fireEvent.click(within(list).getByRole("button", { name: "Delete all" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete all memory?" });
    // It opens on Cancel, not on the ✕ (whose tooltip would pop up).
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus(),
    );
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete all" }));
    await waitFor(() => expect(fakeState().memory).toHaveLength(0));
  });

  it("stores the knowledge budget within its limits", async () => {
    renderApp("/settings/memory");
    const field = await screen.findByRole("spinbutton", { name: /Knowledge budget/ });
    fireEvent.change(field, { target: { value: "50" } });
    fireEvent.blur(field);
    await waitFor(() => expect(fakeState().settings.projects.knowledgeBudget).toBe(1000));
  });

  it("shows a Save button while the budget has an unsaved change", async () => {
    renderApp("/settings/memory");
    const field = await screen.findByRole("spinbutton", { name: /Knowledge budget/ });
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    fireEvent.change(field, { target: { value: "25000" } });
    expect(screen.getByText(/Unsaved change/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(fakeState().settings.projects.knowledgeBudget).toBe(25000));
    expect(screen.queryByText(/Unsaved change/)).not.toBeInTheDocument();
    // Esc undoes an edit.
    fireEvent.change(field, { target: { value: "9" } });
    fireEvent.keyDown(field, { key: "Escape" });
    expect(field).toHaveValue(25000);
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
  });
});

describe("groupMemory", () => {
  const item = (id: string, projectId: string | null): MemoryItem => ({
    id,
    projectId,
    text: id,
    conversationId: null,
    createdAt: 0,
    updatedAt: 0,
  });

  it("puts all-chat items first and names projects", () => {
    const names: Record<string, string> = { p1: "Zeta", p2: "Alpha" };
    const groups = groupMemory(
      [item("a", "p2"), item("b", null), item("c", "p1"), item("d", "gone")],
      (id) => names[id],
    );
    expect(groups.map((g) => [g.title, g.items.map((i) => i.id)])).toEqual([
      ["All chats", ["b"]],
      ["A deleted project", ["d"]],
      ["Alpha", ["a"]],
      ["Zeta", ["c"]],
    ]);
  });
});
