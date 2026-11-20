import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { appPreviewRequest, fakeState } from "@/test/fakeBackend";
import {
  draftFile,
  fakeApps,
  fakeBundle,
  fakeManifest,
  modelWritesDraftFile,
  resetFakeApps,
} from "@/test/fakeApps";
import { renderApp } from "@/test/renderApp";
import { findAll } from "./editorText";
import { fileTree } from "./fileTree";

/** The preview's frame (the newest one: the preview reloads by remounting it). */
async function preview(name: string) {
  const frames = await screen.findAllByTitle(name);
  const iframe = frames[frames.length - 1] as HTMLIFrameElement;
  const win = iframe.contentWindow;
  if (!win) throw new Error("no frame window");
  const posted = vi.spyOn(win, "postMessage");
  const send = (data: unknown) =>
    act(async () => {
      window.dispatchEvent(new MessageEvent("message", { data, source: win }));
    });
  const loaded = () =>
    posted.mock.calls
      .map(([m]) => m as { type: string; files?: Array<{ path: string; content: string }> })
      .find((m) => m.type === "load");
  return { iframe, posted, send, loaded };
}

async function openNewApp() {
  const view = renderApp("/apps");
  fireEvent.click(await screen.findByRole("button", { name: /New app/ }));
  await waitFor(() => expect(view.location.pathname).toBe("/apps/new-app/build"));
  await screen.findByRole("region", { name: "Builder chat" });
  return view;
}

