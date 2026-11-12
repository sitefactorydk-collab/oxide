import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ContentBlock } from "@/bindings";
import type { ToolCallView } from "@/features/chat/streams";
import { callHeadline, toolKind, usageRows } from "@/features/tools/toolView";
import { grantLabel } from "@/features/workspace/grants";
import { askApproval, emitStream, fakeState, heldReplyId } from "@/test/fakeBackend";
import {
  emitBrowserFrame,
  failFakeBrowserSetup,
  fakeBrowserReady,
  fakeBrowserState,
  finishFakeBrowserSetup,
  setFakeBrowserOpen,
} from "@/test/fakeBrowser";
import { fakeMcpState } from "@/test/fakeMcp";
import { renderApp } from "@/test/renderApp";

const ON = { settings: { browser: { enabled: true } } };

describe("Settings → Browser", () => {
  it("shows what is installed, sets up with progress, and saves the settings", async () => {
    renderApp("/settings/browser");
    const card = await screen.findByRole("region", { name: "Browser setup" });
    expect(await within(card).findByText("Not set up yet")).toBeInTheDocument();
    expect(card).toHaveTextContent("Microsoft Edge");
    expect(card).toHaveTextContent("Not downloaded");

    fireEvent.click(screen.getByRole("switch", { name: "Let models use a browser" }));
    await waitFor(() => expect(fakeState().settings.browser.enabled).toBe(true));

    fireEvent.click(within(card).getByRole("button", { name: "Set up now" }));
    expect(await within(card).findByText("Installing the browser packages")).toBeInTheDocument();
    expect(fakeBrowserState().setups).toEqual([{ chromium: false }]);
    act(() => finishFakeBrowserSetup());
    expect(await within(card).findByText("The browser is ready")).toBeInTheDocument();
    expect(card).toHaveTextContent("browser-use 1.0.2");

    // Chromium on request.
    fireEvent.click(within(card).getByRole("button", { name: "Download Chromium" }));
    await waitFor(() => expect(fakeBrowserState().setups).toContainEqual({ chromium: true }));
    act(() => failFakeBrowserSetup("the Chromium download ended with exit code 1."));
    expect(await within(card).findByText("Setting up the browser failed")).toBeInTheDocument();
    expect(card).toHaveTextContent("exit code 1");
    expect(within(card).getByRole("button", { name: "Try again" })).toBeInTheDocument();

    // Connecting to the user's own Chrome shows where and how.
    fireEvent.click(
      screen.getByRole("radio", { name: "My own Chrome (connect over remote debugging)" }),
    );
    await waitFor(() => expect(fakeState().settings.browser.source).toBe("cdp"));
    const addr = screen.getByRole("textbox", { name: "Chrome's debugging address" });
    expect(addr).toHaveValue("http://127.0.0.1:9222");
    fireEvent.change(addr, { target: { value: "http://127.0.0.1:9333" } });
    fireEvent.blur(addr);
    await waitFor(() => expect(fakeState().settings.browser.cdpUrl).toBe("http://127.0.0.1:9333"));
    expect(screen.getByText(/--remote-debugging-port=9222/)).toBeInTheDocument();

    const steps = screen.getByRole("spinbutton", { name: "Steps per browsing task" });
    fireEvent.change(steps, { target: { value: "40" } });
    fireEvent.blur(steps);
    await waitFor(() => expect(fakeState().settings.browser.maxSteps).toBe(40));

    fireEvent.click(screen.getByRole("switch", { name: "Show the browser window" }));
    await waitFor(() => expect(fakeState().settings.browser.showWindow).toBe(true));
    fireEvent.click(
      screen.getByRole("switch", {
        name: "Ask before opening pages, clicking, typing and browsing tasks",
      }),
    );
    await waitFor(() => expect(fakeState().settings.browser.ask).toBe(false));
  });

  it("is a built-in connector with its own switch", async () => {
    renderApp("/settings/connectors");
    const card = await screen.findByRole("region", { name: "browser (built in)" });
    expect(card).toHaveTextContent("Off");
    fireEvent.click(within(card).getByRole("switch", { name: "Use browser" }));
    await waitFor(() => expect(fakeState().settings.browser.enabled).toBe(true));
    expect(await within(card).findByText(/Sets up the first time/)).toBeInTheDocument();
  });
});

