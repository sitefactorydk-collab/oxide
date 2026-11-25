import * as Babel from "@babel/standalone";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import JSZip from "jszip";
import * as JSZipNs from "jszip";
import * as Lucide from "lucide-react";
import * as React from "react";
import * as ReactMarkdown from "react-markdown";
import * as RemarkGfm from "remark-gfm";
import { describe, expect, it, vi } from "vitest";
import type { Transform } from "@/frame/run";
import { appComponent, loadApp } from "@/appframe/loader";
import sppbApp from "../../../utility-apps/sp-page-builder-addon-creator/App.tsx?raw";
import sppbBench from "../../../utility-apps/sp-page-builder-addon-creator/Workbench.tsx?raw";
import elementorApp from "../../../utility-apps/elementor-widget-creator/App.tsx?raw";
import elementorBench from "../../../utility-apps/elementor-widget-creator/Workbench.tsx?raw";

// The SP Page Builder Addon Creator and the Elementor Widget Creator, with a fake SDK whose
// `folder` is an in-memory folder.

const transform = ((Babel as { transform?: Transform }).transform ??
  (Babel as unknown as { default: { transform: Transform } }).default.transform) as Transform;

const esm = (ns: object) => ({
  ...ns,
  __esModule: true,
  default: (ns as { default?: unknown }).default ?? ns,
});

type Data = string | Uint8Array;

/** An in-memory folder with the SDK's `folder` methods (paths relative to it). */
function memoryFolder(files: Record<string, Data> = {}, chosen = true) {
  const fs = new Map<string, Data>(Object.entries(files));
  let info: { path: string; name: string; exists: boolean } | null = chosen
    ? { path: "C:\\work\\addons", name: "addons", exists: true }
    : null;
  const under = (p: string, base: string) => (base ? p.startsWith(`${base}/`) : true);
  const api = {
    get: vi.fn(async () => info),
    choose: vi.fn(async () => {
      info = { path: "C:\\work\\addons", name: "addons", exists: true };
      return info;
    }),
    list: vi.fn(async (path = "", o: { depth?: number } = {}) => {
      const depth = o.depth ?? 1;
      const out = new Map<string, { path: string; kind: "file" | "dir"; size: number }>();
      for (const [p, v] of fs) {
        if (!under(p, path)) continue;
        const rest = path ? p.slice(path.length + 1) : p;
        const parts = rest.split("/");
        for (let i = 1; i < parts.length && i <= depth; i++) {
          const d = [path, ...parts.slice(0, i)].filter(Boolean).join("/");
          out.set(d, { path: d, kind: "dir", size: 0 });
        }
        if (parts.length <= depth) {
          out.set(p, { path: p, kind: "file", size: typeof v === "string" ? v.length : v.length });
        }
      }
      return [...out.values()]
        .sort((a, b) => a.path.localeCompare(b.path))
        .map((e) => ({ ...e, modified: 0 }));
    }),
    readText: vi.fn(async (p: string) => {
      const v = fs.get(p);
      if (v === undefined) throw new Error(`${p} doesn't exist`);
      return typeof v === "string" ? v : new TextDecoder().decode(v);
    }),
    readBytes: vi.fn(async (p: string) => {
      const v = fs.get(p);
      if (v === undefined) throw new Error(`${p} doesn't exist`);
      return typeof v === "string" ? new TextEncoder().encode(v) : v;
    }),
    write: vi.fn(async (p: string, d: Data | Blob) => {
      fs.set(p, d instanceof Blob ? new Uint8Array(await d.arrayBuffer()) : d);
    }),
    mkdir: vi.fn(async () => undefined),
    delete: vi.fn(async (p: string) => {
      for (const k of [...fs.keys()]) if (k === p || k.startsWith(`${p}/`)) fs.delete(k);
    }),
    rename: vi.fn(async (from: string, to: string) => {
      for (const [k, v] of [...fs]) {
        if (k === from || k.startsWith(`${from}/`)) {
          fs.delete(k);
          fs.set(to + k.slice(from.length), v);
        }
      }
    }),
  };
  return { api, fs };
}

