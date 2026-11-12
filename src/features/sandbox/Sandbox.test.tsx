import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ToolCallView } from "@/features/chat/streams";
import { callHeadline, toolKind, usageRows } from "@/features/tools/toolView";
import { grantLabel } from "@/features/workspace/grants";
import { askApproval, emitStream, fakeState, heldReplyId } from "@/test/fakeBackend";
import { fakeMcpState } from "@/test/fakeMcp";
import {
  fakeDockerMissing,
  fakeSandboxState,
  sandboxFile,
  setFakeDocker,
  setFakePick,
  setFakeSandbox,
} from "@/test/fakeSandbox";
import { renderApp } from "@/test/renderApp";
import { invalidateFor, sandboxKeys } from "./queries";

const ON = { settings: { sandbox: { enabled: true } } };

describe("Settings → Sandbox", () => {
  it("shows Docker's status and saves the settings", async () => {
    renderApp("/settings/sandbox");
    const docker = await screen.findByRole("region", { name: "Docker" });
    expect(await within(docker).findByText("Docker 28.1.1 is running")).toBeInTheDocument();
    expect(docker).toHaveTextContent("8 CPUs");

    fireEvent.click(screen.getByRole("switch", { name: "Let models use a sandbox" }));
    await waitFor(() => expect(fakeState().settings.sandbox.enabled).toBe(true));

    // The network is off by default; turning it on is a choice.
    const net = screen.getByRole("switch", { name: "Internet access" });
    expect(net).toHaveAttribute("aria-checked", "false");
    fireEvent.click(net);
    await waitFor(() => expect(fakeState().settings.sandbox.network).toBe(true));

    const mem = screen.getByRole("spinbutton", { name: "Memory (MB)" });
    fireEvent.change(mem, { target: { value: "4096" } });
    fireEvent.blur(mem);
    await waitFor(() => expect(fakeState().settings.sandbox.memoryMb).toBe(4096));

    const image = screen.getByRole("textbox", { name: "Image for commands" });
    fireEvent.change(image, { target: { value: "node:22-bookworm" } });
    fireEvent.blur(image);
    await waitFor(() => expect(fakeState().settings.sandbox.image).toBe("node:22-bookworm"));

    fireEvent.click(screen.getByRole("radio", { name: "In a folder on this computer" }));
    await waitFor(() => expect(fakeState().settings.sandbox.storage).toBe("folder"));
    expect(screen.getByText("Oxide's data folder (default)")).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("switch", { name: "Ask before commands and file changes in the sandbox" }),
    );
    await waitFor(() => expect(fakeState().settings.sandbox.ask).toBe(false));
  });

  it("says clearly when Docker is missing, with a link to get it", async () => {
    renderApp("/settings/sandbox");
    await screen.findByText("Docker 28.1.1 is running");
    act(() => fakeDockerMissing());
    const docker = screen.getByRole("region", { name: "Docker" });
    expect(await within(docker).findByText("Docker isn't installed")).toBeInTheDocument();
    fireEvent.click(within(docker).getByRole("button", { name: "Get Docker" }));
    await waitFor(() =>
      expect(fakeState().opened.at(-1)).toBe("https://docs.docker.com/get-started/get-docker/"),
    );
  });

  it("downloads missing images", async () => {
    renderApp("/settings/sandbox");
    await screen.findByText("Docker 28.1.1 is running");
    act(() =>
      setFakeDocker({
        images: [
          { name: "python:3.12-bookworm", role: "Commands", present: false, sizeBytes: null },
          { name: "mcp/filesystem", role: "Files", present: true, sizeBytes: 1000 },
        ],
      }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Download images" }));
    await waitFor(() => expect(fakeSandboxState().prepared).toBe(1));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Download images" })).not.toBeInTheDocument(),
    );
  });
});

