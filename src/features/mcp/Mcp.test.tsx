import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeMcpAsk, fakeMcpNeedsTrust, fakeMcpState } from "@/test/fakeMcp";
import { renderApp } from "@/test/renderApp";

const card = async (name: string) => screen.findByRole("region", { name });

describe("Settings → Connectors", () => {
  it("lists servers with their status and what they offer", async () => {
    renderApp("/settings/connectors");
    const everything = await card("everything");
    expect(within(everything).getByTestId("mcp-status")).toHaveTextContent("Running");
    expect(within(everything).getByText(/2 tools · 1 resource · 2 prompts/)).toBeInTheDocument();
    const docs = await card("docs");
    expect(within(docs).getByTestId("mcp-status")).toHaveTextContent("Sign-in needed");
    expect(within(docs).getByText("Sign in to connect.")).toBeInTheDocument();
  });

  it("holds a command added outside Oxide until it is trusted", async () => {
    renderApp("/settings/connectors");
    const everything = await card("everything");
    act(() => fakeMcpNeedsTrust("everything", true));
    expect(within(everything).getByTestId("mcp-status")).toHaveTextContent("Waiting for your OK");
    expect(within(everything).getByRole("note")).toHaveTextContent(
      "command changed since you last allowed it",
    );
    expect(within(everything).queryByRole("button", { name: "Restart everything" })).toBeNull();
    fireEvent.click(within(everything).getByRole("button", { name: "Trust and start" }));
    await waitFor(() =>
      expect(within(everything).getByTestId("mcp-status")).toHaveTextContent("Running"),
    );
    expect(fakeMcpState().trusted).toEqual(["everything"]);
  });

  it("signs in to a remote server", async () => {
    renderApp("/settings/connectors");
    const docs = await card("docs");
    fireEvent.click(within(docs).getByRole("button", { name: "Sign in" }));
    await waitFor(() =>
      expect(within(docs).getByTestId("mcp-status")).toHaveTextContent("Running"),
    );
  });

  it("adds a local server from the form", async () => {
    renderApp("/settings/connectors");
    await card("everything");
    fireEvent.click(screen.getByRole("button", { name: "Add server" }));
    const form = screen.getByRole("form", { name: "New server" });
    fireEvent.change(within(form).getByLabelText("Name"), { target: { value: "time" } });
    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "uvx" } });
    fireEvent.change(within(form).getByLabelText("Arguments"), {
      target: { value: "mcp-server-time\n--local-timezone\nEurope/Copenhagen\n" },
    });
    fireEvent.change(within(form).getByLabelText("Environment variables"), {
      target: { value: "TZ=Europe/Copenhagen" },
    });
    fireEvent.click(within(form).getByRole("button", { name: "Add server" }));
    await card("time");
    const saved = fakeMcpState().configs.find((c) => c.name === "time");
    expect(saved).toMatchObject({
      transport: "stdio",
      command: "uvx",
      args: ["mcp-server-time", "--local-timezone", "Europe/Copenhagen"],
      env: { TZ: "Europe/Copenhagen" },
    });
  });

  it("asks for a URL for remote servers", async () => {
    renderApp("/settings/connectors");
    await card("everything");
    fireEvent.click(screen.getByRole("button", { name: "Add server" }));
    const form = screen.getByRole("form", { name: "New server" });
    fireEvent.change(within(form).getByLabelText("Name"), { target: { value: "wiki" } });
    fireEvent.click(within(form).getByLabelText("Remote URL"));
    fireEvent.click(within(form).getByRole("button", { name: "Add server" }));
    expect(await within(form).findByRole("alert")).toHaveTextContent("Enter the server's URL.");
  });

  it("runs a tool from the debug panel", async () => {
    renderApp("/settings/connectors");
    const everything = await card("everything");
    fireEvent.click(within(everything).getByRole("button", { name: "Show everything details" }));
    fireEvent.click(
      await within(everything).findByRole("button", { name: "Open the runner for echo" }),
    );
    const args = within(everything).getByLabelText("Arguments (JSON)");
    expect(args).toHaveValue('{\n  "message": ""\n}');
    fireEvent.change(args, { target: { value: '{"message": "hi"}' } });
    fireEvent.click(within(everything).getByRole("button", { name: "Run echo" }));
    expect(await within(everything).findByText("Echo: hi")).toBeInTheDocument();
    expect(fakeMcpState().calls[0]).toMatchObject({ server: "everything", tool: "echo" });
    expect(fakeMcpState().calls[0]?.token).toMatch(/^debug-/);
  });

  it("explains Windows paths when the arguments aren't valid JSON", async () => {
    renderApp("/settings/connectors");
    const everything = await card("everything");
    fireEvent.click(within(everything).getByRole("button", { name: "Show everything details" }));
    fireEvent.click(
      await within(everything).findByRole("button", { name: "Open the runner for echo" }),
    );
    fireEvent.change(within(everything).getByLabelText("Arguments (JSON)"), {
      target: { value: '{"message": "C:\\AI Apps\\Oxide"}' },
    });
    fireEvent.click(within(everything).getByRole("button", { name: "Run echo" }));
    expect(await within(everything).findByRole("alert")).toHaveTextContent(
      "a backslash must be doubled",
    );
    expect(fakeMcpState().calls).toEqual([]);
  });

  it("imports servers from Claude desktop", async () => {
    renderApp("/settings/connectors");
    await card("everything");
    fireEvent.click(screen.getByRole("button", { name: "Import from Claude desktop" }));
    const dialog = await screen.findByRole("dialog", { name: "Import from Claude desktop" });
    const fs = await within(dialog).findByRole("checkbox", { name: /filesystem/ });
    expect(fs).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: /everything/ })).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("button", { name: /Import/ }));
    await waitFor(() => expect(fakeMcpState().imported).toEqual(["filesystem"]));
  });
});

