import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ToolCallView } from "@/features/chat/streams";
import { callHeadline, toolKind, usageRows } from "@/features/tools/toolView";
import { grantLabel } from "@/features/workspace/grants";
import { askApproval, emitStream, fakeState, heldReplyId } from "@/test/fakeBackend";
import { fakeChromeState, pairFakeChrome, setFakeChromeConnected } from "@/test/fakeChrome";
import { fakeMcpState } from "@/test/fakeMcp";
import { renderApp } from "@/test/renderApp";

const ON = { settings: { chrome: { enabled: true } } };

describe("Settings → Chrome", () => {
  it("turns Chrome on, pairs with a six-word code, and forgets a browser", async () => {
    renderApp("/settings/chrome");
    const card = await screen.findByRole("region", { name: "Chrome connection" });
    expect(within(card).getByRole("status")).toHaveTextContent("Off");
    // How to install: the folder to load unpacked.
    const install = screen.getByRole("region", { name: "Install the extension" });
    expect(install).toHaveTextContent("chrome://extensions");
    expect(install).toHaveTextContent("C:/AI Apps/Oxide/extension/dist");

    fireEvent.click(screen.getByRole("switch", { name: "Let models use Chrome" }));
    await waitFor(() => expect(fakeState().settings.chrome.enabled).toBe(true));
    expect(await within(card).findByText("Not paired yet")).toBeInTheDocument();
    expect(card).toHaveTextContent("127.0.0.1:47820");

    fireEvent.click(within(card).getByRole("button", { name: "Pair a browser" }));
    const code = await within(card).findByRole("group", { name: "Pairing code" });
    expect(within(code).getByLabelText("Code")).toHaveTextContent(
      "amber river toast mint lake ocean",
    );
    expect(code).toHaveTextContent(/Valid for 5 more minutes/);
    expect(fakeChromeState().pairings).toBe(1);

    // The extension pairs: the code goes, the browser is connected and listed.
    act(() => void pairFakeChrome());
    expect(await within(card).findByText("Connected: Chrome 142 on Windows")).toBeInTheDocument();
    expect(within(card).queryByRole("group", { name: "Pairing code" })).not.toBeInTheDocument();
    const paired = screen.getByRole("region", { name: "Paired browsers" });
    expect(paired).toHaveTextContent("Chrome 142 on Windows");
    expect(paired).toHaveTextContent("Connected");

    act(() => setFakeChromeConnected(null));
    expect(await within(card).findByText("Waiting for Chrome")).toBeInTheDocument();

    fireEvent.click(within(paired).getByRole("button", { name: "Forget" }));
    await waitFor(() => expect(fakeChromeState().forgotten).toEqual(["b1"]));
    expect(await within(paired).findByText("None yet.")).toBeInTheDocument();

    const port = screen.getByRole("spinbutton", { name: "Port" });
    fireEvent.change(port, { target: { value: "47900" } });
    fireEvent.blur(port);
    await waitFor(() => expect(fakeState().settings.chrome.port).toBe(47900));
    fireEvent.click(
      screen.getByRole("switch", {
        name: "Ask before opening pages, clicking and typing in Chrome",
      }),
    );
    await waitFor(() => expect(fakeState().settings.chrome.ask).toBe(false));
  });

  it("is a built-in connector with its own switch", async () => {
    renderApp("/settings/connectors");
    const card = await screen.findByRole("region", { name: "chrome (built in)" });
    expect(card).toHaveTextContent("Off");
    fireEvent.click(within(card).getByRole("switch", { name: "Use Chrome" }));
    await waitFor(() => expect(fakeState().settings.chrome.enabled).toBe(true));
    expect(await within(card).findByText(/Not paired yet/)).toBeInTheDocument();
  });
});

describe("Chrome calls in a reply", () => {
  it("asks before opening a page in the user's Chrome", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold", ...ON } });
    await screen.findByText(/warm neutral palette/);
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Check my order" } });
    fireEvent.keyDown(box, { key: "Enter" });
    const live = await screen.findByLabelText("Reply in progress");
    await within(live).findByText("Partial");
    const rid = heldReplyId("c2");
    const input = JSON.stringify({ url: "https://shop.example/orders" });
    act(() => {
      emitStream("c2", { type: "toolUse", id: "c1", name: "chrome__navigate" });
      emitStream("c2", {
        type: "toolCall",
        id: "c1",
        name: "chrome__navigate",
        server: "chrome",
        tool: "navigate",
        title: "Open a page in Chrome",
        inputJson: input,
      });
      askApproval({
        conversationId: "c2",
        messageId: rid,
        toolUseId: "c1",
        server: "chrome",
        tool: "navigate",
        title: "Open a page in Chrome",
        description: "Open an address in this chat's tab in the user's Chrome.",
        inputJson: input,
        readOnly: false,
        destructive: false,
        requestedAt: Date.now(),
      });
    });
    const card = await screen.findByRole("group", { name: "Approve Open a page in Chrome" });
    expect(card).toHaveTextContent("your own Chrome, with your sign-ins");
    expect(card).toHaveTextContent("shows Stop in the tab");
    fireEvent.click(within(card).getByRole("button", { name: "Allow once" }));
    act(() => {
      emitStream("c2", { type: "toolRunning", id: "c1" });
    });
    expect(
      await within(live).findByRole("button", { name: /Opening in Chrome/ }),
    ).toHaveTextContent("https://shop.example/orders");
  });

  it("names Chrome's tools, rows and grants", () => {
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
    const nav = view("navigate", "chrome", JSON.stringify({ url: "https://shop.example" }));
    expect(toolKind(nav)).toBe("chrome");
    expect(callHeadline(nav)).toEqual({
      action: "Opened in Chrome",
      detail: "https://shop.example",
    });
    expect(callHeadline(view("click", "chrome", JSON.stringify({ ref: 3 })))).toEqual({
      action: "Clicked in Chrome",
      detail: "[3]",
    });
    expect(toolKind(view("bookmark", "chrome", "{}"))).toBe("connector");
    expect(grantLabel("builtin/chrome__form_input")).toBe("Chrome: form input");
    const rows = usageRows([
      {
        id: "1",
        conversationId: "c",
        messageId: "m",
        toolUseId: "t1",
        kind: "chrome",
        server: "chrome",
        tool: "navigate",
        label: "https://shop.example",
        isError: false,
        createdAt: 1,
      },
    ]);
    expect(rows[0]).toMatchObject({ title: "Chrome", count: 1 });
  });
});

describe("the + menu", () => {
  it("has a Chrome switch when Chrome is on in Settings", async () => {
    renderApp("/chat/c2", { backend: ON });
    await screen.findByText(/warm neutral palette/);
    fireEvent.keyDown(screen.getByRole("button", { name: "Add files, context and tools" }), {
      key: "Enter",
    });
    const item = await screen.findByRole("menuitemcheckbox", { name: "Chrome" });
    expect(item).toHaveAttribute("aria-checked", "true");
    fireEvent.click(item);
    await waitFor(() =>
      expect(fakeMcpState().chatTools.get("c2")?.disabledBuiltins).toContain("chrome"),
    );
  });
});
