import { describe, expect, it } from "vitest";
import { prepareMarkdown } from "./prepare";

const done = (s: string) => prepareMarkdown(s);
const live = (s: string) => prepareMarkdown(s, { streaming: true });

describe("prepareMarkdown: math and dollars", () => {
  it("keeps prices as text", () => {
    expect(done("It costs $5 and $10.")).toBe("It costs \\$5 and \\$10.");
    expect(done("Between $5 and $ 7")).toBe("Between \\$5 and \\$ 7");
    expect(done("Pay $20 $ now")).toBe("Pay \\$20 \\$ now");
  });

  it("keeps inline math that follows Pandoc's rule", () => {
    expect(done("Let $x^2$ be positive.")).toBe("Let $x^2$ be positive.");
    expect(done("Mixed: $5 and $y$.")).toBe("Mixed: \\$5 and $y$.");
    expect(done("price $5, or $x+1$")).toBe("price \\$5, or $x+1$");
    // Already escaped dollars stay escaped.
    expect(done("a \\$ sign")).toBe("a \\$ sign");
  });

  it("keeps display math and converts LaTeX delimiters", () => {
    expect(done("$$\ne^{i\\pi}+1=0\n$$")).toBe("$$\ne^{i\\pi}+1=0\n$$");
    expect(done("Euler \\(e^{ix}\\) here")).toBe("Euler $e^{ix}$ here");
    expect(done("Then\n\\[\n x = 1\n\\]\nend")).toBe("Then\n$$\nx = 1\n$$\nend");
    // Inside a list item the block stays indented, and so does the text after it.
    expect(done("- item \\[a+b\\] tail")).toBe("- item \n  $$\n  a+b\n  $$\n  tail");
  });

  it("never touches code", () => {
    const src = "`$HOME` and `` a`$`b ``\n```sh\necho $PATH \\(x\\)\n```\n$x$";
    expect(done(src)).toBe(src);
    // Longer fences may contain shorter ones.
    const nested = "````md\n```js\nconst a = $b;\n```\n````\nok $5";
    expect(done(nested)).toBe("````md\n```js\nconst a = $b;\n```\n````\nok \\$5");
    const tilde = "~~~\n$ not math\n~~~";
    expect(done(tilde)).toBe(tilde);
  });

  it("escapes an unclosed $$ in finished text", () => {
    expect(done("Sum:\n\n$$\n\\sum_i")).toBe("Sum:\n\n\\$\\$\n\\sum_i");
  });

  it("normalizes line endings", () => {
    expect(done("a\r\nb\rc")).toBe("a\nb\nc");
  });
});

describe("prepareMarkdown: streaming", () => {
  it("closes an open code fence so the block renders as code at once", () => {
    expect(live("Here:\n```py\nprint(1)")).toBe("Here:\n```py\nprint(1)\n```");
    expect(live("  ~~~~\nx")).toBe("  ~~~~\nx\n  ~~~~");
    // Finished text is left alone (CommonMark runs an open fence to the end anyway).
    expect(done("```py\nprint(1)")).toBe("```py\nprint(1)");
  });

  it("holds back display math until it closes", () => {
    expect(live("Sum:\n\n$$\n\\sum_i")).toBe("Sum:\n\n");
    expect(live("Sum:\n\n$$\n\\sum_i\n$$")).toBe("Sum:\n\n$$\n\\sum_i\n$$");
    expect(live("See \\[ x +")).toBe("See ");
  });

  it("waits for a table row's line to end", () => {
    expect(live("| a | b |\n|---|---|\n| 1 | 2")).toBe("| a | b |\n|---|---|\n");
    expect(live("| a | b |\n|--")).toBe("| a | b |\n");
    expect(live("| a | b |\n|---|---|\n| 1 | 2 |\n")).toBe("| a | b |\n|---|---|\n| 1 | 2 |\n");
  });

  it("holds back an inline code span being written, but not across paragraphs", () => {
    expect(live("Use `foo")).toBe("Use ");
    expect(live("Use `foo`")).toBe("Use `foo`");
    expect(live("A ` tick\n\nnext")).toBe("A ` tick\n\nnext");
  });

  it("doesn't hold back text after closed constructs", () => {
    expect(live("Price $5 and growing")).toBe("Price \\$5 and growing");
    expect(live("```\ncode\n```\nafter")).toBe("```\ncode\n```\nafter");
  });
});