describe("the session panel's Sandbox section", () => {
  it("isn't there while the sandbox is off", async () => {
    renderApp("/chat/c2", { ui: { rightPanelOpen: true } });
    const panel = await screen.findByRole("complementary", { name: "Chat details" });
    await within(panel).findByRole("region", { name: "Progress" });
    expect(within(panel).queryByRole("region", { name: "Sandbox" })).not.toBeInTheDocument();
  });

  it("shows files, saves one, copies one in, and resets after confirming", async () => {
    renderApp("/chat/c2", { ui: { rightPanelOpen: true }, backend: ON });
    const panel = await screen.findByRole("complementary", { name: "Chat details" });
    const section = await within(panel).findByRole("region", { name: "Sandbox" });
    expect(await within(section).findByText(/Nothing yet/)).toBeInTheDocument();

    act(() =>
      setFakeSandbox(
        "c2",
        {
          state: "running",
          image: "python:3.12-bookworm",
          workspace: "oxide-sandbox-c2",
          commands: [{ id: "1", command: "python3 -m unittest", startedAt: Date.now() }],
        },
        {
          "/workspace": [
            sandboxFile("/workspace/app", null, true),
            sandboxFile("/workspace/README.md", 120),
          ],
          "/workspace/app": [sandboxFile("/workspace/app/main.py", 42)],
        },
      ),
    );
    expect(await within(section).findByText("Running")).toBeInTheDocument();
    expect(within(section).getByRole("list", { name: "Running commands" })).toHaveTextContent(
      "python3 -m unittest",
    );
    expect(section).toHaveTextContent("no network");
    // Busy: no reset while a command runs.
    expect(within(section).getByRole("button", { name: "Reset" })).toBeDisabled();

    const files = await within(section).findByRole("list", { name: "Files in the sandbox" });
    expect(within(files).getByText("README.md")).toBeInTheDocument();
    fireEvent.click(within(files).getByRole("button", { name: "app" }));
    expect(await within(section).findByText("main.py")).toBeInTheDocument();

    setFakePick("C:\\Users\\me\\Out");
    fireEvent.click(within(section).getByRole("button", { name: "Save main.py to a folder" }));
    await waitFor(() =>
      expect(fakeSandboxState().saved).toEqual([
        { conversationId: "c2", path: "/workspace/app/main.py" },
      ]),
    );

    setFakePick("C:\\Users\\me\\data.csv");
    fireEvent.click(within(section).getByRole("button", { name: "Copy in a file" }));
    await waitFor(() => expect(fakeSandboxState().copiedIn).toHaveLength(1));
    expect(fakeSandboxState().copiedIn[0]).toMatchObject({
      dest: "/workspace/app/",
      folder: false,
    });
    expect(await within(section).findByText("data.csv")).toBeInTheDocument();

    act(() => setFakeSandbox("c2", { commands: [] }));
    await waitFor(() =>
      expect(within(section).getByRole("button", { name: "Reset" })).toBeEnabled(),
    );
    fireEvent.click(within(section).getByRole("button", { name: "Reset" }));
    const dialog = await screen.findByRole("dialog", { name: "Reset this chat's sandbox?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Reset" }));
    await waitFor(() => expect(fakeSandboxState().resets).toEqual(["c2"]));
    expect(await within(section).findByText(/Nothing yet/)).toBeInTheDocument();
  });

  it("starts a stopped sandbox to show its files", async () => {
    renderApp("/chat/c2", { ui: { rightPanelOpen: true }, backend: ON });
    const panel = await screen.findByRole("complementary", { name: "Chat details" });
    act(() =>
      setFakeSandbox(
        "c2",
        { state: "stopped", image: "python:3.12-bookworm" },
        {
          "/workspace": [sandboxFile("/workspace/kept.txt", 4)],
        },
      ),
    );
    const section = await within(panel).findByRole("region", { name: "Sandbox" });
    expect(await within(section).findByText(/Stopped while idle/)).toBeInTheDocument();
    fireEvent.click(within(section).getByRole("button", { name: "Start and show files" }));
    expect(await within(section).findByText("kept.txt")).toBeInTheDocument();
  });

  it("explains a missing Docker in the panel", async () => {
    renderApp("/chat/c2", { ui: { rightPanelOpen: true }, backend: ON });
    const panel = await screen.findByRole("complementary", { name: "Chat details" });
    act(() => fakeDockerMissing());
    act(() => setFakeSandbox("c2", {}));
    const section = await within(panel).findByRole("region", { name: "Sandbox" });
    expect(await within(section).findByRole("alert")).toHaveTextContent("Docker isn't installed");
  });
});

