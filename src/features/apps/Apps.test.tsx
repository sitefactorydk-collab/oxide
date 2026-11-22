import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fakeApps, fakeBundle, fakeManifest, resetFakeApps } from "@/test/fakeApps";
import { setFakeMedia } from "@/test/fakeGeneration";
import { renderApp } from "@/test/renderApp";

/** The app's frame, and a spy on what Oxide posts to it. */
async function frame(name: string) {
  const iframe = (await screen.findByTitle(name)) as HTMLIFrameElement;
  const win = iframe.contentWindow;
  if (!win) throw new Error("no frame window");
  const posted = vi.spyOn(win, "postMessage");
  const send = (data: unknown) =>
    act(async () => {
      window.dispatchEvent(new MessageEvent("message", { data, source: win }));
    });
  const result = async (id: string) => {
    await waitFor(() =>
      expect(posted.mock.calls.some(([m]) => (m as { id?: string }).id === id)).toBe(true),
    );
    return posted.mock.calls.map(([m]) => m).find((m) => (m as { id?: string }).id === id) as {
      ok: boolean;
      value?: unknown;
      error?: { code: string };
    };
  };
  return { iframe, posted, send, result };
}

const actions = () => within(screen.getByRole("toolbar", { name: "App actions" }));

describe("Utility Apps", () => {
  // renderApp resets the rest of the fake backend; the apps reset here, so a test can set
  // them up before rendering.
  beforeEach(resetFakeApps);

  it("lists the built-in apps with search and categories", async () => {
    renderApp("/apps");
    expect(await screen.findByRole("heading", { name: "Utility Apps" })).toBeInTheDocument();
    await screen.findByRole("region", { name: "Built-in" });
    const builtIn = () => within(screen.getByRole("region", { name: "Built-in" }));
    expect(builtIn().getByRole("link", { name: /AI Meta Tag Generator/ })).toBeInTheDocument();
    expect(builtIn().getAllByRole("link")).toHaveLength(4);
    fireEvent.change(screen.getByLabelText("Search apps"), { target: { value: "svg" } });
    expect(builtIn().getByRole("link", { name: /AI SVG Creator/ })).toBeInTheDocument();
    expect(builtIn().queryByRole("link", { name: /Meta Tag/ })).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Search apps"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "SEO" }));
    expect(builtIn().getAllByRole("link")).toHaveLength(1);
    // The sidebar has the page after Projects.
    expect(screen.getByRole("link", { name: "Utility Apps" })).toBeInTheDocument();
  });

  it("runs an app: loads its files, asks once per capability, and remembers a denial", async () => {
    fakeApps().answer = (_id, method) =>
      method === "text.generate" ? { text: "hi", model: "lmstudio/fake-model" } : null;
    renderApp("/apps/ai-meta-tag-generator");
    const f = await frame("AI Meta Tag Generator");
    await f.send({ source: "oxide-app", type: "ready" });
    const load = f.posted.mock.calls
      .map(([m]) => m)
      .find((m) => (m as { type: string }).type === "load") as {
      entry: string;
      files: Array<{ path: string }>;
    };
    expect(load.entry).toBe("App.tsx");
    expect(load.files.map((x) => x.path)).toContain("App.tsx");

    // The first text call asks; Allow grants it and the call goes through.
    await f.send({
      source: "oxide-app",
      type: "call",
      id: "1",
      method: "text.generate",
      params: { prompt: "x" },
    });
    const card = await screen.findByRole("alertdialog");
    expect(card).toHaveTextContent("AI Meta Tag Generator wants to use your AI models");
    fireEvent.click(within(card).getByRole("button", { name: "Allow for this app" }));
    expect(await f.result("1")).toMatchObject({ ok: true, value: { text: "hi" } });
    expect(fakeApps().prefs.get("ai-meta-tag-generator")?.granted).toEqual(["text"]);

    // The second doesn't ask.
    await f.send({
      source: "oxide-app",
      type: "call",
      id: "2",
      method: "text.generate",
      params: { prompt: "y" },
    });
    expect(await f.result("2")).toMatchObject({ ok: true });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();

    // Deny holds for this visit.
    await f.send({
      source: "oxide-app",
      type: "call",
      id: "3",
      method: "web.fetch",
      params: { url: "https://a.b" },
    });
    fireEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Deny" }),
    );
    expect(await f.result("3")).toMatchObject({ ok: false, error: { code: "denied" } });
    await f.send({
      source: "oxide-app",
      type: "call",
      id: "4",
      method: "web.fetch",
      params: { url: "https://a.b" },
    });
    expect(await f.result("4")).toMatchObject({ ok: false, error: { code: "denied" } });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();

    // Not in the manifest: refused without asking.
    await f.send({
      source: "oxide-app",
      type: "call",
      id: "5",
      method: "python.run",
      params: { code: "1" },
    });
    expect(await f.result("5")).toMatchObject({ ok: false, error: { code: "undeclared" } });

    // Storage needs nothing; errors the app reports are shown.
    await f.send({ source: "oxide-app", type: "error", message: "boom" });
    expect(screen.getByText("The app reported an error: boom")).toBeInTheDocument();
  });

  it("turns generated files into blobs and saves files through Oxide", async () => {
    const id = "imager";
    fakeApps().apps.set(
      id,
      fakeBundle(fakeManifest({ id, name: "Imager", capabilities: ["image"] }), false),
    );
    fakeApps().prefs.set(id, {
      textModel: null,
      image: { providerId: null, baseUrl: "", model: "" },
      video: { providerId: null, baseUrl: "", model: "" },
      music: { providerId: null, baseUrl: "", model: "" },
      granted: ["image"],
      folder: null,
    });
    fakeApps().answer = () => ({
      files: [{ name: "cat.png", mediaType: "image/png", path: "/att/cat.png" }],
    });
    renderApp(`/apps/${id}`);
    // After renderApp, which resets the fake generation backend.
    setFakeMedia("/att/cat.png", btoa("png-bytes"));
    const f = await frame("Imager");
    await f.send({
      source: "oxide-app",
      type: "call",
      id: "g",
      method: "media.image",
      params: { prompt: "cat" },
    });
    const r = await f.result("g");
    expect(r).toMatchObject({ ok: true });
    const file = (r.value as { files: Array<{ blob: Blob; mediaType: string }> }).files[0];
    expect(file?.blob).toBeInstanceOf(Blob);
    expect(file?.blob.size).toBe("png-bytes".length);
    await f.send({
      source: "oxide-app",
      type: "call",
      id: "s",
      method: "files.save",
      params: { name: "a.txt", data: btoa("x") },
    });
    expect(await f.result("s")).toMatchObject({
      ok: true,
      value: { path: "/Users/test/Downloads/a.txt" },
    });
  });

  it("clones a built-in app into My apps, edits and exports it", async () => {
    const { location } = renderApp("/apps/ai-svg-creator");
    await screen.findByTitle("AI SVG Creator");
    expect(actions().queryByRole("button", { name: /Edit/ })).not.toBeInTheDocument();
    fireEvent.click(actions().getByRole("button", { name: /Clone/ }));
    await waitFor(() => expect(location.pathname).toBe("/apps/ai-svg-creator-copy"));
    expect(await screen.findByTitle("AI SVG Creator (copy)")).toBeInTheDocument();
    expect(screen.getByText(/My app ·/)).toBeInTheDocument();

    // Edit opens the builder; a change saved in Code reaches the app with Save to Utility Apps.
    fireEvent.click(actions().getByRole("button", { name: /Edit/ }));
    await waitFor(() => expect(location.pathname).toBe("/apps/ai-svg-creator-copy/build"));
    fireEvent.click(await screen.findByRole("tab", { name: "Code" }));
    const editor = await screen.findByRole("textbox", { name: "App.tsx" });
    fireEvent.change(editor, { target: { value: "export default () => 'changed';" } });
    fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));
    const publish = await screen.findByRole("button", { name: /Save to Utility Apps/ });
    await waitFor(() => expect(publish).toBeEnabled());
    fireEvent.click(publish);
    await waitFor(() =>
      expect(
        fakeApps()
          .apps.get("ai-svg-creator-copy")
          ?.files.find((x) => x.path === "App.tsx")?.content,
      ).toContain("changed"),
    );
    fireEvent.click(screen.getByRole("button", { name: "Close the builder" }));
    await waitFor(() => expect(location.pathname).toBe("/apps/ai-svg-creator-copy"));
    await screen.findByTitle("AI SVG Creator (copy)");

    fireEvent.click(actions().getByRole("button", { name: /Export/ }));
    await waitFor(() => expect(fakeApps().exported).toEqual(["ai-svg-creator-copy"]));
  });

  it("imports an app after showing what it may ask for", async () => {
    fakeApps().nextImport = fakeBundle(
      fakeManifest({ id: "shared", name: "Shared tool", capabilities: ["webSearch", "python"] }),
      false,
    );
    const { location } = renderApp("/apps");
    fireEvent.click(await screen.findByRole("button", { name: /Import app/ }));
    const dialog = await screen.findByRole("dialog", { name: "Import app" });
    expect(dialog).toHaveTextContent("Shared tool");
    expect(dialog).toHaveTextContent("search the web");
    expect(dialog).toHaveTextContent("run Python code on this computer");
    fireEvent.click(within(dialog).getByRole("button", { name: "Install" }));
    await waitFor(() => expect(location.pathname).toBe("/apps/shared"));
    expect(fakeApps().apps.has("shared")).toBe(true);
  });

  it("app settings pick a text model, override generation and revoke permissions", async () => {
    fakeApps().prefs.set("ai-meta-tag-generator", {
      textModel: null,
      image: { providerId: null, baseUrl: "", model: "" },
      video: { providerId: null, baseUrl: "", model: "" },
      music: { providerId: null, baseUrl: "", model: "" },
      granted: ["text", "webFetch"],
      folder: null,
    });
    renderApp("/apps/ai-meta-tag-generator");
    await screen.findByTitle("AI Meta Tag Generator");
    fireEvent.click(actions().getByRole("button", { name: /Settings/ }));
    const dialog = await screen.findByRole("dialog", { name: "AI Meta Tag Generator settings" });
    expect(await within(dialog).findByLabelText("Text model")).toBeInTheDocument();
    // The app declares no generation, so no overrides are offered.
    expect(within(dialog).queryByText("Image generation")).not.toBeInTheDocument();
    const rows = within(dialog).getAllByRole("button", { name: "Revoke" });
    expect(rows).toHaveLength(2);
    fireEvent.click(rows[0] as HTMLElement);
    await waitFor(() =>
      expect(fakeApps().prefs.get("ai-meta-tag-generator")?.granted).toEqual(["webFetch"]),
    );
  });
  it("an app's folder is picked in the app or in its settings, which allows it", async () => {
    const id = "folder-app";
    fakeApps().apps.set(
      id,
      fakeBundle(fakeManifest({ id, name: "Folder app", capabilities: ["folder"] }), false),
    );
    fakeApps().answer = (appId, method) =>
      method === "folder.get"
        ? { path: fakeApps().prefs.get(appId)?.folder, name: "addons", exists: true }
        : null;
    renderApp(`/apps/${id}`);
    const f = await frame("Folder app");
    // Without a folder, the folder's methods need the permission picking it gives.
    await f.send({
      source: "oxide-app",
      type: "call",
      id: "c0",
      method: "folder.choose",
      params: {},
    });
    expect(await f.result("c0")).toMatchObject({ ok: true, value: null });
    fakeApps().nextFolder = "C:\\work\\addons";
    await f.send({
      source: "oxide-app",
      type: "call",
      id: "c1",
      method: "folder.choose",
      params: {},
    });
    expect(await f.result("c1")).toMatchObject({
      ok: true,
      value: { path: "C:\\work\\addons", name: "addons" },
    });
    expect(fakeApps().prefs.get(id)?.granted).toContain("folder");
    await f.send({
      source: "oxide-app",
      type: "call",
      id: "c2",
      method: "folder.write",
      params: { path: "a/b.txt", text: "hi" },
    });
    expect(await f.result("c2")).toMatchObject({ ok: true });

    // The settings dialog shows it; Remove forgets it and the permission.
    fireEvent.click(actions().getByRole("button", { name: /Settings/ }));
    const dialog = await screen.findByRole("dialog", { name: "Folder app settings" });
    expect(await within(dialog).findByText("C:\\work\\addons")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    expect(await within(dialog).findByText("No folder chosen")).toBeInTheDocument();
    expect(fakeApps().prefs.get(id)?.folder).toBeNull();
    expect(fakeApps().prefs.get(id)?.granted).not.toContain("folder");
    fakeApps().nextFolder = "D:\\other";
    fireEvent.click(within(dialog).getByRole("button", { name: "Choose…" }));
    expect(await within(dialog).findByText("D:\\other")).toBeInTheDocument();
    expect(fakeApps().prefs.get(id)?.folder).toBe("D:\\other");
  });
});