describe("browser calls in a reply", () => {
  it("asks with the goal, shows a live view, then the screenshots", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold", ...ON } });
    await screen.findByText(/warm neutral palette/);
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Find the bakery's opening hours" } });
    fireEvent.keyDown(box, { key: "Enter" });
    const live = await screen.findByLabelText("Reply in progress");
    await within(live).findByText("Partial");
    const rid = heldReplyId("c2");
    const input = JSON.stringify({ goal: "Find the opening hours of The Little Bakery" });
    act(() => {
      emitStream("c2", { type: "toolUse", id: "b1", name: "browser__browser_task" });
      emitStream("c2", {
        type: "toolCall",
        id: "b1",
        name: "browser__browser_task",
        server: "browser",
        tool: "browser_task",
        title: "Browse autonomously",
        inputJson: input,
      });
      askApproval({
        conversationId: "c2",
        messageId: rid,
        toolUseId: "b1",
        server: "browser",
        tool: "browser_task",
        title: "Browse autonomously",
        description: "Hand a browsing goal to an autonomous agent.",
        inputJson: input,
        readOnly: false,
        destructive: false,
        requestedAt: Date.now(),
      });
    });
    const card = await screen.findByRole("group", { name: "Approve Browse autonomously" });
    expect(within(card).getByLabelText("Goal")).toHaveTextContent(
      "Find the opening hours of The Little Bakery",
    );
    expect(card).toHaveTextContent("browses on its own");
    fireEvent.click(within(card).getByRole("button", { name: "Allow once" }));

    act(() => {
      emitStream("c2", { type: "toolRunning", id: "b1" });
      emitBrowserFrame({ conversationId: "c2", toolUseId: "b1", text: "Browsing: Bakery" });
    });
    const line = await within(live).findByRole("button", { name: /Browsing/ });
    expect(line).toHaveTextContent("Find the opening hours");
    expect(await within(live).findByAltText("Live view of the browser")).toHaveAttribute(
      "src",
      "data:image/jpeg;base64,/9j/4AAQ",
    );
    expect(within(live).getByText("https://bakery.example/")).toBeInTheDocument();
    expect(line).toHaveTextContent("Browsing: Bakery");

    const content: ContentBlock[] = [
      { type: "text", text: "Browser-Use finished the task.\n\nResult:\nSat: 8–14" },
      { type: "image", mediaType: "image/jpeg", source: { kind: "base64", data: "/9j/end" } },
    ];
    act(() =>
      emitStream("c2", { type: "toolResult", id: "b1", content, isError: false, durationMs: 9000 }),
    );
    expect(await within(live).findByAltText("Tool image 1")).toBeInTheDocument();
    expect(within(live).queryByAltText("Live view of the browser")).not.toBeInTheDocument();
    expect(within(live).getByRole("button", { name: /Browsed/ })).toBeInTheDocument();
  });

  it("names the browser's tools, rows and grants", () => {
    const view = (tool: string, server: string | null, inputJson: string): ToolCallView =>
      ({
        id: "x",
        name: server ? `${server}__${tool}` : tool,
        tool,
        server,
        title: null,
        inputJson,
        status: "done",
        approval: null,
        result: null,
        isError: false,
        durationMs: null,
        provider: false,
      }) as ToolCallView;
    const nav = view("navigate", "browser", JSON.stringify({ url: "https://bakery.example" }));
    expect(toolKind(nav)).toBe("browser");
    expect(callHeadline(nav)).toEqual({ action: "Opened", detail: "https://bakery.example" });
    expect(callHeadline(view("click", "browser", JSON.stringify({ ref: 3 })))).toEqual({
      action: "Clicked",
      detail: "[3]",
    });
    expect(callHeadline(view("type", "browser", JSON.stringify({ text: "rye" })))).toEqual({
      action: "Typed",
      detail: "“rye”",
    });
    // A connector of the user's that happens to be called "browser" with other tools stays one.
    expect(toolKind(view("bookmark", "browser", "{}"))).toBe("connector");
    expect(grantLabel("builtin/browser__navigate")).toBe("Browser: navigate");
    const rows = usageRows([
      {
        id: "1",
        conversationId: "c",
        messageId: "m",
        toolUseId: "t1",
        kind: "browser",
        server: "browser",
        tool: "navigate",
        label: "https://bakery.example",
        isError: false,
        createdAt: 1,
      },
      {
        id: "2",
        conversationId: "c",
        messageId: "m",
        toolUseId: "t2",
        kind: "browser",
        server: "browser",
        tool: "browser_task",
        label: "Find the hours",
        isError: false,
        createdAt: 2,
      },
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ title: "Browser", count: 2 });
  });
});

describe("the chat's browser", () => {
  it("shows in the session panel with Close, and has a + menu switch", async () => {
    renderApp("/chat/c2", { ui: { rightPanelOpen: true }, backend: ON });
    const panel = await screen.findByRole("complementary", { name: "Chat details" });
    const section = await within(panel).findByRole("region", { name: "Browser" });
    expect(section).toHaveTextContent("Closed");
    act(() => {
      fakeBrowserReady();
      setFakeBrowserOpen("c2", true);
      emitBrowserFrame({ conversationId: "c2", toolUseId: "n1", url: "https://bakery.example/" });
    });
    expect(await within(section).findByText("Open")).toBeInTheDocument();
    expect(within(section).getByAltText("The chat's browser")).toBeInTheDocument();
    fireEvent.click(within(section).getByRole("button", { name: "Close the browser" }));
    await waitFor(() => expect(fakeBrowserState().closed).toEqual(["c2"]));
    expect(await within(section).findByText("Closed")).toBeInTheDocument();

    const plus = screen.getByRole("button", { name: "Add files, context and tools" });
    fireEvent.keyDown(plus, { key: "Enter" });
    const item = await screen.findByRole("menuitemcheckbox", { name: "Browser" });
    expect(item).toHaveAttribute("aria-checked", "true");
    fireEvent.click(item);
    await waitFor(() =>
      expect(fakeMcpState().chatTools.get("c2")?.disabledBuiltins).toContain("browser"),
    );
  });
});