describe("sandbox calls in a reply", () => {
  it("asks with the command and labels the calls", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold", ...ON } });
    await screen.findByText(/warm neutral palette/);
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Build and test it" } });
    fireEvent.keyDown(box, { key: "Enter" });
    const live = await screen.findByLabelText("Reply in progress");
    await within(live).findByText("Partial");
    const rid = heldReplyId("c2");
    const input = JSON.stringify({ command: "python3 -m unittest -v", cwd: "app" });
    act(() => {
      emitStream("c2", { type: "toolUse", id: "s1", name: "sandbox_exec" });
      emitStream("c2", {
        type: "toolCall",
        id: "s1",
        name: "sandbox_exec",
        server: null,
        tool: "sandbox_exec",
        title: "Run in the sandbox",
        inputJson: input,
      });
      askApproval({
        conversationId: "c2",
        messageId: rid,
        toolUseId: "s1",
        server: null,
        tool: "sandbox_exec",
        title: "Run in the sandbox",
        description: "Run a shell command in this chat's sandbox.",
        inputJson: input,
        readOnly: false,
        destructive: false,
        requestedAt: Date.now(),
      });
    });
    const card = await screen.findByRole("group", { name: "Approve Run in the sandbox" });
    expect(within(card).getByLabelText("Command")).toHaveTextContent("python3 -m unittest -v");
    expect(within(live).getByRole("button", { name: /Running in the sandbox/ })).toHaveTextContent(
      "python3 -m unittest -v",
    );
  });

  it("names the sandbox's tools and file tools", () => {
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
    const exec = view("sandbox_exec", null, JSON.stringify({ command: "make test\nmore" }));
    expect(toolKind(exec)).toBe("sandbox");
    expect(callHeadline(exec)).toEqual({ action: "Ran in the sandbox", detail: "make test" });
    const out = view("sandbox_copy_out", null, JSON.stringify({ from: "dist", to: "Out" }));
    expect(callHeadline(out)).toEqual({
      action: "Copied out of the sandbox",
      detail: "dist → Out",
    });
    const write = view("write_file", "sandbox", JSON.stringify({ path: "/workspace/app/m.py" }));
    expect(toolKind(write)).toBe("sandbox");
    expect(callHeadline(write)).toEqual({ action: "Wrote in the sandbox", detail: "app/m.py" });
    // A connector of the user's that happens to be called "sandbox" with other tools stays one.
    expect(toolKind(view("deploy", "sandbox", "{}"))).toBe("connector");

    expect(grantLabel("builtin/sandbox__write_file")).toBe("Sandbox files: write file");
    expect(grantLabel("builtin/sandbox_exec")).toBe("sandbox exec");

    const rows = usageRows([
      {
        id: "1",
        conversationId: "c",
        messageId: "m",
        toolUseId: "t1",
        kind: "sandbox",
        server: null,
        tool: "sandbox_exec",
        label: "make test",
        isError: false,
        createdAt: 1,
      },
      {
        id: "2",
        conversationId: "c",
        messageId: "m",
        toolUseId: "t2",
        kind: "sandbox",
        server: "sandbox",
        tool: "write_file",
        label: "/workspace/a.py",
        isError: false,
        createdAt: 2,
      },
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ title: "Sandbox", count: 2 });
    expect(rows[0]?.details).toEqual(["make test", "/workspace/a.py"]);
  });

  it("reads Docker again on a general change and a chat's sandbox on its own", () => {
    expect(invalidateFor({ conversationId: null })).toEqual([sandboxKeys.docker]);
    expect(invalidateFor({ conversationId: "c2" })).toEqual([
      sandboxKeys.chat("c2"),
      sandboxKeys.files("c2"),
    ]);
  });
});

describe("the + menu", () => {
  it("has a Sandbox switch only when the sandbox is on", async () => {
    renderApp("/chat/c2", { backend: ON });
    await screen.findByText(/warm neutral palette/);
    const plus = screen.getByRole("button", { name: "Add files, context and tools" });
    fireEvent.keyDown(plus, { key: "Enter" });
    const item = await screen.findByRole("menuitemcheckbox", { name: "Sandbox" });
    expect(item).toHaveAttribute("aria-checked", "true");
    fireEvent.click(item);
    await waitFor(() =>
      expect(fakeMcpState().chatTools.get("c2")?.disabledBuiltins).toContain("sandbox"),
    );
  });
});