describe("the Utility App builder", () => {
  beforeEach(resetFakeApps);

  it("makes a new app as a draft with its own chat, outside the chat list", async () => {
    const { location } = await openNewApp();
    expect(screen.getByRole("button", { name: "App details" })).toHaveTextContent("Draft");
    expect(screen.getByText("Build New app with AI")).toBeInTheDocument();
    const chatId = fakeApps().builderChats.get("new-app") ?? "";
    expect(chatId).not.toBe("");
    // The builder chat answers in the builder, and never shows up in Recents.
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Make a CSV to chart app" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(await screen.findByText("Reply to: Make a CSV to chart app")).toBeInTheDocument();
    expect(fakeState().messages.some((m) => m.conversationId === chatId)).toBe(true);
    const sidebar = screen.getByRole("complementary", { name: "Sidebar" });
    expect(within(sidebar).queryByText(/Building New app/)).not.toBeInTheDocument();
    // Back in the gallery the unsaved app waits under Drafts.
    fireEvent.click(screen.getByRole("button", { name: "Close the builder" }));
    await waitFor(() => expect(location.pathname).toBe("/apps"));
    const drafts = await screen.findByRole("region", { name: "Drafts" });
    expect(within(drafts).getByRole("link", { name: /New app/ })).toHaveAttribute(
      "href",
      "/apps/new-app/build",
    );
  });

  it("Code saves a file, makes a checkpoint and the Preview runs the new code", async () => {
    await openNewApp();
    fireEvent.click(screen.getByRole("tab", { name: "Code" }));
    const editor = await screen.findByRole("textbox", { name: "App.tsx" });
    fireEvent.change(editor, { target: { value: "export default () => <p>Edited by hand</p>;" } });
    fireEvent.keyDown(editor, { key: "s", ctrlKey: true });
    await waitFor(() => expect(draftFile("new-app", "App.tsx")).toContain("Edited by hand"));
    expect(fakeApps().checkpoints.get("new-app")?.[0]?.label).toBe("Saved App.tsx");
    // The preview reloads with the saved file.
    fireEvent.click(screen.getByRole("tab", { name: "Preview" }));
    await waitFor(async () => {
      const f = await preview("New app");
      await f.send({ source: "oxide-app", type: "ready" });
      expect(f.loaded()?.files?.find((x) => x.path === "App.tsx")?.content).toContain(
        "Edited by hand",
      );
    });
  });

  it("follows the model's changes, and restores a checkpoint", async () => {
    await openNewApp();
    fireEvent.click(screen.getByRole("tab", { name: "Code" }));
    await screen.findByRole("textbox", { name: "App.tsx" });
    act(() => modelWritesDraftFile("new-app", "components/Chart.tsx", "export const Chart = 1;"));
    const files = screen.getByRole("navigation", { name: "Files" });
    expect(await within(files).findByRole("button", { name: /Chart\.tsx/ })).toBeInTheDocument();
    expect(within(files).getByText("components")).toBeInTheDocument();
    // Restore the start: the model's file goes away.
    fireEvent.keyDown(screen.getByRole("button", { name: "Checkpoints" }), { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitem", { name: /Start/ }));
    await waitFor(() => expect(draftFile("new-app", "components/Chart.tsx")).toBeUndefined());
    await waitFor(() =>
      expect(within(files).queryByRole("button", { name: /Chart\.tsx/ })).not.toBeInTheDocument(),
    );
  });

  it("answers the model's preview check with the app's errors and console", async () => {
    await openNewApp();
    await act(async () => {
      await appPreviewRequest.emit({ requestId: "r1", appId: "new-app", kind: "status" });
    });
    // The check reloads the preview: the new frame starts, logs and fails.
    await waitFor(async () => {
      const f = await preview("New app");
      await f.send({ source: "oxide-app", type: "ready" });
      expect(f.loaded()).toBeDefined();
    });
    const f = await preview("New app");
    await f.send({ source: "oxide-app", type: "console", level: "warn", text: "careful" });
    await f.send({ source: "oxide-app", type: "error", message: "Chart is not defined" });
    await waitFor(() => expect(fakeApps().previewAnswers).toHaveLength(1), { timeout: 4000 });
    const report = fakeApps().previewAnswers[0]?.report;
    expect(report?.errors).toEqual(["Chart is not defined"]);
    expect(report?.console).toEqual([{ level: "warn", text: "careful" }]);
    expect(report?.rendered).toBe(false);
    // The error bar offers Fix with AI, which asks the builder chat.
    fireEvent.click(await screen.findByRole("button", { name: /Fix with AI/ }));
    const chatId = fakeApps().builderChats.get("new-app");
    await waitFor(() =>
      expect(
        fakeState().messages.some(
          (m) =>
            m.conversationId === chatId &&
            m.content.some((b) => b.type === "text" && b.text.includes("Chart is not defined")),
        ),
      ).toBe(true),
    );
  });

  it("saves the draft to Utility Apps, and an own app's Edit opens the builder", async () => {
    const { location } = await openNewApp();
    fireEvent.click(screen.getByRole("button", { name: "App details" }));
    const dialog = await screen.findByRole("dialog", { name: "App details" });
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "CSV Chart" } });
    fireEvent.click(within(dialog).getByRole("option", { name: "chart-line" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save details" }));
    await waitFor(() =>
      expect(fakeApps().drafts.get("new-app")?.app.manifest.name).toBe("CSV Chart"),
    );
    expect(fakeApps().drafts.get("new-app")?.app.manifest.icon).toBe("chart-line");
    fireEvent.click(screen.getByRole("button", { name: /Save to Utility Apps/ }));
    await waitFor(() =>
      expect(fakeApps().apps.get("new-app")?.app.manifest.name).toBe("CSV Chart"),
    );
    // Nothing left to save: the button gives way to a quiet “Saved”.
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: /Save to Utility Apps/ }),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: "App details" })).toHaveTextContent("Saved");
    fireEvent.click(screen.getByRole("button", { name: "Close the builder" }));
    await waitFor(() => expect(location.pathname).toBe("/apps/new-app"));
    const toolbar = await screen.findByRole("toolbar", { name: "App actions" });
    fireEvent.click(within(toolbar).getByRole("button", { name: /Edit/ }));
    await waitFor(() => expect(location.pathname).toBe("/apps/new-app/build"));
    expect(await screen.findByRole("region", { name: "Builder chat" })).toBeInTheDocument();
  });

  it("an own app is changed in the builder, saved and exported", async () => {
    fakeApps().apps.set(
      "mine",
      fakeBundle(fakeManifest({ id: "mine", name: "Mine" }), false, [
        { path: "App.tsx", content: "export default () => 'old';" },
      ]),
    );
    const { location } = renderApp("/apps/mine/build");
    await screen.findByRole("region", { name: "Builder chat" });
    expect(screen.getByRole("button", { name: "App details" })).toHaveTextContent("Saved");
    fireEvent.click(screen.getByRole("tab", { name: "Code" }));
    const editor = await screen.findByRole("textbox", { name: "App.tsx" });
    fireEvent.change(editor, { target: { value: "export default () => 'new';" } });
    fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));
    expect(await screen.findByText("Unsaved changes")).toBeInTheDocument();
    // The gallery still runs the saved app until Save to Utility Apps.
    expect(
      fakeApps()
        .apps.get("mine")
        ?.files.find((f) => f.path === "App.tsx")?.content,
    ).toContain("old");
    fireEvent.click(screen.getByRole("button", { name: /Save to Utility Apps/ }));
    await waitFor(() =>
      expect(
        fakeApps()
          .apps.get("mine")
          ?.files.find((f) => f.path === "App.tsx")?.content,
      ).toContain("new"),
    );
    fireEvent.keyDown(screen.getByRole("button", { name: "More" }), { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitem", { name: /Export saved app/ }));
    await waitFor(() => expect(fakeApps().exported).toEqual(["mine"]));
    expect(location.pathname).toBe("/apps/mine/build");
  });
});

describe("builder helpers", () => {
  it("lists files as a tree, folders first", () => {
    expect(
      fileTree(["App.tsx", "components/Chart.tsx", "lib/csv.ts", "components/ui/Button.tsx"]).map(
        (r) => `${"  ".repeat(r.depth)}${r.name}${r.folder ? "/" : ""}`,
      ),
    ).toEqual([
      "components/",
      "  ui/",
      "    Button.tsx",
      "  Chart.tsx",
      "lib/",
      "  csv.ts",
      "App.tsx",
    ]);
  });

  it("finds text case-insensitively", () => {
    expect(findAll("Chart chart CHART", "chart")).toEqual([0, 6, 12]);
    expect(findAll("abc", "")).toEqual([]);
  });
});
