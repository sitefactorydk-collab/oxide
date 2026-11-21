import * as Babel from "@babel/standalone";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import JSZip from "jszip";
import * as JSZipNs from "jszip";
import * as Lucide from "lucide-react";
import * as React from "react";
import * as ReactMarkdown from "react-markdown";
import * as RemarkGfm from "remark-gfm";
import { describe, expect, it, vi } from "vitest";
import type { Transform } from "@/frame/run";
import { appComponent, loadApp } from "@/appframe/loader";
import metaApp from "../../../utility-apps/ai-meta-tag-generator/App.tsx?raw";
import readabilityApp from "../../../utility-apps/readability-tone-analyzer/App.tsx?raw";
import tableApp from "../../../utility-apps/ai-html-table-from-image/App.tsx?raw";
import svgApp from "../../../utility-apps/ai-svg-creator/App.tsx?raw";
import translatorApp from "../../../utility-apps/multi-purpose-ai-translator/App.tsx?raw";
import pluginApp from "../../../utility-apps/ai-plugin-creator/App.tsx?raw";
import llmsApp from "../../../utility-apps/llms-txt-generator/App.tsx?raw";

const transform = ((Babel as { transform?: Transform }).transform ??
  (Babel as unknown as { default: { transform: Transform } }).default.transform) as Transform;

const esm = (ns: object) => ({
  ...ns,
  __esModule: true,
  default: (ns as { default?: unknown }).default ?? ns,
});

/** A stand-in for the `oxide` SDK, the way a fake provider would answer. */
function fakeSdk(over: Record<string, unknown> = {}) {
  const sdk = {
    ai: {
      text: vi.fn(),
      json: vi.fn(),
      info: vi.fn(async () => ({ textModel: "lmstudio/fake-model" })),
    },
    web: { fetch: vi.fn(), crawl: vi.fn() },
    files: {
      pick: vi.fn(async () => []),
      save: vi.fn(async () => true),
      zip: vi.fn(async () => new Blob(["zip"])),
      readText: vi.fn(async () => "file text"),
      toBase64: vi.fn(async () => "QkFTRTY0"),
    },
    storage: { get: vi.fn(async () => undefined), set: vi.fn(async () => undefined) },
    app: { copy: vi.fn(async () => undefined), id: "x", name: "x", theme: "light" },
    ...over,
  };
  return sdk;
}

function mount(source: string, sdk: ReturnType<typeof fakeSdk>) {
  const exports = loadApp([{ path: "App.tsx", content: source }], "App.tsx", {
    transform,
    modules: {
      react: esm(React),
      "lucide-react": esm(Lucide),
      "react-markdown": esm(ReactMarkdown),
      "remark-gfm": esm(RemarkGfm),
      jszip: esm(JSZipNs),
      oxide: esm(sdk),
    },
  });
  const App = appComponent(exports) as React.ComponentType;
  return render(<App />);
}

