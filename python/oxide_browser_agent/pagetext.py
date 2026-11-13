"""A page as text a model can act on: headings, text, and numbered interactive elements.

``PAGE_TEXT_JS`` runs in the page. It numbers the visible links, buttons and fields with a
``data-oxide-ref`` attribute (``[3] button "Search"``), so ``click`` and ``type`` can find
them again. Numbers are given afresh on every read: after a page changes, read it again.
"""

from __future__ import annotations

REF_ATTR = "data-oxide-ref"

PAGE_TEXT_JS = r"""
() => {
  const ATTR = "data-oxide-ref";
  const INTERACTIVE =
    'a[href],button,input:not([type=hidden]),select,textarea,summary,' +
    '[role=button],[role=link],[role=checkbox],[role=radio],[role=tab],[role=menuitem],' +
    '[role=option],[role=switch],[role=combobox],[role=textbox],[role=searchbox],' +
    '[contenteditable=""],[contenteditable=true],[onclick]';
  const SKIP = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "SVG", "CANVAS",
    "IFRAME", "HEAD", "META", "LINK", "OBJECT", "EMBED"]);
  const BLOCK = new Set(["P", "DIV", "SECTION", "ARTICLE", "MAIN", "HEADER", "FOOTER",
    "NAV", "ASIDE", "UL", "OL", "LI", "TABLE", "TR", "BR", "HR", "FORM", "FIELDSET", "DL",
    "DT", "DD", "BLOCKQUOTE", "PRE", "FIGURE", "FIGCAPTION", "ADDRESS", "DETAILS", "TBODY",
    "THEAD", "CAPTION", "LABEL"]);

  for (const el of document.querySelectorAll("[" + ATTR + "]")) el.removeAttribute(ATTR);

  const visible = (el) => {
    // `display: contents` has no box, so checkVisibility() calls it hidden even though its
    // children show (Google wraps all its results in one). Let the children decide.
    const cs = getComputedStyle(el);
    if (cs.display === "contents") return cs.visibility !== "hidden";
    if (typeof el.checkVisibility === "function") {
      if (!el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) return false;
    } else {
      const s = getComputedStyle(el);
      if (s.display === "none" || s.visibility === "hidden") return false;
    }
    return true;
  };
  const clean = (s) => (s || "").replace(/\s+/g, " ").trim();
  const cut = (s, n) => (s.length > n ? s.slice(0, n - 1) + "…" : s);

  const roleOf = (el) => {
    const r = el.getAttribute("role");
    if (r) return r;
    const t = el.tagName;
    if (t === "A") return "link";
    if (t === "BUTTON" || t === "SUMMARY") return "button";
    if (t === "SELECT") return "select";
    if (t === "TEXTAREA") return "textbox";
    if (t === "INPUT") {
      const type = (el.getAttribute("type") || "text").toLowerCase();
      if (["button", "submit", "reset", "image"].includes(type)) return "button";
      if (type === "checkbox" || type === "radio") return type;
      if (type === "search") return "searchbox";
      return type === "text" ? "textbox" : "textbox(" + type + ")";
    }
    if (el.isContentEditable) return "textbox";
    return "clickable";
  };
  const labelOf = (el) => {
    const byId = el.id ? document.querySelector('label[for="' + CSS.escape(el.id) + '"]') : null;
    const parts = [
      el.getAttribute("aria-label"),
      byId ? byId.innerText : null,
      el.getAttribute("alt"),
      el.getAttribute("title"),
      el.getAttribute("placeholder"),
    ];
    if (!["INPUT", "SELECT", "TEXTAREA"].includes(el.tagName)) parts.push(el.innerText);
    if (el.tagName === "INPUT" && ["button", "submit", "reset"].includes((el.type || "").toLowerCase())) {
      parts.push(el.value);
    }
    parts.push(el.getAttribute("name"));
    for (const p of parts) {
      const c = clean(p);
      if (c) return cut(c, 100);
    }
    const img = el.querySelector && el.querySelector("img[alt]");
    return img ? cut(clean(img.getAttribute("alt")), 100) : "";
  };
  const stateOf = (el) => {
    const out = [];
    if (el.tagName === "INPUT") {
      const type = (el.type || "").toLowerCase();
      if (type === "checkbox" || type === "radio") out.push(el.checked ? "checked" : "not checked");
      else if (!["button", "submit", "reset", "image", "password"].includes(type) && el.value) {
        out.push('value="' + cut(clean(el.value), 80) + '"');
      }
    } else if (el.tagName === "TEXTAREA" && el.value) {
      out.push('value="' + cut(clean(el.value), 80) + '"');
    } else if (el.tagName === "SELECT") {
      const o = el.selectedOptions && el.selectedOptions[0];
      if (o) out.push('selected="' + cut(clean(o.textContent), 60) + '"');
    }
    if (el.disabled) out.push("disabled");
    return out.length ? " (" + out.join(", ") + ")" : "";
  };

  let next = 1;
  const out = [];
  const nl = () => out.push("\n");
  const walk = (node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      const t = node.textContent.replace(/\s+/g, " ");
      if (t.trim()) out.push(t);
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE && node.nodeType !== Node.DOCUMENT_FRAGMENT_NODE) return;
    if (node.nodeType === Node.ELEMENT_NODE) {
      const el = node;
      if (SKIP.has(el.tagName.toUpperCase())) return;
      if (!visible(el)) return;
      if (el.matches(INTERACTIVE)) {
        const n = next++;
        el.setAttribute(ATTR, String(n));
        out.push(" [" + n + "] " + roleOf(el) + ' "' + labelOf(el) + '"' + stateOf(el) + " ");
        return;
      }
      const h = /^H([1-6])$/.exec(el.tagName);
      if (h) {
        nl();
        out.push("#".repeat(Number(h[1])) + " ");
        for (const c of el.childNodes) walk(c);
        nl();
        return;
      }
      if (el.tagName === "IMG") {
        const alt = clean(el.getAttribute("alt"));
        if (alt) out.push(" [image: " + cut(alt, 80) + "] ");
        return;
      }
      const block = BLOCK.has(el.tagName) || el.tagName === "TD" || el.tagName === "TH";
      if (block) nl();
      if (el.tagName === "LI") out.push("- ");
      if (el.tagName === "TD" || el.tagName === "TH") out.push(" | ");
      if (el.shadowRoot) walk(el.shadowRoot);
      for (const c of el.childNodes) walk(c);
      if (block) nl();
      return;
    }
    for (const c of node.childNodes) walk(c);
  };
  if (document.body) walk(document.body);
  const lines = out
    .join("")
    .split("\n")
    .map((l) => l.replace(/[ \t]+/g, " ").trim())
    .filter((l, i, all) => l !== "" || (i > 0 && all[i - 1] !== ""));
  return {
    text: lines.join("\n").replace(/\n{3,}/g, "\n\n").trim(),
    refs: next - 1,
    url: location.href,
    title: document.title,
  };
}
"""