describe("Settings → Developer", () => {
  it("edits the config file and refuses invalid JSON", async () => {
    renderApp("/settings/developer");
    const editor = await screen.findByLabelText("Config file contents");
    await waitFor(() => expect(editor).toHaveValue('{\n  "mcpServers": {}\n}\n'));
    fireEvent.change(editor, { target: { value: "{ nope" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("isn't valid JSON");
    fireEvent.change(editor, { target: { value: '{"mcpServers": {"a": {"command": "x"}}}' } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(fakeMcpState().configText).toContain('"a"'));
  });

  it("shows a server's log", async () => {
    renderApp("/settings/developer");
    const log = await screen.findByRole("log", { name: "Server log" });
    expect(await within(log).findByText("Starting default (STDIO) server...")).toBeInTheDocument();
  });
});

describe("questions from servers", () => {
  it("renders an elicitation form and sends the answer", async () => {
    renderApp("/new");
    act(() =>
      fakeMcpAsk({
        id: "ask-1",
        server: "everything",
        createdAt: 1,
        ask: {
          kind: "elicitation",
          message: "Which colour?",
          url: null,
          schema: JSON.stringify({
            type: "object",
            properties: {
              color: { type: "string", title: "Colour", enum: ["red", "blue"] },
              count: { type: "integer", title: "How many" },
              sure: { type: "boolean", title: "Sure?" },
            },
            required: ["color"],
          }),
        },
      }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Which colour?")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Send" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Colour is required.");
    fireEvent.change(within(dialog).getByLabelText(/Colour/), { target: { value: "blue" } });
    fireEvent.change(within(dialog).getByLabelText("How many"), { target: { value: "3" } });
    fireEvent.click(within(dialog).getByLabelText("Sure?"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Send" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(fakeMcpState().answers).toEqual([
      {
        id: "ask-1",
        answer: {
          kind: "elicitation",
          action: "accept",
          content: JSON.stringify({ color: "blue", count: 3, sure: true }),
        },
      },
    ]);
  });

  it("asks before a server may use a model", async () => {
    renderApp("/new");
    act(() =>
      fakeMcpAsk({
        id: "ask-2",
        server: "everything",
        createdAt: 1,
        ask: {
          kind: "sampling",
          systemPrompt: null,
          messages: [{ role: "user", content: { type: "text", text: "Summarise this." } }],
          maxTokens: 100,
          modelHints: [],
        },
      }),
    );
    const dialog = await screen.findByRole("dialog", { name: /Let everything use a model/ });
    expect(within(dialog).getByText("Summarise this.")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Deny" }));
    await waitFor(() =>
      expect(fakeMcpState().answers[0]?.answer).toEqual({
        kind: "sampling",
        approved: false,
        model: null,
      }),
    );
  });
});

describe("composer + menu", () => {
  const openPlus = async () => {
    const trigger = await screen.findByRole("button", { name: "Add files, context and tools" });
    fireEvent.keyDown(trigger, { key: "Enter" });
    return screen.findByRole("menu");
  };

  it("switches a connector off for the current chat", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    const menu = await openPlus();
    const connectors = within(menu).getByRole("menuitem", { name: /Connectors/ });
    fireEvent.keyDown(connectors, { key: "ArrowRight" });
    const docs = await screen.findByRole("menuitemcheckbox", { name: "Use docs in this chat" });
    expect(docs).toHaveAttribute("aria-checked", "true");
    fireEvent.click(docs);
    await waitFor(() =>
      expect(fakeMcpState().chatTools.get("c2")).toEqual({
        disabledServers: ["docs"],
        disabledTools: [],
        disabledBuiltins: [],
      }),
    );
  });

  it("attaches an MCP prompt to the message", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    const menu = await openPlus();
    const prompts = await within(menu).findByRole("menuitem", { name: /Add a prompt/ });
    fireEvent.keyDown(prompts, { key: "ArrowRight" });
    fireEvent.click(await screen.findByRole("menuitem", { name: /Simple prompt/ }));
    expect(await screen.findByText("simple_prompt.md")).toBeInTheDocument();
  });
});
