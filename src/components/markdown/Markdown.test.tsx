import { fireEvent, render, screen, within } from "@testing-library/react";
import { Tooltip } from "radix-ui";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import { fakeState, resetFakeBackend } from "@/test/fakeBackend";
import { Markdown } from "./Markdown";

// Code blocks have tooltip buttons; the app provides this in AppProviders.
const wrapper = ({ children }: { children: ReactNode }) => (
  <Tooltip.Provider>{children}</Tooltip.Provider>
);

function md(text: string, streaming = false) {
  return render(<Markdown text={text} streaming={streaming} />, { wrapper }).container;
}

describe("Markdown", () => {
  it("renders GFM: headings, lists, task lists, strikethrough and tables", () => {
    const c = md(
      [
        "## Plan",
        "",
        "1. First",
        "2. Second",
        "",
        "- [x] done",
        "- [ ] todo",
        "",
        "~~old~~ **new**",
        "",
        "| Name | Score |",
        "|---|---:|",
        "| Ada | 10 |",
      ].join("\n"),
    );
    expect(screen.getByRole("heading", { level: 2, name: "Plan" })).toBeInTheDocument();
    expect(c.querySelectorAll("ol > li")).toHaveLength(2);
    expect(screen.getAllByRole("checkbox")).toHaveLength(2);
    expect(c.querySelector("del")).toHaveTextContent("old");
    const table = screen.getByRole("table");
    expect(
      within(table)
        .getAllByRole("columnheader")
        .map((h) => h.textContent),
    ).toEqual(["Name", "Score"]);
    expect(within(table).getByRole("cell", { name: "Ada" })).toBeInTheDocument();
    // Wide tables scroll inside their own region.
    expect(screen.getByRole("region", { name: "Table" })).toContainElement(table);
  });

  it("renders inline and display math with KaTeX, and leaves prices alone", () => {
    const c = md("Inline $x^2$ and a price of $5 or $10.\n\n$$\n\\frac{a}{b}\n$$");
    expect(c.querySelectorAll(".katex")).toHaveLength(2);
    expect(c.querySelector(".katex-display")).not.toBeNull();
    expect(c).toHaveTextContent("a price of $5 or $10.");
  });

  it("converts \\( \\) and \\[ \\] delimiters", () => {
    const c = md("Euler: \\(e^{i\\pi}\\)\n\n\\[\nx = 1\n\\]");
    expect(c.querySelectorAll(".katex")).toHaveLength(2);
    expect(c.querySelector(".katex-display")).not.toBeNull();
  });

  it("shows bad TeX as an error instead of throwing", () => {
    const c = md("$$\n\\frac{\n$$");
    expect(c.querySelector(".katex-error")).not.toBeNull();
  });

  it("renders code blocks with a language label, copy and a wrap toggle", () => {
    md("```rust\nfn main() {}\n```\n\nand `inline` code");
    expect(screen.getByText("rust")).toBeInTheDocument();
    const block = screen.getByLabelText("rust code");
    expect(block).toHaveTextContent("fn main() {}");
    const wrap = screen.getByRole("button", { name: "Wrap lines" });
    expect(block).toHaveClass("whitespace-pre");
    fireEvent.click(wrap);
    expect(block).toHaveClass("whitespace-pre-wrap");
    expect(screen.getByRole("button", { name: "Don’t wrap lines" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Copy code" })).toBeInTheDocument();
    expect(screen.getByText("inline").tagName).toBe("CODE");
  });

  it("labels code without a language as text and keeps it verbatim", () => {
    md("```\n  <b>not html</b>\n\ttab\n```");
    expect(screen.getByText("text")).toBeInTheDocument();
    expect(screen.getByLabelText("text code").textContent).toBe("  <b>not html</b>\n\ttab");
  });

  it("opens links outside the app and never renders raw HTML or remote images", () => {
    resetFakeBackend();
    const c = md(
      "[Docs](https://example.com/docs) <script>alert(1)</script> ![chart](https://example.com/c.png)",
    );
    expect(c.querySelector("script")).toBeNull();
    expect(c.querySelector("img")).toBeNull();
    fireEvent.click(screen.getByRole("link", { name: "Docs" }));
    expect(screen.getByRole("link", { name: /chart/ })).toHaveAttribute(
      "href",
      "https://example.com/c.png",
    );
    expect(fakeState().opened).toEqual(["https://example.com/docs"]);
  });

  it("drops javascript: links", () => {
    md("[x](javascript:alert(1))");
    expect(screen.getByText("x").getAttribute("href") ?? "").not.toMatch(/javascript/i);
  });

  describe("while streaming", () => {
    it("renders an unfinished code block as code", () => {
      md("Here you go:\n\n```ts\nconst a = 1;", true);
      expect(screen.getByLabelText("ts code")).toHaveTextContent("const a = 1;");
    });

    it("hides display math and half a table row until they're complete", () => {
      const c = md("Intro\n\n| a | b |\n|---|---|\n| 1 | 2", true);
      expect(screen.getByRole("table")).toBeInTheDocument();
      expect(screen.queryByRole("cell", { name: "1" })).not.toBeInTheDocument();
      const m = md("Sum:\n\n$$\n\\sum_{i=1", true);
      expect(m).not.toHaveTextContent("sum");
      expect(m.querySelector(".katex-error")).toBeNull();
      expect(c).toHaveTextContent("Intro");
    });

    it("keeps the same blocks as the text grows", () => {
      const parts = ["# Ti", "# Title\n\nSome **bo", "# Title\n\nSome **bold** text\n\n- a"];
      const { container, rerender } = render(<Markdown text={parts[0]!} streaming />, {
        wrapper,
      });
      for (const p of parts.slice(1)) rerender(<Markdown text={p} streaming />);
      expect(screen.getByRole("heading", { name: "Title" })).toBeInTheDocument();
      expect(container.querySelector("strong")).toHaveTextContent("bold");
      expect(container.querySelectorAll("li")).toHaveLength(1);
    });
  });
});