function fakeSdk(folder: ReturnType<typeof memoryFolder>["api"]) {
  const store = new Map<string, unknown>();
  return {
    ai: {
      text: vi.fn(),
      json: vi.fn(),
      edit: vi.fn(),
      info: vi.fn(async () => ({ textModel: "lmstudio/fake-model" })),
    },
    files: {
      pick: vi.fn(async () => [] as File[]),
      save: vi.fn(async () => true),
      readText: vi.fn(async (b: Blob) => b.text()),
    },
    folder,
    storage: {
      get: vi.fn(async (k: string) => store.get(k)),
      set: vi.fn(async (k: string, v: unknown) => void store.set(k, v)),
      delete: vi.fn(async (k: string) => void store.delete(k)),
    },
    app: { copy: vi.fn(async () => undefined), id: "x", name: "x", theme: "light" },
  };
}

function mount(app: string, bench: string, sdk: ReturnType<typeof fakeSdk>) {
  const exports = loadApp(
    [
      { path: "App.tsx", content: app },
      { path: "Workbench.tsx", content: bench },
    ],
    "App.tsx",
    {
      transform,
      modules: {
        react: esm(React),
        "lucide-react": esm(Lucide),
        "react-markdown": esm(ReactMarkdown),
        "remark-gfm": esm(RemarkGfm),
        jszip: esm(JSZipNs),
        oxide: esm(sdk),
      },
    },
  );
  const App = appComponent(exports) as React.ComponentType;
  return render(<App />);
}

/** A small, valid SP Page Builder addon plugin (`c10faq`), its paths inside the plugin. */
function sppbFiles(el = "c10faq", P = "C10faq"): Record<string, string> {
  const up = el.toUpperCase();
  return {
    [`${el}.xml`]: `<?xml version="1.0" encoding="utf-8"?>
<extension type="plugin" group="sppagebuilder" method="upgrade">
	<name>PLG_SPPAGEBUILDER_${up}</name>
	<version>1.1.0</version>
	<description>PLG_SPPAGEBUILDER_${up}_XML_DESCRIPTION</description>
	<namespace path="src">C10\\Plugin\\Sppagebuilder\\${P}</namespace>
	<scriptfile>script.php</scriptfile>
	<files>
		<folder plugin="${el}">services</folder>
		<folder>src</folder>
		<folder>addons</folder>
		<folder>language</folder>
	</files>
</extension>`,
    "script.php": "<?php \\defined('_JEXEC') or die;",
    "services/provider.php": `<?php use C10\\Plugin\\Sppagebuilder\\${P}\\Extension\\${P};`,
    [`src/Extension/${P}.php`]: `<?php namespace C10\\Plugin\\Sppagebuilder\\${P}\\Extension; final class ${P} {}`,
    [`language/en-GB/plg_sppagebuilder_${el}.sys.ini`]: `PLG_SPPAGEBUILDER_${up}="SP Page Builder - C10 FAQ widget"\nPLG_SPPAGEBUILDER_${up}_XML_DESCRIPTION="FAQ with search."`,
    [`addons/${el}/admin.php`]: `<?php SpAddonsConfig::addonConfig(['type' => 'repeatable', 'addon_name' => '${el}', 'title' => 'C10 FAQ widget']);`,
    [`addons/${el}/site.php`]: `<?php class SppagebuilderAddon${P} extends SppagebuilderAddons { public function render() { return '<div class="sppb-addon-${el} ${el}__grid"></div>'; } }`,
  };
}

