import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  askApproval,
  emitStream,
  fakeState,
  finishReplyWith,
  heldReplyId,
  saveArtifact,
} from "@/test/fakeBackend";
import {
  failFakePython,
  fakePythonReady,
  fakePythonState,
  fakePythonStep,
  finishFakePython,
} from "@/test/fakePython";
import { renderApp } from "@/test/renderApp";
import {
  callHeadline,
  codeLabel,
  pythonCode,
  toolKind,
  usageRows,
} from "@/features/tools/toolView";

const CODE = "import pandas as pd\nprint(pd.__version__)";
const INPUT = JSON.stringify({ code: CODE });

describe("Settings → Advanced", () => {
  it("sets Python up, showing each step, then lists what is installed", async () => {
    renderApp("/settings/advanced");
    expect(await screen.findByText("Not set up yet")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Set up Python" }));
    await waitFor(() => expect(fakePythonState().setups).toBe(1));

    const steps = await screen.findByRole("list", { name: "Setup steps" });
    expect(within(steps).getByText("Find uv").closest("li")).toHaveAttribute(
      "aria-current",
      "step",
    );
    expect(screen.getByRole("button", { name: "Repair" })).toBeDisabled();

    act(() => fakePythonStep("packages", "Installing packages", "Installed 12 packages"));
    expect(await screen.findByText("Installing packages")).toBeInTheDocument();
    expect(within(steps).getByText("Install the packages").closest("li")).toHaveAttribute(
      "aria-current",
      "step",
    );
    expect(within(steps).getByText("Find uv").closest("li")).not.toHaveAttribute("aria-current");
    expect(screen.getByLabelText("uv output")).toHaveTextContent("Installed 12 packages");

    act(() => finishFakePython());
    expect(await screen.findByText("Ready")).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Setup steps" })).not.toBeInTheDocument();
    const packages = screen.getByRole("list", { name: "Installed packages" });
    expect(
      within(packages)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(["pandas 2.2.3", "numpy 2.1.3", "matplotlib 3.9.2", "openpyxl 3.1.5"]);
    expect(screen.getByText("3.12.8")).toBeInTheDocument();
    expect(screen.getByText("412 MB")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Set up Python" })).not.toBeInTheDocument();
  });

  it("shows why a setup failed and tries again", async () => {
    renderApp("/settings/advanced");
    fireEvent.click(await screen.findByRole("button", { name: "Set up Python" }));
    // Wait until setup has started, or its first status would come after the failure.
    await screen.findByRole("list", { name: "Setup steps" });
    act(() => failFakePython("No network: could not download Python."));
    expect(await screen.findByText("Setup failed")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("No network");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(fakePythonState().setups).toBe(2));
    expect(await screen.findByRole("list", { name: "Setup steps" })).toBeInTheDocument();
  });

  it("repairs, shows the folder, and resets only after confirming", async () => {
    renderApp("/settings/advanced");
    await screen.findByText("Not set up yet");
    act(() => fakePythonReady());
    expect(await screen.findByText("Ready")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Show folder" }));
    await waitFor(() => expect(fakePythonState().folderOpened).toBe(1));

    fireEvent.click(screen.getByRole("button", { name: "Repair" }));
    await waitFor(() => expect(fakePythonState().repairs).toBe(1));
    expect(await screen.findByRole("list", { name: "Setup steps" })).toBeInTheDocument();
    act(() => finishFakePython());
    await screen.findByRole("list", { name: "Installed packages" });

    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    const dialog = await screen.findByRole("dialog", { name: "Reset the Python environment?" });
    expect(fakePythonState().resets).toBe(0);
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(fakePythonState().resets).toBe(0);

    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    const again = await screen.findByRole("dialog", { name: "Reset the Python environment?" });
    fireEvent.click(within(again).getByRole("button", { name: "Reset" }));
    await waitFor(() => expect(fakePythonState().resets).toBe(1));
    expect(await screen.findByText("Not set up yet")).toBeInTheDocument();
  });
});

describe("Settings → Permissions → Python", () => {
  it("switches the tool off and saves the time limit", async () => {
    renderApp("/settings/permissions");
    const timeout = await screen.findByRole("spinbutton", { name: /Time limit per script/ });
    fireEvent.change(timeout, { target: { value: "9999" } });
    fireEvent.blur(timeout);
    await waitFor(() => expect(fakeState().settings.python.timeoutSecs).toBe(3600));

    fireEvent.click(screen.getByRole("switch", { name: "Let models run Python code" }));
    await waitFor(() => expect(fakeState().settings.python.enabled).toBe(false));
    expect(
      screen.queryByRole("spinbutton", { name: /Time limit per script/ }),
    ).not.toBeInTheDocument();
  });
});

describe("run_python in a reply", () => {
  it("asks with the code, shows the setup, and brings the chart and artifact back", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Plot it" } });
    fireEvent.keyDown(box, { key: "Enter" });
    const live = await screen.findByLabelText("Reply in progress");
    await within(live).findByText("Partial");
    const rid = heldReplyId("c2");

    act(() => {
      emitStream("c2", { type: "toolUse", id: "p1", name: "run_python" });
      emitStream("c2", {
        type: "toolCall",
        id: "p1",
        name: "run_python",
        server: null,
        tool: "run_python",
        title: "Run Python",
        inputJson: INPUT,
      });
      askApproval({
        conversationId: "c2",
        messageId: rid,
        toolUseId: "p1",
        server: null,
        tool: "run_python",
        title: "Run Python",
        description: "Run Python 3.12 code",
        inputJson: INPUT,
        readOnly: false,
        destructive: true,
        skill: null,
        requestedAt: Date.now(),
      });
    });
    const card = await screen.findByRole("group", { name: "Approve Run Python" });
    expect(within(card).getByLabelText("Code")).toHaveTextContent("print(pd.__version__)");
    expect(card).toHaveTextContent("not in a sandbox");
    fireEvent.click(within(card).getByRole("button", { name: "Allow always for this chat" }));
    await waitFor(() => expect(fakeState().answers.at(-1)?.decision).toBe("allowChat"));

    // The first run installs Python: the badge says so while the environment is installing.
    act(() => {
      fakePythonStep("python", "Installing Python");
      emitStream("c2", { type: "toolRunning", id: "p1" });
    });
    expect(await within(live).findByText("Setting up Python")).toBeInTheDocument();

    const ref = saveArtifact({
      conversationId: "c2",
      key: "table.csv",
      kind: "code",
      title: "table.csv",
      content: "a,b\n1,2",
      language: "csv",
      messageId: rid,
    });
    const content = [
      { type: "text" as const, text: "exit code 0\nstdout:\n2.2.3" },
      {
        type: "image" as const,
        mediaType: "image/png",
        source: { kind: "base64" as const, data: "AAAA" },
      },
      ref,
    ];
    act(() => {
      finishFakePython();
      emitStream("c2", { type: "toolResult", id: "p1", content, isError: false, durationMs: 4200 });
      finishReplyWith("c2", [
        {
          type: "toolUse",
          id: "p1",
          name: "run_python",
          inputJson: INPUT,
          server: null,
          tool: "run_python",
        },
        { type: "toolResult", toolUseId: "p1", content, isError: false },
        { type: "text", text: "Here is the chart." },
      ]);
    });
    const reply = (await screen.findByText("Here is the chart.")).closest("article") as HTMLElement;
    // Nothing is expanded, yet the chart and the artifact are visible.
    expect(within(reply).getByRole("button", { name: /Ran Python/ })).toHaveTextContent(
      "import pandas as pd",
    );
    expect(within(reply).getByAltText("Tool image 1")).toBeInTheDocument();
    fireEvent.click(within(reply).getByRole("button", { name: "Open artifact table.csv" }));
    expect(await screen.findByRole("heading", { name: "table.csv" })).toBeInTheDocument();
  });
});

describe("python helpers", () => {
  it("labels the call by its first line of code", () => {
    const call = { tool: "run_python", server: null, inputJson: INPUT };
    expect(toolKind(call)).toBe("python");
    expect(toolKind({ ...call, server: "x" })).toBe("connector");
    expect(pythonCode(INPUT)).toBe(CODE);
    expect(pythonCode('{"code":')).toBe("");
    expect(codeLabel("\n\n  x = 1\ny = 2")).toBe("x = 1");
    expect(codeLabel("x".repeat(100))).toBe(`${"x".repeat(80)}…`);
    const view = {
      id: "p1",
      name: "run_python",
      server: null,
      tool: "run_python",
      title: null,
      inputJson: INPUT,
      status: "done" as const,
      approval: null,
      result: null,
      isError: false,
      durationMs: null,
      provider: false,
    };
    expect(callHeadline(view)).toEqual({ action: "Ran Python", detail: "import pandas as pd" });
    expect(callHeadline({ ...view, status: "running" }).action).toBe("Running Python");
  });

  it("groups python uses in the details panel", () => {
    const rows = usageRows([
      {
        toolUseId: "a",
        kind: "python",
        server: null,
        tool: "run_python",
        label: "x = 1",
        isError: false,
      },
      {
        toolUseId: "b",
        kind: "python",
        server: null,
        tool: "run_python",
        label: "y = 2",
        isError: true,
      },
    ] as Parameters<typeof usageRows>[0]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      title: "Python",
      count: 2,
      errors: 1,
      details: ["x = 1", "y = 2"],
    });
  });
});