describe("the built-in Utility Apps", () => {
  it("Meta Tag Generator: fetches the page's HTML and shows three suggestions", async () => {
    const sdk = fakeSdk();
    sdk.web.fetch.mockResolvedValue({ url: "https://ex.com/", content: "<h1>Shoes</h1>" });
    sdk.ai.json.mockResolvedValue({
      suggestions: [1, 2, 3].map((n) => ({
        title: `Title ${n} for great running shoes online`,
        description: `Description ${n}`,
        reasoning: `Because ${n}`,
      })),
    });
    mount(metaApp, sdk);
    fireEvent.change(screen.getByPlaceholderText("https://example.com/page"), {
      target: { value: "https://ex.com/" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Generate meta tags/ }));
    expect(await screen.findByText("Title 3 for great running shoes online")).toBeInTheDocument();
    expect(sdk.web.fetch).toHaveBeenCalledWith("https://ex.com/", { format: "html" });
    const req = sdk.ai.json.mock.calls[0]?.[0] as { prompt: string; schema: object };
    expect(req.prompt).toContain("<h1>Shoes</h1>");
    expect(req.schema).toBeTruthy();
    fireEvent.click(screen.getAllByTitle("Copy meta title")[0] as HTMLElement);
    await waitFor(() =>
      expect(sdk.app.copy).toHaveBeenCalledWith("Title 1 for great running shoes online"),
    );
  });

  it("Readability Analyzer: analyzes pasted text and shows score, tone and advice", async () => {
    const sdk = fakeSdk();
    sdk.ai.json.mockResolvedValue({
      readabilityScore: 72.4,
      gradeLevel: "8th Grade",
      tone: "Conversational",
      targetAudience: "Runners",
      strengths: ["Short sentences"],
      improvements: [{ issue: "Jargon", suggestion: "Explain terms" }],
      detailedAnalysis: "## Overall\nClear **and** friendly.",
    });
    mount(readabilityApp, sdk);
    fireEvent.click(screen.getByRole("button", { name: "Direct text" }));
    fireEvent.change(screen.getByPlaceholderText("Paste your content here…"), {
      target: { value: "Some text to analyze." },
    });
    fireEvent.click(screen.getByRole("button", { name: /Analyze content/ }));
    expect(await screen.findByText("72/100")).toBeInTheDocument();
    expect(screen.getByText("Conversational")).toBeInTheDocument();
    expect(screen.getByText("Explain terms")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Overall" })).toBeInTheDocument();
    expect(sdk.web.fetch).not.toHaveBeenCalled();
  });

  it("HTML Table from Image: sends the image and shows the cleaned table", async () => {
    const sdk = fakeSdk();
    const image = new File(["png"], "t.png", { type: "image/png" });
    sdk.files.pick.mockResolvedValue([image] as never);
    sdk.ai.text.mockResolvedValue({
      text: '```html\n<table><tr><th>A</th><td onclick="x()">1</td></tr></table>\n```',
      model: "m",
    });
    URL.createObjectURL ??= () => "blob:x";
    URL.revokeObjectURL ??= () => undefined;
    mount(tableApp, sdk);
    await act(async () => {
      fireEvent.click(screen.getByText(/Drop an image of a table here/));
    });
    const generate = screen.getByRole("button", { name: /Generate HTML table/ });
    await waitFor(() => expect(generate).toBeEnabled());
    fireEvent.click(generate);
    expect(await screen.findByText("A")).toBeInTheDocument();
    const req = sdk.ai.text.mock.calls[0]?.[0] as { images: File[]; system: string };
    expect(req.images[0]).toBe(image);
    fireEvent.click(screen.getByRole("button", { name: "Code" }));
    const code = screen.getByText(/<table>/).textContent ?? "";
    expect(code).not.toContain("onclick");
    fireEvent.click(screen.getByRole("button", { name: /Download/ }));
    expect(sdk.files.save).toHaveBeenCalledWith("table.html", code, { mediaType: "text/html" });
  });

  it("SVG Creator: makes an SVG, then refines it with the current code", async () => {
    const sdk = fakeSdk();
    sdk.ai.text
      .mockResolvedValueOnce({
        text: 'Here: <svg viewBox="0 0 1 1"><circle r="1"/></svg>',
        model: "m",
      })
      .mockResolvedValueOnce({ text: '<svg viewBox="0 0 2 2"><rect/></svg>', model: "m" });
    mount(svgApp, sdk);
    fireEvent.change(screen.getByPlaceholderText(/minimal rocket icon/), {
      target: { value: "a circle" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Generate SVG/ }));
    const img = await screen.findByAltText("The generated SVG");
    expect(img.getAttribute("src")).toContain(encodeURIComponent("<circle"));
    fireEvent.change(screen.getByPlaceholderText(/thicker/), { target: { value: "a square" } });
    fireEvent.click(screen.getByRole("button", { name: /Update SVG/ }));
    await waitFor(() =>
      expect(screen.getByAltText("The generated SVG").getAttribute("src")).toContain(
        encodeURIComponent("<rect"),
      ),
    );
    const second = sdk.ai.text.mock.calls[1]?.[0] as { prompt: string };
    expect(second.prompt).toContain("<circle");
    expect(second.prompt).toContain("a square");
  });

  it("Translator: streams a translation of pasted text in the chosen language and style", async () => {
    const sdk = fakeSdk();
    sdk.ai.text.mockImplementation(async (o: { onText?: (d: string, t: string) => void }) => {
      o.onText?.("Hej", "Hej");
      return { text: "Hej verden\nAnden linje", model: "lmstudio/fake-model" };
    });
    const { container } = mount(translatorApp, sdk);
    fireEvent.change(screen.getByLabelText("Translate to"), { target: { value: "Danish" } });
    fireEvent.change(screen.getByLabelText("Style"), { target: { value: "Casual / Colloquial" } });
    fireEvent.change(screen.getByPlaceholderText("Paste your text or HTML here..."), {
      target: { value: "Hello world" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Translate" }));
    expect(await screen.findByText(/Hej verden/)).toBeInTheDocument();
    expect(container.querySelector("br")).toBeTruthy();
    expect(screen.getByText("lmstudio/fake-model")).toBeInTheDocument();
    const req = sdk.ai.text.mock.calls[0]?.[0] as { prompt: string; images?: unknown };
    expect(req.prompt).toContain("Translate the following content to Danish");
    expect(req.prompt).toContain("casual, conversational language");
    expect(req.prompt).toContain("Hello world");
    expect(req.images).toBeUndefined();
    fireEvent.click(screen.getByRole("button", { name: /Copy/ }));
    await waitFor(() => expect(sdk.app.copy).toHaveBeenCalledWith("Hej verden\nAnden linje"));
  });

  it("Translator: saves a new style and translates a PDF with it, then saves Rich Text", async () => {
    const sdk = fakeSdk();
    const pdf = new File(["%PDF"], "flyer.pdf", { type: "application/pdf" });
    sdk.files.pick.mockResolvedValue([pdf] as never);
    sdk.ai.text.mockResolvedValue({ text: "**Overskrift**\n\n*Tekst:* Hej", model: "m" });
    mount(translatorApp, sdk);
    fireEvent.click(screen.getByTitle("Manage styles"));
    fireEvent.click(screen.getByRole("button", { name: /New style/ }));
    fireEvent.change(screen.getByPlaceholderText("e.g. Marketing copy"), {
      target: { value: "Marketing" },
    });
    fireEvent.change(screen.getByPlaceholderText(/How the model should translate/), {
      target: { value: "Make it punchy." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save style" }));
    await waitFor(() => expect(sdk.storage.set).toHaveBeenCalled());
    const saved = sdk.storage.set.mock.calls[0] as unknown as [string, Array<{ name: string }>];
    expect(saved[1].map((i) => i.name)).toContain("Marketing");
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect((screen.getByLabelText("Style") as HTMLSelectElement).value).toBe("Marketing");
    fireEvent.click(screen.getByRole("button", { name: /Document \/ Image/ }));
    await act(async () => {
      fireEvent.click(screen.getByText(/Drag & drop a file here/));
    });
    expect(await screen.findByText("flyer.pdf")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Translate" }));
    expect(await screen.findByText("Overskrift")).toBeInTheDocument();
    const req = sdk.ai.text.mock.calls[0]?.[0] as {
      prompt: string;
      documents: Array<{ mediaType: string; data: string; name: string }>;
    };
    expect(req.prompt).toContain("Make it punchy.");
    expect(req.prompt).toContain("OUTPUT FORMAT");
    expect(req.documents[0]).toEqual({
      mediaType: "application/pdf",
      data: "QkFTRTY0",
      name: "flyer.pdf",
    });
    fireEvent.click(screen.getByRole("button", { name: /Save as Rich Text file/ }));
    const [name, rtf] = sdk.files.save.mock.calls[0] as unknown as [string, string];
    expect(name).toMatch(/^English-translation-\d{4}-\d{2}-\d{2}\.rtf$/);
    expect(rtf).toContain("\\rtf1");
    expect(rtf).toContain("\\b Overskrift\\b0");
  });

  it("Plugin Creator: generates a WordPress plugin, shows its files and saves a ZIP", async () => {
    const sdk = fakeSdk();
    sdk.ai.json.mockResolvedValue({
      files: [
        { path: "awesome-seo/awesome-seo.php", content: "<?php // main" },
        { path: "../evil/readme.txt", content: "notes" },
      ],
    });
    mount(pluginApp, sdk);
    fireEvent.change(screen.getByPlaceholderText("e.g. Awesome SEO"), {
      target: { value: "Awesome SEO" },
    });
    fireEvent.change(screen.getByPlaceholderText(/Describe what the extension should do/), {
      target: { value: "Add meta tags" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Generate Extension/ }));
    expect(await screen.findByText("awesome-seo.php")).toBeInTheDocument();
    expect(screen.getByText("<?php // main")).toBeInTheDocument();
    expect(screen.getByText("evil")).toBeInTheDocument();
    const req = sdk.ai.json.mock.calls[0]?.[0] as { prompt: string; schema: object };
    expect(req.prompt).toContain("expert CMS developer for WordPress");
    expect(req.prompt).toContain("Create a new installable extension");
    expect(req.prompt).toContain("Add meta tags");
    expect(req.schema).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Save to ZIP file/ }));
    await waitFor(() => expect(sdk.files.save).toHaveBeenCalled());
    const [name, blob] = sdk.files.save.mock.calls[0] as unknown as [string, Blob];
    expect(name).toBe("awesome-seo-v1.0.0.zip");
    const zip = await JSZip.loadAsync(blob);
    expect(
      Object.keys(zip.files)
        .filter((p) => !zip.files[p]?.dir)
        .sort(),
    ).toEqual(["awesome-seo/awesome-seo.php", "evil/readme.txt"]);
  });

  it("Plugin Creator: modifies an uploaded ZIP and keeps the files the model left alone", async () => {
    const sdk = fakeSdk();
    const old = new JSZip();
    old.file("old/old.php", "<?php // old");
    old.file("old/logo.png", new Uint8Array([137, 80, 78, 71]));
    const bytes = await old.generateAsync({ type: "arraybuffer" });
    sdk.files.pick.mockResolvedValue([new File([bytes], "old.zip")] as never);
    sdk.ai.json.mockResolvedValue({ files: [{ path: "old/old.php", content: "<?php // new" }] });
    mount(pluginApp, sdk);
    await act(async () => {
      fireEvent.click(screen.getByText("Drag & drop a ZIP file here"));
    });
    expect(await screen.findByText("old.zip")).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText("e.g. Awesome SEO"), { target: { value: "Old" } });
    fireEvent.change(screen.getByPlaceholderText(/Describe what the extension should do/), {
      target: { value: "Rename things" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Modify Extension/ }));
    expect(await screen.findByText("logo.png")).toBeInTheDocument();
    const req = sdk.ai.json.mock.calls[0]?.[0] as { prompt: string };
    expect(req.prompt).toContain("Modify the provided extension");
    expect(req.prompt).toContain("--- FILE: old/old.php ---\n<?php // old");
    expect(req.prompt).not.toContain("logo.png");
    fireEvent.click(screen.getByRole("button", { name: /Save to ZIP file/ }));
    await waitFor(() => expect(sdk.files.save).toHaveBeenCalled());
    const blob = (sdk.files.save.mock.calls[0] as unknown as [string, Blob])[1];
    const zip = await JSZip.loadAsync(blob);
    expect(await zip.file("old/old.php")?.async("string")).toBe("<?php // new");
    expect(Array.from((await zip.file("old/logo.png")?.async("uint8array")) ?? [])).toEqual([
      137, 80, 78, 71,
    ]);
  });

  it("llms.txt Generator: crawls, lets the model triage and bundles both files", async () => {
    const sdk = fakeSdk();
    sdk.web.crawl.mockResolvedValue({
      pages: [
        { url: "https://ex.com/", title: "Home", content: "# Welcome", depth: 0 },
        { url: "https://ex.com/docs", title: "Docs", content: "Docs body", depth: 1 },
        { url: "https://ex.com/login", title: "Login", content: "Sign in", depth: 1 },
        { url: "https://ex.com/logo.png", title: null, content: "x", depth: 1 },
      ],
      skipped: [{ url: "https://ex.com/private", reason: "robots.txt disallows it" }],
      robots: true,
    });
    sdk.ai.json.mockResolvedValue({
      highValue: [
        {
          language: "en",
          pages: [
            { url: "https://ex.com/", title: "x" },
            { url: "https://ex.com/docs", title: "Docs" },
            { url: "https://ex.com/made-up", title: "?" },
          ],
        },
      ],
      lowValue: [],
    });
    mount(llmsApp, sdk);
    const urls = screen.getByLabelText("Website URLs (one per line)");
    fireEvent.change(urls, { target: { value: "not a url" } });
    fireEvent.click(screen.getByRole("button", { name: "Generate Files" }));
    expect(await screen.findByText(/isn't a valid URL/)).toBeInTheDocument();
    expect(sdk.web.crawl).not.toHaveBeenCalled();
    fireEvent.change(urls, { target: { value: "https://ex.com" } });
    fireEvent.click(screen.getByLabelText(/Markdown Ready/));
    fireEvent.click(screen.getByRole("button", { name: "Generate Files" }));
    expect(await screen.findByText("Analysis Complete!")).toBeInTheDocument();
    expect(sdk.web.crawl).toHaveBeenCalledWith(
      "https://ex.com/",
      expect.objectContaining({ maxPages: 50, maxDepth: 5 }),
    );
    const req = sdk.ai.json.mock.calls[0]?.[0] as { prompt: string };
    expect(req.prompt).toContain('"url":"https://ex.com/docs"');
    expect(req.prompt).not.toContain("logo.png");
    const entries =
      (
        sdk.files.zip.mock.calls as unknown as Array<[Array<{ name: string; data: string }>]>
      )[0]?.[0] ?? [];
    expect(entries.map((e) => e.name)).toEqual(["llms.txt", "llms-full.txt"]);
    const llms = entries[0]?.data ?? "";
    expect(llms).toContain("# ex.com\n\n## Guidance");
    expect(llms).toContain(
      "## High Value Pages (en)\n- [Home](https://ex.com/)\n- [Docs](https://ex.com/docs)",
    );
    expect(llms).toContain("## Low Value Pages\n- [Login](https://ex.com/login)");
    expect(llms).not.toContain("made-up");
    const full = entries[1]?.data ?? "";
    expect(full).toContain("# Docs\n\nSource: https://ex.com/docs\n\nDocs body");
    expect(full).not.toContain("Sign in");
    expect(screen.getByLabelText("llms.txt").textContent).toContain("# ex.com");
    expect(screen.getByText(/1 skipped/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Download .zip Bundle/ }));
    expect(sdk.files.save).toHaveBeenCalledWith("llms-pack.zip", expect.any(Blob), {
      mediaType: "application/zip",
    });
  });
});
