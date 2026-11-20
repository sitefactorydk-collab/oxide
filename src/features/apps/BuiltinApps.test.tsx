import * as Babel from "@babel/standalone";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
    web: { fetch: vi.fn() },
    files: { pick: vi.fn(async () => []), save: vi.fn(async () => true) },
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
});
