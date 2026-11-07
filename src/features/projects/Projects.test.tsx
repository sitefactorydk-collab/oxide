import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeState } from "@/test/fakeBackend";
import { renderApp } from "@/test/renderApp";
import { budgetPercent, formatTokens, knowledgeSummary } from "./knowledge";

const sidebar = () => screen.getByRole("complementary", { name: "Sidebar" });
const openMenu = (button: HTMLElement) => fireEvent.keyDown(button, { key: "Enter" });

async function knowledgeCard() {
  return screen.findByRole("region", { name: "Knowledge" });
}

describe("project page", () => {
  it("edits the project's instructions", async () => {
    renderApp("/project/p1");
    fireEvent.click(await screen.findByRole("button", { name: "Edit instructions" }));
    const box = await screen.findByRole("textbox", { name: "Project instructions" });
    expect(box).toHaveValue(fakeState().projects.find((p) => p.id === "p1")?.instructions);
    fireEvent.change(box, { target: { value: "  Answer in Danish.  " } });
    fireEvent.click(screen.getByRole("button", { name: "Save instructions" }));
    await waitFor(() =>
      expect(fakeState().projects.find((p) => p.id === "p1")?.instructions).toBe(
        "Answer in Danish.",
      ),
    );
    const card = screen.getByRole("region", { name: "Instructions" });
    expect(await within(card).findByText("Answer in Danish.")).toBeInTheDocument();
  });

  it("adds text and files to the knowledge, shows and removes them", async () => {
    renderApp("/project/p1");
    let card = await knowledgeCard();
    expect(await within(card).findByText(/No files yet/)).toBeInTheDocument();

    openMenu(within(card).getByRole("button", { name: "Add knowledge" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Add text content" }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Title" }), {
      target: { value: "Meeting notes" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: "Content" }), {
      target: { value: "We ship on Friday." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add content" }));
    card = await knowledgeCard();
    expect(await within(card).findByRole("button", { name: /^Meeting notes/ })).toBeInTheDocument();
    expect(within(card).getByText(/of the knowledge budget/)).toBeInTheDocument();
    expect(within(card).getByRole("meter", { name: "Knowledge budget used" })).toBeInTheDocument();

    // Upload (browser file input) a text file and an image: the image is refused.
    const input = screen.getByLabelText("Upload knowledge files");
    fireEvent.change(input, {
      target: {
        files: [
          new File(["hello world"], "notes.md", { type: "text/markdown" }),
          new File(["x"], "photo.png", { type: "image/png" }),
        ],
      },
    });
    expect(await within(card).findByRole("button", { name: /^notes\.md/ })).toBeInTheDocument();
    expect(await screen.findByText(/photo\.png is an image/)).toBeInTheDocument();
    expect(fakeState().knowledge.map((k) => k.file.name)).toEqual(["Meeting notes", "notes.md"]);

    // View the stored text.
    fireEvent.click(within(card).getByRole("button", { name: /^notes\.md/ }));
    const viewer = await screen.findByRole("dialog", { name: "notes.md" });
    expect(within(viewer).getByText("hello world")).toBeInTheDocument();
    fireEvent.click(within(viewer).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    fireEvent.click(within(card).getByRole("button", { name: "Remove notes.md" }));
    await waitFor(() =>
      expect(within(card).queryByRole("button", { name: /^notes\.md/ })).not.toBeInTheDocument(),
    );
    expect(fakeState().knowledge).toHaveLength(1);
  });

  it("says when the knowledge is too large and will be searched", async () => {
    renderApp("/project/p2");
    fakeState().settings.projects.knowledgeBudget = 5;
    const card = await knowledgeCard();
    openMenu(within(card).getByRole("button", { name: "Add knowledge" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Add text content" }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Content" }), {
      target: { value: "Broad beans go in early. ".repeat(4) },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add content" }));
    expect(await within(card).findByText(/Chats search the files/)).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: /^Untitled/ })).toBeInTheDocument();
  });

  it("archives a project, hiding it from the list until shown", async () => {
    renderApp("/project/p1");
    openMenu(await screen.findByRole("button", { name: "Project options" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Archive" }));
    expect(await screen.findByText(/This project is archived/)).toBeInTheDocument();
    expect(fakeState().projects.find((p) => p.id === "p1")?.archived).toBe(true);

    fireEvent.click(screen.getByRole("link", { name: "← All projects" }));
    expect(await screen.findByRole("link", { name: /Garden planner/ })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /^Oxide/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Archived (1)" }));
    const archived = await screen.findByRole("link", { name: /^Oxide/ });
    expect(archived).toHaveTextContent("Archived");
    fireEvent.click(archived);
    fireEvent.click(await screen.findByRole("button", { name: "Unarchive" }));
    await waitFor(() =>
      expect(fakeState().projects.find((p) => p.id === "p1")?.archived).toBe(false),
    );
  });

  it("deletes a project after confirming; its chats stay", async () => {
    const { location } = renderApp("/project/p2");
    openMenu(await screen.findByRole("button", { name: "Project options" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete project" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete “Garden planner”?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete project" }));
    await waitFor(() => expect(location.pathname).toBe("/projects"));
    expect(fakeState().projects.some((p) => p.id === "p2")).toBe(false);
    expect(fakeState().conversations.find((c) => c.id === "c3")?.projectId).toBeNull();
  });
});

describe("moving chats", () => {
  it("removes a chat from the project page", async () => {
    renderApp("/project/p1");
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Remove “Tauri custom title bar on Windows” from the project",
      }),
    );
    await waitFor(() =>
      expect(fakeState().conversations.find((c) => c.id === "c1")?.projectId).toBeNull(),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("button", {
          name: "Remove “Tauri custom title bar on Windows” from the project",
        }),
      ).not.toBeInTheDocument(),
    );
  });

  it("moves a chat into a project from the sidebar menu", async () => {
    renderApp("/new");
    await within(sidebar()).findByRole("link", { name: "SQLite FTS5 vs tantivy" });
    openMenu(within(sidebar()).getByRole("button", { name: /More options for SQLite/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Move to project" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Garden planner" }));
    await waitFor(() =>
      expect(fakeState().conversations.find((c) => c.id === "c5")?.projectId).toBe("p2"),
    );
    expect(await screen.findByText("Moved to Garden planner")).toBeInTheDocument();
  });
});

describe("knowledge helpers", () => {
  it("formats sizes and the budget line", () => {
    expect(formatTokens(950)).toBe("950");
    expect(formatTokens(1200)).toBe("1.2k");
    expect(formatTokens(2000)).toBe("2k");
    expect(formatTokens(45678)).toBe("46k");
    expect(budgetPercent({ totalTokens: 50, budgetTokens: 200 })).toBe(25);
    expect(budgetPercent({ totalTokens: 500, budgetTokens: 200 })).toBe(100);
    const base = { files: [], totalTokens: 0, budgetTokens: 1000 };
    expect(knowledgeSummary({ ...base, mode: "empty" })).toMatch(/No files yet/);
    expect(knowledgeSummary({ ...base, totalTokens: 100, mode: "full" })).toMatch(/^10%/);
    expect(knowledgeSummary({ ...base, totalTokens: 1500, mode: "search" })).toMatch(
      /1\.5k of 1k tokens/,
    );
  });
});