/** The file a per-file request asks for. */
const askedFile = (prompt: string) =>
  prompt.match(/Write the complete (?:content of|new version of) (\S+) \(/)?.[1] ?? "";

/** `ai.text` answering each file request from `files` (as `filesFor(path)`). */
const writesFiles =
  (filesFor: (path: string) => string) =>
  async (o: { prompt: string; onText?: (d: string, t: string) => void }) => {
    const text = filesFor(askedFile(o.prompt));
    o.onText?.(text, text);
    return { text, model: "lmstudio/fake-model", truncated: false };
  };

/** What `ai.edit` answers: the new content, edited with `n` blocks or rewritten. */
const edited = (content: string, n = 1, rewritten = false) => ({
  content,
  changed: true,
  edits: rewritten
    ? []
    : Array.from({ length: n }, () => ({ old: "x", new: "y", replaceAll: false })),
  ranges: [],
  summary: "",
  rewritten,
  retried: false,
  model: "lmstudio/fake-model",
});

const inFolder = (folderName: string, files: Record<string, string>) =>
  Object.fromEntries(Object.entries(files).map(([p, v]) => [`${folderName}/${p}`, v]));

describe("the plugin creator apps", () => {
  it("both apps carry the same workbench", () => {
    expect(elementorBench).toBe(sppbBench);
  });

  it("SP Page Builder: asks for a folder, generates an addon and opens it in the editor", async () => {
    const { api, fs } = memoryFolder({}, false);
    const sdk = fakeSdk(api);
    const files = sppbFiles();
    sdk.ai.json.mockResolvedValue({
      summary: "Made the FAQ addon.",
      files: Object.keys(files).map((path) => ({ path, action: "create", purpose: `the ${path}` })),
    });
    sdk.ai.text.mockImplementation(writesFiles((path) => files[path] ?? ""));
    mount(sppbApp, sppbBench, sdk);
    fireEvent.click(await screen.findByRole("button", { name: /Choose folder/ }));
    expect(await screen.findByText(/No addons in addons yet/)).toBeInTheDocument();
    expect(api.choose).toHaveBeenCalled();

    fireEvent.click(screen.getAllByRole("button", { name: /New addon/ })[0] as HTMLElement);
    const dialog = screen.getByRole("dialog", { name: "New addon" });
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "C10 FAQ" } });
    expect(within(dialog).getByText("Folder: plg_sppagebuilder_c10faq")).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText("What should it do and look like?"), {
      target: { value: "An FAQ with categories" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: /Generate addon/ }));

    // The editor: the addon's name from its language file, its files, no problems.
    expect(
      await screen.findByRole("heading", { level: 1, name: /C10 FAQ widget/ }),
    ).toBeInTheDocument();
    const req = sdk.ai.json.mock.calls[0]?.[0] as { system: string; prompt: string };
    expect(req.system).toContain("SppagebuilderAddon");
    expect(req.system).toContain("Joomla 6");
    expect(req.prompt).toContain("Element name (E): c10faq");
    expect(req.prompt).toContain("An FAQ with categories");
    expect(req.prompt).toContain("Step 1: the plan");
    // Then one plain-text request per planned file, each with the plan and the files so far.
    const fileReqs = sdk.ai.text.mock.calls.map((c) => (c[0] as { prompt: string }).prompt);
    expect(fileReqs).toHaveLength(Object.keys(files).length);
    expect(fileReqs[0]).toContain("## The plan");
    expect(fileReqs[0]).toContain("Write the complete content of c10faq.xml");
    expect(fileReqs[1]).toContain("--- FILE: c10faq.xml ---");
    expect(fs.has("plg_sppagebuilder_c10faq/addons/c10faq/site.php")).toBe(true);
    expect(screen.queryByText(/things? to check/)).not.toBeInTheDocument();
    expect(screen.getByText("Made the FAQ addon.")).toBeInTheDocument();

    // Download installable zip: the plugin's folder inside, named with the version.
    fireEvent.click(screen.getByRole("button", { name: /Download installable zip/ }));
    await waitFor(() => expect(sdk.files.save).toHaveBeenCalled());
    const [name, bytes] = sdk.files.save.mock.calls[0] as unknown as [string, Uint8Array];
    expect(name).toBe("plg_sppagebuilder_c10faq-v1.1.0.zip");
    const zip = await JSZip.loadAsync(bytes);
    expect(zip.file("plg_sppagebuilder_c10faq/c10faq.xml")).toBeTruthy();
    expect(zip.file("plg_sppagebuilder_c10faq/addons/c10faq/admin.php")).toBeTruthy();
  });

  it("SP Page Builder: edits a file by hand, changes it with AI and undoes the change", async () => {
    const { api, fs } = memoryFolder(inFolder("plg_sppagebuilder_c10faq", sppbFiles()));
    const sdk = fakeSdk(api);
    mount(sppbApp, sppbBench, sdk);
    fireEvent.click(await screen.findByRole("button", { name: "Edit C10 FAQ widget" }));
    expect(
      await screen.findByRole("heading", { level: 1, name: /C10 FAQ widget/ }),
    ).toBeInTheDocument();

    // By hand: Ctrl+S saves the file.
    fireEvent.click(screen.getByRole("button", { name: "site.php" }));
    const code = screen.getByLabelText("Code") as HTMLTextAreaElement;
    expect(code.value).toContain("class SppagebuilderAddonC10faq");
    fireEvent.change(code, { target: { value: `${code.value}\n// mine` } });
    expect(screen.getByText("● unsaved")).toBeInTheDocument();
    fireEvent.keyDown(code, { key: "s", ctrlKey: true });
    await waitFor(() =>
      expect(fs.get("plg_sppagebuilder_c10faq/addons/c10faq/site.php")).toContain("// mine"),
    );

    // With AI: the files go to the model, the answer is written, Undo puts them back.
    sdk.ai.json.mockResolvedValue({
      summary: "Added a title color.",
      files: [
        { path: "addons/c10faq/admin.php", action: "update", purpose: "a title color field" },
        {
          path: "plg_sppagebuilder_c10faq/addons/c10faq/assets/css/c10faq.css",
          action: "create",
          purpose: "the title color",
        },
      ],
    });
    // The existing file is changed through ai.edit, the new one written whole.
    sdk.ai.edit.mockResolvedValue(edited("<?php // with color\n"));
    sdk.ai.text.mockImplementation(writesFiles(() => "```css\n.x{}\n```"));
    fireEvent.change(screen.getByLabelText("What to change"), {
      target: { value: "Add a title color" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Apply changes/ }));
    expect(await screen.findByText("Added a title color.")).toBeInTheDocument();
    const req = sdk.ai.json.mock.calls[0]?.[0] as { prompt: string };
    expect(req.prompt).toContain("--- FILE: addons/c10faq/site.php ---");
    expect(req.prompt).toContain("// mine");
    expect(req.prompt).toContain("Add a title color");
    const ed = sdk.ai.edit.mock.calls[0]?.[0] as {
      path: string;
      content: string;
      request: string;
      context: string;
      mode: string;
    };
    expect(ed.path).toBe("addons/c10faq/admin.php");
    expect(ed.content).toContain("addon_name");
    expect(ed.mode).toBe("edit");
    expect(ed.request).toContain("a title color field");
    expect(ed.context).toContain("## The plan");
    expect(ed.context).toContain("--- FILE: addons/c10faq/site.php ---");
    const second = (sdk.ai.text.mock.calls[0]?.[0] as { prompt: string }).prompt;
    expect(second).toContain("Write the complete content of addons/c10faq/assets/css/c10faq.css");
    expect(second).toContain("--- FILE: addons/c10faq/admin.php ---\n<?php // with color");
    expect(fs.get("plg_sppagebuilder_c10faq/addons/c10faq/admin.php")).toBe(
      "<?php // with color\n",
    );
    expect(fs.get("plg_sppagebuilder_c10faq/addons/c10faq/assets/css/c10faq.css")).toBe(".x{}\n");

    fireEvent.click(screen.getByRole("button", { name: /Undo/ }));
    await waitFor(() =>
      expect(fs.get("plg_sppagebuilder_c10faq/addons/c10faq/admin.php")).toContain("addon_name"),
    );
    expect(fs.has("plg_sppagebuilder_c10faq/addons/c10faq/assets/css/c10faq.css")).toBe(false);
  });

  it("SP Page Builder: a file cut off at the output limit stops the change, Undo keeps working", async () => {
    const { api, fs } = memoryFolder(inFolder("plg_sppagebuilder_c10faq", sppbFiles()));
    const sdk = fakeSdk(api);
    mount(sppbApp, sppbBench, sdk);
    fireEvent.click(await screen.findByRole("button", { name: "Edit C10 FAQ widget" }));
    await screen.findByRole("heading", { level: 1, name: /C10 FAQ widget/ });
    sdk.ai.json.mockResolvedValue({
      summary: "A new layout.",
      files: [
        { path: "addons/c10faq/admin.php", action: "update", purpose: "layout fields" },
        { path: "addons/c10faq/site.php", action: "update", purpose: "the new layout" },
        { path: "script.php", action: "delete", purpose: "not needed" },
      ],
    });
    sdk.ai.edit
      .mockResolvedValueOnce(edited("<?php // new admin\n"))
      .mockRejectedValueOnce(
        new Error(
          "The model's rewrite of addons/c10faq/site.php was cut off at its output limit after 16 characters, so the file wasn't changed.",
        ),
      );
    fireEvent.change(screen.getByLabelText("What to change"), {
      target: { value: "A new layout" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Apply changes/ }));
    // In the error bar and in the history.
    expect(
      await screen.findAllByText(/Stopped after 1 file: addons\/c10faq\/site.php: .*was cut off/),
    ).toHaveLength(2);
    expect(fs.get("plg_sppagebuilder_c10faq/addons/c10faq/admin.php")).toBe("<?php // new admin\n");
    expect(fs.get("plg_sppagebuilder_c10faq/addons/c10faq/site.php")).toContain(
      "SppagebuilderAddon",
    );
    expect(fs.has("plg_sppagebuilder_c10faq/script.php")).toBe(true);
    // The request stays for another try; Undo puts back the written file.
    expect((screen.getByLabelText("What to change") as HTMLTextAreaElement).value).toBe(
      "A new layout",
    );
    fireEvent.click(screen.getByRole("button", { name: /Undo/ }));
    await waitFor(() =>
      expect(fs.get("plg_sppagebuilder_c10faq/addons/c10faq/admin.php")).toContain("addon_name"),
    );
  });

  it("SP Page Builder: edits one function of site.php with edit blocks and shows a rewrite fallback", async () => {
    const site = [
      "<?php",
      "class SppagebuilderAddonC10faq extends SppagebuilderAddons {",
      "\tpublic function render() {",
      "\t\treturn '<div class=\"c10faq\"></div>';",
      "\t}",
      "",
      "\tpublic function stylesheets() {",
      "\t\treturn [];",
      "\t}",
      "}",
      "",
    ].join("\r\n");
    const files = { ...sppbFiles(), "addons/c10faq/site.php": site };
    const { api, fs } = memoryFolder(inFolder("plg_sppagebuilder_c10faq", files));
    const sdk = fakeSdk(api);
    mount(sppbApp, sppbBench, sdk);
    fireEvent.click(await screen.findByRole("button", { name: "Edit C10 FAQ widget" }));
    await screen.findByRole("heading", { level: 1, name: /C10 FAQ widget/ });
    sdk.ai.json.mockResolvedValue({
      summary: "Added a subtitle.",
      files: [
        { path: "addons/c10faq/site.php", action: "edit", purpose: "render the subtitle" },
        { path: "addons/c10faq/admin.php", action: "edit", purpose: "a subtitle field" },
      ],
    });
    // site.php: one block replacing render()'s body (as the engine would); admin.php: the
    // blocks didn't apply, so it came back rewritten. The second answer waits, so the
    // progress list can be read.
    const before = "\t\treturn '<div class=\"c10faq\"></div>';";
    const after = "\t\treturn '<div class=\"c10faq\"><p>' . $subtitle . '</p></div>';";
    sdk.ai.edit.mockImplementationOnce(
      async (o: { content: string; onStage?: (s: string) => void }) => {
        o.onStage?.("edits");
        return edited(o.content.replace(before, after));
      },
    );
    let finish: (v: unknown) => void = () => undefined;
    sdk.ai.edit.mockImplementationOnce((o: { onStage?: (s: string, n: string | null) => void }) => {
      o.onStage?.("edits", null);
      o.onStage?.("retry", "not found");
      o.onStage?.("rewrite", "the corrected edit blocks didn't apply either");
      return new Promise((resolve) => {
        finish = resolve;
      });
    });
    fireEvent.change(screen.getByLabelText("What to change"), {
      target: { value: "Add a subtitle field" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Apply changes/ }));

    const progress = await screen.findByLabelText("Progress");
    expect(await within(progress).findByText("edited (1 change)")).toBeInTheDocument();
    expect(within(progress).getByText("rewriting")).toBeInTheDocument();
    // Only render() changed; everything else in site.php is byte for byte the same.
    const written = fs.get("plg_sppagebuilder_c10faq/addons/c10faq/site.php") as string;
    expect(written).toBe(site.replace(before, after));
    expect(written.split("\r\n").filter((l, i) => l !== site.split("\r\n")[i])).toEqual([after]);
    const asked = sdk.ai.edit.mock.calls.map(
      (c) => c[0] as { path: string; mode: string; content: string },
    );
    expect(asked.map((a) => [a.path, a.mode])).toEqual([
      ["addons/c10faq/site.php", "edit"],
      ["addons/c10faq/admin.php", "edit"],
    ]);

    await act(async () => finish(edited("<?php // rewritten admin\n", 0, true)));
    expect(await screen.findByText("Added a subtitle.")).toBeInTheDocument();
    expect(fs.get("plg_sppagebuilder_c10faq/addons/c10faq/admin.php")).toBe(
      "<?php // rewritten admin\n",
    );
    // The history says what was done to each file.
    expect(
      screen.getByText(
        "addons/c10faq/site.php (edited, 1 change), addons/c10faq/admin.php (rewritten, fallback)",
      ),
    ).toBeInTheDocument();
    expect(sdk.ai.text).not.toHaveBeenCalled();
  });

  it("SP Page Builder: a planned rewrite writes the whole file through ai.edit", async () => {
    const { api, fs } = memoryFolder(inFolder("plg_sppagebuilder_c10faq", sppbFiles()));
    const sdk = fakeSdk(api);
    mount(sppbApp, sppbBench, sdk);
    fireEvent.click(await screen.findByRole("button", { name: "Edit C10 FAQ widget" }));
    await screen.findByRole("heading", { level: 1, name: /C10 FAQ widget/ });
    sdk.ai.json.mockResolvedValue({
      summary: "New layout.",
      files: [{ path: "addons/c10faq/site.php", action: "rewrite", purpose: "a new layout" }],
    });
    sdk.ai.edit.mockResolvedValue(edited("<?php // all new\n", 0, true));
    fireEvent.change(screen.getByLabelText("What to change"), {
      target: { value: "Redo the layout" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Apply changes/ }));
    expect(await screen.findByText("New layout.")).toBeInTheDocument();
    expect((sdk.ai.edit.mock.calls[0]?.[0] as { mode: string }).mode).toBe("rewrite");
    expect(fs.get("plg_sppagebuilder_c10faq/addons/c10faq/site.php")).toBe("<?php // all new\n");
    expect(screen.getByText("addons/c10faq/site.php (rewritten)")).toBeInTheDocument();
  });

  it("SP Page Builder: the browser clones (renaming the element), deletes and imports", async () => {
    const { api, fs } = memoryFolder(inFolder("plg_sppagebuilder_c10faq", sppbFiles()));
    const sdk = fakeSdk(api);
    mount(sppbApp, sppbBench, sdk);
    expect(await screen.findByText("C10 FAQ widget")).toBeInTheDocument();
    expect(screen.getByText("v1.1.0")).toBeInTheDocument();
    expect(screen.getByText("FAQ with search.")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Clone C10 FAQ widget" }));
    const dialog = screen.getByRole("dialog", { name: /Clone/ });
    fireEvent.change(within(dialog).getByLabelText("Name of the copy"), {
      target: { value: "Team FAQ" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: /Clone/ }));
    await waitFor(() => expect(fs.has("plg_sppagebuilder_teamfaq/teamfaq.xml")).toBe(true));
    const site = fs.get("plg_sppagebuilder_teamfaq/addons/teamfaq/site.php") as string;
    expect(site).toContain("class SppagebuilderAddonTeamfaq");
    expect(site).toContain("sppb-addon-teamfaq teamfaq__grid");
    const xml = fs.get("plg_sppagebuilder_teamfaq/teamfaq.xml") as string;
    expect(xml).toContain("PLG_SPPAGEBUILDER_TEAMFAQ");
    expect(xml).toContain('plugin="teamfaq"');
    expect(xml).toContain("Sppagebuilder\\Teamfaq");
    expect(fs.has("plg_sppagebuilder_teamfaq/src/Extension/Teamfaq.php")).toBe(true);
    expect(fs.get("plg_sppagebuilder_c10faq/addons/c10faq/site.php")).toContain("C10faq");

    // Back in the browser: delete the original.
    fireEvent.click(await screen.findByRole("button", { name: "Back to addons" }));
    fireEvent.click(await screen.findByRole("button", { name: "Delete C10 FAQ widget" }));
    fireEvent.click(
      within(screen.getByRole("dialog", { name: /Delete/ })).getByRole("button", {
        name: /Delete/,
      }),
    );
    await waitFor(() => expect(api.delete).toHaveBeenCalledWith("plg_sppagebuilder_c10faq"));

    // Import: a ZIP with the plugin's folder inside.
    const zip = new JSZip();
    for (const [p, v] of Object.entries(sppbFiles("hello", "Hello"))) {
      zip.file(`plg_sppagebuilder_hello/${p}`, v);
    }
    zip.file("plg_sppagebuilder_hello/addons/hello/assets/images/icon.png", new Uint8Array([1, 2]));
    zip.file("__MACOSX/._junk", "x");
    const bytes = await zip.generateAsync({ type: "arraybuffer" });
    sdk.files.pick.mockResolvedValue([new File([bytes], "hello.zip")]);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Import ZIP/ }));
    });
    await waitFor(() => expect(fs.has("plg_sppagebuilder_hello/hello.xml")).toBe(true));
    expect(fs.get("plg_sppagebuilder_hello/addons/hello/assets/images/icon.png")).toEqual(
      new Uint8Array([1, 2]),
    );
    expect([...fs.keys()].some((k) => k.includes("__MACOSX"))).toBe(false);
  });

  it("Elementor: imports a widget plugin, flags problems and clones it under a new slug", async () => {
    const { api, fs } = memoryFolder();
    const sdk = fakeSdk(api);
    const zip = new JSZip();
    zip.file(
      "c10-faq/c10-faq.php",
      `<?php
/**
 * Plugin Name: C10 FAQ
 * Description: An FAQ widget.
 * Version: 2.0.0
 * Text Domain: c10-faq
 */
if ( ! defined( 'ABSPATH' ) ) exit;
define( 'C10_FAQ_VERSION', '2.0.0' );
add_action( 'elementor/widgets/register', function ( $m ) { $m->register( new \\C10Faq\\C10_Faq_Widget() ); } );`,
    );
    zip.file(
      "c10-faq/widgets/class-c10-faq-widget.php",
      `<?php
namespace C10Faq;
if ( ! defined( 'ABSPATH' ) ) exit;
class C10_Faq_Widget extends \\Elementor\\Widget_Base {
  public function get_name(): string { return 'c10_faq'; }
  protected function render(): void { echo '<div class="c10-faq">'; }
}`,
    );
    sdk.files.pick.mockResolvedValue([
      new File([await zip.generateAsync({ type: "arraybuffer" })], "c10-faq-2.0.0.zip"),
    ]);
    mount(elementorApp, elementorBench, sdk);
    expect(await screen.findByText(/No widgets in addons yet/)).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(screen.getAllByRole("button", { name: /Import ZIP/ })[0] as HTMLElement);
    });
    expect(await screen.findByRole("heading", { level: 1, name: /C10 FAQ/ })).toBeInTheDocument();
    expect(fs.has("c10-faq/widgets/class-c10-faq-widget.php")).toBe(true);
    // The header lacks "Requires Plugins: elementor".
    expect(screen.getByText(/1 thing to check/)).toBeInTheDocument();
    expect(screen.getByText(/Requires Plugins: elementor/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^Clone$/ }));
    const dialog = screen.getByRole("dialog", { name: /Clone/ });
    fireEvent.change(within(dialog).getByLabelText("Name of the copy"), {
      target: { value: "Team FAQ" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: /Clone/ }));
    await waitFor(() => expect(fs.has("team-faq/team-faq.php")).toBe(true));
    const main = fs.get("team-faq/team-faq.php") as string;
    expect(main).toContain("Plugin Name: Team FAQ");
    expect(main).toContain("TEAM_FAQ_VERSION");
    expect(main).toContain("\\TeamFaq\\Team_Faq_Widget");
    expect(main).toContain("Text Domain: team-faq");
    const widget = fs.get("team-faq/widgets/class-team-faq-widget.php") as string;
    expect(widget).toContain("return 'team_faq';");
    expect(widget).toContain('class="team-faq"');

    // The ZIP has the slug folder inside.
    fireEvent.click(await screen.findByRole("button", { name: /Download installable zip/ }));
    await waitFor(() => expect(sdk.files.save).toHaveBeenCalled());
    const [name, bytes] = sdk.files.save.mock.calls[0] as unknown as [string, Uint8Array];
    expect(name).toBe("team-faq-2.0.0.zip");
    expect((await JSZip.loadAsync(bytes)).file("team-faq/team-faq.php")).toBeTruthy();
  });

  it("Elementor: a new widget's prompt names every identifier from the slug", async () => {
    const { api } = memoryFolder();
    const sdk = fakeSdk(api);
    sdk.ai.json.mockRejectedValue(Object.assign(new Error("Stopped."), { code: "cancelled" }));
    mount(elementorApp, elementorBench, sdk);
    fireEvent.click(
      (await screen.findAllByRole("button", { name: /New widget/ }))[0] as HTMLElement,
    );
    const dialog = screen.getByRole("dialog", { name: "New widget" });
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Price Table" } });
    fireEvent.change(within(dialog).getByLabelText("What should it do and look like?"), {
      target: { value: "Three plans" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: /Generate widget/ }));
    await waitFor(() => expect(sdk.ai.json).toHaveBeenCalled());
    const req = sdk.ai.json.mock.calls[0]?.[0] as { system: string; prompt: string };
    expect(req.system).toContain("\\Elementor\\Widget_Base");
    expect(req.system).toContain("elementor/widgets/register");
    expect(req.prompt).toContain("Slug (S): price-table");
    expect(req.prompt).toContain("Class name: Price_Table_Widget");
    expect(req.prompt).toContain("Namespace: PriceTable");
  });
});