# Google's "AI Overview" at the top of a results page: the text of its section, found from its
# heading (Google's markup has no stable names). `null` when there is none.
AI_OVERVIEW_JS = r"""
() => {
  const isHeading = (e) => /^\s*AI Overview\s*$/i.test(e.textContent || "");
  const heads = [...document.querySelectorAll("h1,h2,h3,h4,[role=heading],div,span,strong")]
    .filter((e) => e.childElementCount === 0 && isHeading(e))
    .filter((e) => e.offsetParent !== null || e.getClientRects().length > 0);
  if (heads.length === 0) return null;
  const total = (document.body.innerText || "").length;
  let box = heads[0];
  // Up to the section around the heading: big enough to hold the answer, well short of the
  // whole page.
  while (box.parentElement && (box.innerText || "").length < 1500) {
    const outer = (box.parentElement.innerText || "").length;
    if (outer >= Math.min(8000, total * 0.6) && (box.innerText || "").length >= 40) break;
    box = box.parentElement;
  }
  const text = (box.innerText || "")
    .replace(/^\s*AI Overview\s*/i, "")
    .replace(/^\s*Show (more|less)\s*$/gim, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  return text.length > 40 ? text.slice(0, 6000) : null;
}
"""


def slice_text(text: str, start: int, max_chars: int) -> str:
    """``max_chars`` characters from ``start``, with a note when there is more."""
    total = len(text)
    start = max(0, min(start, total))
    end = min(total, start + max_chars)
    part = text[start:end]
    notes = []
    if start > 0:
        notes.append(f"[Starts at character {start} of {total}.]")
    if end < total:
        notes.append(
            f"[Cut at character {end} of {total}: call read_page with start={end} to read on.]"
        )
    if notes:
        part = part + "\n\n" + " ".join(notes)
    return part
