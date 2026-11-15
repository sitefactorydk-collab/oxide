import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeState } from "@/test/fakeBackend";
import { renderApp } from "@/test/renderApp";

const composer = () => screen.getByRole("textbox", { name: "Message" });

function pasteFile(name: string, type: string, content = "x") {
  const file = new File([content], name, { type });
  fireEvent.paste(composer(), { clipboardData: { files: [file], types: ["Files"] } });
}

async function send(text: string) {
  fireEvent.change(composer(), { target: { value: text } });
  fireEvent.keyDown(composer(), { key: "Enter" });
  expect(await screen.findByText(text)).toBeInTheDocument();
}

describe("branch navigator", () => {
  it("switches between edited versions of a message", async () => {
    renderApp("/chat/c2");
    const original = await screen.findByText(/warm neutral palette/);
    const bubble = original.closest(".group") as HTMLElement;
    fireEvent.click(within(bubble).getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit message" }), {
      target: { value: "Suggest a cool palette." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText("Reply to: Suggest a cool palette.");

    const nav = screen.getAllByRole("group", { name: "Versions" })[0]!;
    expect(within(nav).getByLabelText("Version 2 of 2")).toBeInTheDocument();
    fireEvent.click(within(nav).getByRole("button", { name: "Previous version" }));
    expect(await screen.findByText(/warm neutral palette/)).toBeInTheDocument();
    expect(screen.getByText(/iron-oxide accent/)).toBeInTheDocument();
    expect(screen.queryByText("Suggest a cool palette.")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Version 1 of 2")).toBeInTheDocument();

    // A message sent now continues the branch on screen.
    await send("Darker please");
    const added = fakeState().messages.find(
      (m) =>
        m.role === "user" && m.content.some((b) => b.type === "text" && b.text === "Darker please"),
    );
    expect(added?.parentId).toBe("m6");
  });

  it("switches between regenerated replies", async () => {
    renderApp("/chat/c2");
    const old = await screen.findByText(/iron-oxide accent/);
    fireEvent.click(
      within(old.closest(".group") as HTMLElement).getByRole("button", { name: "Retry" }),
    );
    await screen.findByText("Reply to: Suggest a warm neutral palette with a rust accent.");
    fireEvent.click(screen.getByRole("button", { name: "Previous version" }));
    expect(await screen.findByText(/iron-oxide accent/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Next version" }));
    expect(
      await screen.findByText("Reply to: Suggest a warm neutral palette with a rust accent."),
    ).toBeInTheDocument();
  });
});

describe("message actions", () => {
  it("rates a reply, asks what went wrong for thumbs down, and stores it locally", async () => {
    renderApp("/chat/c2");
    const reply = (await screen.findByText(/iron-oxide accent/)).closest(".group") as HTMLElement;
    fireEvent.click(within(reply).getByRole("button", { name: "Good response" }));
    await waitFor(() =>
      expect(fakeState().messages.find((m) => m.id === "m6")?.feedback?.rating).toBe("good"),
    );
    expect(within(reply).getByRole("button", { name: "Remove rating" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    fireEvent.click(within(reply).getByRole("button", { name: "Bad response" }));
    const form = await within(reply).findByRole("form", { name: "What went wrong" });
    fireEvent.change(within(form).getByRole("textbox"), { target: { value: "Too pale" } });
    fireEvent.click(within(form).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(fakeState().messages.find((m) => m.id === "m6")?.feedback).toMatchObject({
        rating: "bad",
        note: "Too pale",
      }),
    );
  });

  it("shows token usage for replies that report it", async () => {
    renderApp("/chat/c2", { backend: { replies: "auto" } });
    await screen.findByText(/warm neutral palette/);
    await send("Short answer");
    await screen.findByText("Reply to: Short answer");
    expect(
      screen.getByRole("button", { name: /Token usage: .*10 input · 5 output tokens/ }),
    ).toBeInTheDocument();
  });

  it("exports a chat as Markdown from the title bar menu", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    const crumbs = screen.getByRole("navigation", { name: "Breadcrumb" });
    const trigger = within(crumbs).getByRole("button", {
      name: /Design tokens for light and dark/,
    });
    fireEvent.keyDown(trigger, { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Export as Markdown" }));
    await waitFor(() =>
      expect(fakeState().exports).toEqual([{ id: "c2", leafId: null, format: "markdown" }]),
    );
    expect(await screen.findByText(/Exported to/)).toBeInTheDocument();
  });

  it("exports a chat as a web page", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    const crumbs = screen.getByRole("navigation", { name: "Breadcrumb" });
    const trigger = within(crumbs).getByRole("button", {
      name: /Design tokens for light and dark/,
    });
    fireEvent.keyDown(trigger, { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Export as web page" }));
    await waitFor(() =>
      expect(fakeState().exports).toEqual([{ id: "c2", leafId: null, format: "html" }]),
    );
    expect(await screen.findByText(/export\.html/)).toBeInTheDocument();
  });
});

describe("attachments", () => {
  it("pastes a screenshot, shows its chip and sends it with the message", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    pasteFile("image.png", "image/png");
    const chips = await screen.findByRole("list", { name: "Attachments" });
    await waitFor(() => expect(within(chips).getByText("640×400")).toBeInTheDocument());
    expect(within(chips).getByText(/^Pasted image /)).toBeInTheDocument();

    await send("What is this?");
    await screen.findByText("Reply to: What is this?");
    const sent = fakeState().messages.find(
      (m) =>
        m.role === "user" && m.content.some((b) => b.type === "text" && b.text === "What is this?"),
    );
    expect(sent?.content[0]).toMatchObject({ type: "image", mediaType: "image/png" });
    expect(screen.queryByRole("list", { name: "Attachments" })).not.toBeInTheDocument();
    expect(
      await screen.findByRole("button", { name: "Open attached image 1" }),
    ).toBeInTheDocument();
  });

  it("can send files without text", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    pasteFile("notes.md", "text/markdown", "# hi");
    await screen.findByText("notes.md");
    const sendButton = screen.getByRole("button", { name: "Send" });
    await waitFor(() => expect(sendButton).toBeEnabled());
    fireEvent.click(sendButton);
    await waitFor(() =>
      expect(fakeState().messages.at(-2)?.content).toEqual([
        expect.objectContaining({ type: "document", name: "notes.md" }),
      ]),
    );
  });

  it("refuses unsupported files on the chip and doesn't send them", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    pasteFile("setup.exe", "application/octet-stream");
    const chips = await screen.findByRole("list", { name: "Attachments" });
    expect(await within(chips).findByText("Can’t attach")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    fireEvent.click(within(chips).getByRole("button", { name: "Remove setup.exe" }));
    expect(screen.queryByRole("list", { name: "Attachments" })).not.toBeInTheDocument();
  });

  it("starts a new chat with a file", async () => {
    const { location } = renderApp("/new");
    await screen.findByRole("button", { name: /Model:/ });
    pasteFile("report.pdf", "application/pdf");
    await screen.findByText("PDF · 3 pages");
    await act(async () => {
      fireEvent.change(composer(), { target: { value: "Summarize" } });
      fireEvent.keyDown(composer(), { key: "Enter" });
    });
    await waitFor(() => expect(location.pathname).toMatch(/^\/chat\//));
    const id = location.pathname.split("/")[2];
    const first = fakeState().messages.find((m) => m.conversationId === id && m.role === "user");
    expect(first?.content).toEqual([
      expect.objectContaining({ type: "document", name: "report.pdf" }),
      { type: "text", text: "Summarize" },
    ]);
  });
});

describe("effort", () => {
  it("offers the model's levels and sends the chosen one", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    const trigger = await screen.findByRole("button", { name: /Effort: model default \(Medium\)/ });
    fireEvent.keyDown(trigger, { key: "Enter" });
    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitemradio")
        .map((i) => i.textContent),
    ).toEqual([
      expect.stringContaining("Default (Medium)"),
      expect.stringContaining("Low"),
      expect.stringContaining("Medium"),
      expect.stringContaining("High"),
      expect.stringContaining("Extra"),
      expect.stringContaining("Max"),
    ]);
    fireEvent.click(within(menu).getByRole("menuitemradio", { name: /^Low/ }));
    await screen.findByRole("button", { name: "Effort: Low" });
    await send("Quick one");
    await screen.findByText("Reply to: Quick one");
    expect(fakeState().replyOptions.at(-1)).toEqual({ thinking: false, effort: "low" });
  });

  it("locks thinking on for models that always think", async () => {
    renderApp("/chat/c2");
    const toggle = await screen.findByRole("button", { name: "Extended thinking" });
    await waitFor(() => expect(toggle).toBeDisabled());
    expect(toggle).toHaveAttribute("aria-pressed", "true");
  });

  it("hides the effort control for models without one", async () => {
    renderApp("/chat/c5");
    await screen.findByRole("button", { name: "Model: qwen/qwen3-14b" });
    expect(screen.queryByRole("button", { name: /^Effort/ })).not.toBeInTheDocument();
  });
});

describe("scrolling", () => {
  it("offers to jump to the latest message after scrolling up", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    const log = screen.getByRole("log", { name: "Messages" });
    const scroller = log.parentElement as HTMLElement;
    Object.defineProperty(scroller, "scrollHeight", { configurable: true, value: 3000 });
    Object.defineProperty(scroller, "clientHeight", { configurable: true, value: 600 });
    scroller.scrollTop = 100;
    fireEvent.scroll(scroller);
    const jump = await screen.findByRole("button", { name: "Jump to latest" });
    fireEvent.click(jump);
    expect(screen.queryByRole("button", { name: "Jump to latest" })).not.toBeInTheDocument();
  });
});
