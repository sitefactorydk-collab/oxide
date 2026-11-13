// Runs in pages Oxide works in (injected on demand, in the extension's isolated world).
//
// The service worker calls `globalThis.__oxideRun(op, args)` through
// `chrome.scripting.executeScript`. Every op answers `{ ok: true, ... }` or `{ error }`.
// Pages can't see or call any of this (isolated world); the banner and the site prompt live in
// closed shadow roots and only react to real (trusted) clicks.

(() => {
  type Answer = Record<string, unknown>;
  type Op = (args: Record<string, unknown>) => Answer | Promise<Answer>;
  const g = globalThis as unknown as {
    __oxideRun?: (op: string, args: Record<string, unknown>) => Promise<Answer>;
  };
  if (g.__oxideRun) return;

  const ATTR = "data-oxide-ref";
  const INTERACTIVE =
    "a[href],button,input:not([type=hidden]),select,textarea,summary," +
    "[role=button],[role=link],[role=checkbox],[role=radio],[role=tab],[role=menuitem]," +
    "[role=option],[role=switch],[role=combobox],[role=textbox],[role=searchbox]," +
    '[contenteditable=""],[contenteditable=true],[onclick]';
  const SKIP = new Set([
    "SCRIPT",
    "STYLE",
    "NOSCRIPT",
    "TEMPLATE",
    "SVG",
    "CANVAS",
    "IFRAME",
    "HEAD",
    "META",
    "LINK",
    "OBJECT",
    "EMBED",
  ]);
  const BLOCK = new Set([
    "P",
    "DIV",
    "SECTION",
    "ARTICLE",
    "MAIN",
    "HEADER",
    "FOOTER",
    "NAV",
    "ASIDE",
    "UL",
    "OL",
    "LI",
    "TABLE",
    "TR",
    "BR",
    "HR",
    "FORM",
    "FIELDSET",
    "DL",
    "DT",
    "DD",
    "BLOCKQUOTE",
    "PRE",
    "FIGURE",
    "FIGCAPTION",
    "ADDRESS",
    "DETAILS",
    "TBODY",
    "THEAD",
    "CAPTION",
    "LABEL",
  ]);
  /** Elements numbered by the last read (also inside open shadow roots). */
  let refs = new Map<number, WeakRef<Element>>();
  /** What each numbered element looked like ("button "Search""), for find. */
  let labels = new Map<number, string>();

  const clean = (s: string | null | undefined) => (s || "").replace(/\s+/g, " ").trim();
  const cut = (s: string, n: number) => (s.length > n ? s.slice(0, n - 1) + "…" : s);

  const visible = (el: Element): boolean => {
    const e = el as Element & {
      checkVisibility?: (o: { checkOpacity: boolean; checkVisibilityCSS: boolean }) => boolean;
    };
    // `display: contents` has no box of its own, so checkVisibility() calls it hidden even
    // though its children show (Google wraps all its results in one). Let the children decide.
    const s = getComputedStyle(el);
    if (s.display === "contents") return s.visibility !== "hidden";
    if (typeof e.checkVisibility === "function") {
      return e.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true });
    }
    return s.display !== "none" && s.visibility !== "hidden";
  };

  const inputType = (el: Element) =>
    el instanceof HTMLInputElement ? (el.getAttribute("type") || "text").toLowerCase() : "";

  const roleOf = (el: Element): string => {
    const r = el.getAttribute("role");
    if (r) return r;
    const t = el.tagName;
    if (t === "A") return "link";
    if (t === "BUTTON" || t === "SUMMARY") return "button";
    if (t === "SELECT") return "select";
    if (t === "TEXTAREA") return "textbox";
    if (t === "INPUT") {
      const type = inputType(el);
      if (["button", "submit", "reset", "image"].includes(type)) return "button";
      if (type === "checkbox" || type === "radio") return type;
      if (type === "search") return "searchbox";
      return type === "text" ? "textbox" : `textbox(${type})`;
    }
    if ((el as HTMLElement).isContentEditable) return "textbox";
    return "clickable";
  };

  /** A label's own words (without the text of the controls inside it). */
  const labelText = (label: Element): string => {
    const copy = label.cloneNode(true) as Element;
    copy.querySelectorAll("select,input,textarea,button").forEach((c) => c.remove());
    return clean(copy.textContent);
  };

  const labelOf = (el: Element): string => {
    const labelled = (el.getAttribute("aria-labelledby") || "")
      .split(/\s+/)
      .map((id) => (id ? document.getElementById(id) : null))
      .filter((x): x is HTMLElement => !!x)
      .map((x) => clean(x.innerText || x.textContent))
      .join(" ");
    const own = (el as HTMLInputElement).labels;
    const fromLabels = own ? [...own].map(labelText).filter(Boolean).join(" ") : "";
    const parts: (string | null | undefined)[] = [
      el.getAttribute("aria-label"),
      labelled,
      fromLabels,
      el.getAttribute("alt"),
      el.getAttribute("title"),
      el.getAttribute("placeholder"),
    ];
    if (!["INPUT", "SELECT", "TEXTAREA"].includes(el.tagName)) {
      parts.push((el as HTMLElement).innerText);
    }
    if (el instanceof HTMLInputElement && ["button", "submit", "reset"].includes(inputType(el))) {
      parts.push(el.value);
    }
    parts.push(el.getAttribute("name"));
    for (const p of parts) {
      const c = clean(p);
      if (c) return cut(c, 100);
    }
    const img = el.querySelector("img[alt]");
    return img ? cut(clean(img.getAttribute("alt")), 100) : "";
  };

  const stateOf = (el: Element): string => {
    const out: string[] = [];
    if (el instanceof HTMLInputElement) {
      const type = inputType(el);
      if (type === "checkbox" || type === "radio") out.push(el.checked ? "checked" : "not checked");
      else if (!["button", "submit", "reset", "image", "password"].includes(type) && el.value) {
        out.push(`value="${cut(clean(el.value), 80)}"`);
      }
      if (el.disabled) out.push("disabled");
    } else if (el instanceof HTMLTextAreaElement) {
      if (el.value) out.push(`value="${cut(clean(el.value), 80)}"`);
      if (el.disabled) out.push("disabled");
    } else if (el instanceof HTMLSelectElement) {
      const o = el.selectedOptions[0];
      if (o) out.push(`selected="${cut(clean(o.textContent), 60)}"`);
      if (el.disabled) out.push("disabled");
    } else if (el instanceof HTMLButtonElement && el.disabled) {
      out.push("disabled");
    }
    return out.length ? ` (${out.join(", ")})` : "";
  };

  /** The page as text with numbered interactive elements (numbers given afresh). */
  const readPage = () => {
    for (const w of refs.values()) w.deref()?.removeAttribute(ATTR);
    refs = new Map();
    labels = new Map();
    let next = 1;
    const out: string[] = [];
    const nl = () => out.push("\n");
    const walk = (node: Node) => {
      if (node.nodeType === Node.TEXT_NODE) {
        const t = (node.textContent || "").replace(/\s+/g, " ");
        if (t.trim()) out.push(t);
        return;
      }
      if (node.nodeType === Node.DOCUMENT_FRAGMENT_NODE) {
        for (const c of Array.from(node.childNodes)) walk(c);
        return;
      }
      if (node.nodeType !== Node.ELEMENT_NODE) return;
      const el = node as Element;
      if (SKIP.has(el.tagName.toUpperCase())) return;
      if (el.closest("[data-oxide-ui]")) return;
      if (!visible(el)) return;
      if (el.matches(INTERACTIVE)) {
        const n = next++;
        el.setAttribute(ATTR, String(n));
        refs.set(n, new WeakRef(el));
        const desc = `${roleOf(el)} "${labelOf(el)}"${stateOf(el)}`;
        labels.set(n, desc);
        out.push(` [${n}] ${desc} `);
        return;
      }
      const h = /^H([1-6])$/.exec(el.tagName);
      if (h) {
        nl();
        out.push("#".repeat(Number(h[1])) + " ");
        for (const c of Array.from(el.childNodes)) walk(c);
        nl();
        return;
      }
      if (el.tagName === "IMG") {
        const alt = clean(el.getAttribute("alt"));
        if (alt) out.push(` [image: ${cut(alt, 80)}] `);
        return;
      }
      const cell = el.tagName === "TD" || el.tagName === "TH";
      const block = BLOCK.has(el.tagName) || cell;
      if (block) nl();
      if (el.tagName === "LI") out.push("- ");
      if (cell) out.push(" | ");
      if (el.shadowRoot) walk(el.shadowRoot);
      for (const c of Array.from(el.childNodes)) walk(c);
      if (block) nl();
    };
    if (document.body) walk(document.body);
    const lines = out
      .join("")
      .split("\n")
      .map((l) => l.replace(/[ \t]+/g, " ").trim())
      .filter((l, i, all) => l !== "" || (i > 0 && all[i - 1] !== ""));
    return {
      ok: true,
      text: lines
        .join("\n")
        .replace(/\n{3,}/g, "\n\n")
        .trim(),
      refs: next - 1,
      url: location.href,
      title: document.title,
    };
  };

  const byRef = (ref: unknown): Element | string => {
    const n = Number(ref);
    const el =
      refs.get(n)?.deref() ?? document.querySelector(`[${ATTR}="${CSS.escape(String(n))}"]`);
    if (!el || !el.isConnected) {
      return `There is no element [${n}] on the page now. Call read_page again: numbers change when the page changes.`;
    }
    return el;
  };

  /** A visible interactive element by its text or label (exact matches first). */
  const byText = (text: string, fields = false): Element | string => {
    const want = clean(text).toLowerCase();
    if (!want) return "Give the element's text.";
    readPage();
    const all = [...refs.entries()]
      .map(([n, w]) => [n, w.deref()] as const)
      .filter((x): x is readonly [number, Element] => !!x[1])
      .filter(
        ([, el]) => !fields || roleOf(el).startsWith("textbox") || roleOf(el) === "searchbox",
      );
    const exact = all.find(([, el]) => labelOf(el).toLowerCase() === want);
    const part = all.find(([, el]) => labelOf(el).toLowerCase().includes(want));
    const hit = exact ?? part;
    return hit ? hit[1] : `No visible ${fields ? "field" : "element"} matches “${text}”.`;
  };

  const describe = (el: Element) =>
    el === document.body || el === document.documentElement
      ? "the page"
      : `${roleOf(el)} "${labelOf(el)}"`;

  const isSecret = (el: Element) => {
    if (inputType(el) === "password") return true;
    const ac = (el.getAttribute("autocomplete") || "").toLowerCase();
    return /cc-(number|csc|exp)|one-time-code|current-password|new-password/.test(ac);
  };
  const SECRET_NOTE =
    "Oxide doesn't type into password, one-time-code or payment-card fields. Ask the user to fill it in themselves.";

  const center = (el: Element) => {
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2, r };
  };

  const sleep = (ms: number) => new Promise((res) => setTimeout(res, ms));

  /** Mouse events at the element's middle, then a click, like a user's. */
  const clickAt = (el: Element, x: number, y: number) => {
    const opts = {
      bubbles: true,
      cancelable: true,
      composed: true,
      clientX: x,
      clientY: y,
      view: window,
    };
    el.dispatchEvent(new PointerEvent("pointerover", opts));
    el.dispatchEvent(new MouseEvent("mouseover", opts));
    el.dispatchEvent(new PointerEvent("pointerdown", { ...opts, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent("mousedown", { ...opts, button: 0, buttons: 1 }));
    if (el instanceof HTMLElement) el.focus({ preventScroll: true });
    el.dispatchEvent(new PointerEvent("pointerup", { ...opts, button: 0 }));
    el.dispatchEvent(new MouseEvent("mouseup", { ...opts, button: 0 }));
    if (el instanceof HTMLElement) el.click();
    else el.dispatchEvent(new MouseEvent("click", { ...opts, button: 0 }));
  };

  const click: Op = async (a) => {
    if (typeof a.x === "number" && typeof a.y === "number") {
      // Screenshot pixels → CSS pixels of the viewport.
      const x = a.x / devicePixelRatio;
      const y = a.y / devicePixelRatio;
      const hit = document.elementFromPoint(x, y);
      if (!hit) return { error: `Nothing is at (${a.x}, ${a.y}) in the visible part of the page.` };
      clickAt(hit, x, y);
      return { ok: true, did: `Clicked at (${a.x}, ${a.y}) on ${describe(hit)}` };
    }
    const el: Element | string =
      a.ref !== undefined && a.ref !== null ? byRef(a.ref) : byText(String(a.text ?? ""));
    if (typeof el === "string") return { error: el };
    if ((el as HTMLButtonElement).disabled) {
      return { error: `${describe(el)} is disabled; it can't be clicked now.` };
    }
    el.scrollIntoView({ block: "center", inline: "center" });
    await sleep(60);
    const { x, y, r } = center(el);
    if (r.width === 0 && r.height === 0) {
      return { error: `${describe(el)} isn't visible, so it can't be clicked.` };
    }
    const top = document.elementFromPoint(x, y);
    if (
      top &&
      top !== el &&
      !el.contains(top) &&
      !top.contains(el) &&
      !top.closest("[data-oxide-ui]")
    ) {
      return {
        error: `${describe(el)} is covered by ${describe(top)} (a banner or dialog?). Deal with that first (read_page shows it), rather than clicking through.`,
      };
    }
    clickAt(el, x, y);
    return { ok: true, did: `Clicked ${describe(el)}` };
  };

  const setValue = (el: HTMLInputElement | HTMLTextAreaElement, value: string) => {
    const proto =
      el instanceof HTMLTextAreaElement
        ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
    if (setter) setter.call(el, value);
    else el.value = value;
    el.dispatchEvent(new Event("input", { bubbles: true, composed: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  };

  const keyEvent = (target: Element, type: string, key: string, mods: Record<string, boolean>) => {
    const code = key.length === 1 ? (/[a-z]/i.test(key) ? `Key${key.toUpperCase()}` : key) : key;
    return target.dispatchEvent(
      new KeyboardEvent(type, {
        key,
        code,
        bubbles: true,
        cancelable: true,
        composed: true,
        ctrlKey: !!mods.ctrl,
        shiftKey: !!mods.shift,
        altKey: !!mods.alt,
        metaKey: !!mods.meta,
      }),
    );
  };

  /** Presses Enter on an element; submits its form when nothing stops it. */
  const pressEnter = (el: Element) => {
    const go = keyEvent(el, "keydown", "Enter", {});
    keyEvent(el, "keypress", "Enter", {});
    keyEvent(el, "keyup", "Enter", {});
    const form = (el as HTMLInputElement).form;
    if (go && form && !(el instanceof HTMLTextAreaElement)) {
      if (typeof form.requestSubmit === "function") form.requestSubmit();
      else form.submit();
    }
  };

  const typeText: Op = async (a) => {
    const text = String(a.text ?? "");
    let el: Element | string;
    if (a.ref !== undefined && a.ref !== null) el = byRef(a.ref);
    else if (typeof a.field === "string" && a.field) el = byText(a.field, true);
    else
      el =
        document.activeElement && document.activeElement !== document.body
          ? document.activeElement
          : "Say which field: give ref (from read_page) or field.";
    if (typeof el === "string") return { error: el };
    if (isSecret(el)) return { error: SECRET_NOTE };
    el.scrollIntoView({ block: "center" });
    if (el instanceof HTMLElement) el.focus();
    const clear = a.clear !== false;
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
      if (el.disabled || el.readOnly)
        return { error: `${describe(el)} can't be typed in (disabled or read-only).` };
      setValue(el, clear ? text : el.value + text);
    } else if ((el as HTMLElement).isContentEditable) {
      if (clear) document.execCommand("selectAll", false);
      document.execCommand("insertText", false, text);
    } else {
      return { error: `${describe(el)} isn't a text field.` };
    }
    if (a.submit) pressEnter(el);
    return {
      ok: true,
      did: `Typed “${cut(text, 60)}” into ${describe(el)}${a.submit ? " and pressed Enter" : ""}`,
    };
  };

  const pressKey: Op = async (a) => {
    const combo = String(a.key ?? "").trim();
    if (!combo) return { error: "Give a key, e.g. Enter, Escape, Tab, ArrowDown or ctrl+a." };
    const parts = combo
      .split("+")
      .map((p) => p.trim())
      .filter(Boolean);
    const keyName = parts.pop() ?? "";
    const mods: Record<string, boolean> = {};
    for (const m of parts) {
      const l = m.toLowerCase();
      if (l === "ctrl" || l === "control") mods.ctrl = true;
      else if (l === "shift") mods.shift = true;
      else if (l === "alt" || l === "option") mods.alt = true;
      else if (l === "meta" || l === "cmd" || l === "command") mods.meta = true;
    }
    const aliases: Record<string, string> = {
      enter: "Enter",
      return: "Enter",
      esc: "Escape",
      escape: "Escape",
      tab: "Tab",
      space: " ",
      backspace: "Backspace",
      delete: "Delete",
      up: "ArrowUp",
      down: "ArrowDown",
      left: "ArrowLeft",
      right: "ArrowRight",
      arrowup: "ArrowUp",
      arrowdown: "ArrowDown",
      arrowleft: "ArrowLeft",
      arrowright: "ArrowRight",
      pageup: "PageUp",
      pagedown: "PageDown",
      home: "Home",
      end: "End",
    };
    const key = aliases[keyName.toLowerCase()] ?? keyName;
    let target: Element | string =
      a.ref !== undefined && a.ref !== null
        ? byRef(a.ref)
        : document.activeElement || document.body;
    if (typeof target === "string") return { error: target };
    if (target instanceof HTMLElement && target !== document.activeElement) target.focus();
    if (key === "Enter" && !mods.ctrl && !mods.alt && !mods.meta) {
      pressEnter(target);
    } else {
      const go = keyEvent(target, "keydown", key, mods);
      keyEvent(target, "keyup", key, mods);
      // What the browser would do itself (synthetic keys have no default action).
      if (go) {
        if (key === "Tab") {
          const all = [
            ...document.querySelectorAll<HTMLElement>(
              "a[href],button,input:not([type=hidden]),select,textarea,[tabindex]:not([tabindex='-1'])",
            ),
          ].filter((e) => visible(e) && !(e as HTMLButtonElement).disabled);
          const i = all.indexOf(target as HTMLElement);
          const next = all[(i + (mods.shift ? -1 : 1) + all.length) % all.length];
          next?.focus();
          target = next ?? target;
        } else if (key === "PageDown" || key === "PageUp" || key === "Home" || key === "End") {
          const h = innerHeight * 0.9;
          if (key === "PageDown") scrollBy(0, h);
          if (key === "PageUp") scrollBy(0, -h);
          if (key === "Home") scrollTo(0, 0);
          if (key === "End") scrollTo(0, document.documentElement.scrollHeight);
        } else if (key.toLowerCase() === "a" && (mods.ctrl || mods.meta)) {
          if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)
            target.select();
        }
      }
    }
    return {
      ok: true,
      did: `Pressed ${combo} on ${target instanceof Element ? describe(target) : "the page"}`,
    };
  };

  const scroll: Op = async (a) => {
    if (a.ref !== undefined && a.ref !== null) {
      const el = byRef(a.ref);
      if (typeof el === "string") return { error: el };
      el.scrollIntoView({ block: "center" });
      await sleep(80);
      return { ok: true, did: `Scrolled ${describe(el)} into view` };
    }
    const dir = String(a.direction ?? "down").toLowerCase();
    const amount = Math.max(0.1, Math.min(20, Number(a.amount ?? 1) || 1));
    const v = innerHeight * 0.9 * amount;
    const h = innerWidth * 0.9 * amount;
    if (dir === "down") scrollBy(0, v);
    else if (dir === "up") scrollBy(0, -v);
    else if (dir === "right") scrollBy(h, 0);
    else if (dir === "left") scrollBy(-h, 0);
    else if (dir === "top") scrollTo(0, 0);
    else if (dir === "bottom") scrollTo(0, document.documentElement.scrollHeight);
    else return { error: "direction is up, down, left, right, top or bottom." };
    await sleep(80);
    const total = document.documentElement.scrollHeight;
    return {
      ok: true,
      did: `Scrolled ${dir}: now at ${Math.round(scrollY)}–${Math.round(scrollY + innerHeight)} of ${total} pixels`,
    };
  };

  const formInput: Op = async (a) => {
    const el = byRef(a.ref);
    if (typeof el === "string") return { error: el };
    if (isSecret(el)) return { error: SECRET_NOTE };
    const value = a.value;
    if (el instanceof HTMLSelectElement) {
      const want = clean(String(value ?? "")).toLowerCase();
      const opt =
        [...el.options].find(
          (o) => o.value.toLowerCase() === want || clean(o.textContent).toLowerCase() === want,
        ) ?? [...el.options].find((o) => clean(o.textContent).toLowerCase().includes(want));
      if (!opt) {
        const choices = [...el.options]
          .map((o) => `“${clean(o.textContent)}”`)
          .slice(0, 30)
          .join(", ");
        return { error: `${describe(el)} has no option “${value}”. Options: ${choices}` };
      }
      el.value = opt.value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return { ok: true, did: `Chose “${clean(opt.textContent)}” in ${describe(el)}` };
    }
    if (el instanceof HTMLInputElement && (el.type === "checkbox" || el.type === "radio")) {
      const want =
        typeof value === "boolean" ? value : !/^(false|off|no|0|unchecked)$/i.test(String(value));
      if (el.checked !== want) el.click();
      return { ok: true, did: `${want ? "Checked" : "Unchecked"} ${describe(el)}` };
    }
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
      setValue(el, String(value ?? ""));
      return { ok: true, did: `Set ${describe(el)} to “${cut(String(value ?? ""), 60)}”` };
    }
    return { error: `${describe(el)} isn't a form field; use click or type.` };
  };

  const find: Op = (a) => {
    const query = clean(String(a.query ?? "")).toLowerCase();
    if (!query) return { error: "Say what to find." };
    const page = readPage();
    const words = query.split(" ").filter((w) => w.length > 1);
    const scored: [number, number, string][] = [];
    for (const [n, desc] of labels) {
      const d = desc.toLowerCase();
      let score = d.includes(query) ? 10 : 0;
      for (const w of words) if (d.includes(w)) score += 1;
      if (score > 0) scored.push([score, n, desc]);
    }
    scored.sort((x, y) => y[0] - x[0] || x[1] - y[1]);
    const limit = Math.max(1, Math.min(50, Number(a.limit ?? 20) || 20));
    // Text on the page (not elements) that mentions the query, for context.
    const lines = page.text
      .split("\n")
      .filter((l) => l.toLowerCase().includes(query) && !/^\s*\[\d+\]/.test(l))
      .slice(0, 5)
      .map((l) => cut(l, 200));
    return {
      ok: true,
      matches: scored.slice(0, limit).map(([, n, desc]) => `[${n}] ${desc}`),
      text: lines,
      title: page.title,
      url: page.url,
    };
  };

  // ---- Console messages (recorded by console-hook.js in the page's own world) ----------
  const consoleLog: { level: string; text: string; time: number }[] = [];
  window.addEventListener("__oxide_console", (e) => {
    try {
      const d = JSON.parse(String((e as CustomEvent).detail)) as {
        level: string;
        text: string;
        time: number;
      };
      consoleLog.push({
        level: String(d.level),
        text: cut(String(d.text), 2000),
        time: Number(d.time),
      });
      if (consoleLog.length > 500) consoleLog.splice(0, consoleLog.length - 500);
    } catch {
      // Not ours.
    }
  });
  // Anything the page's recorder kept before this script arrived.
  window.dispatchEvent(new Event("__oxide_console_hello"));
  const getConsole: Op = (a) => {
    window.dispatchEvent(new Event("__oxide_console_hello"));
    let entries = consoleLog.slice();
    if (typeof a.pattern === "string" && a.pattern) {
      let re: RegExp;
      try {
        re = new RegExp(a.pattern, "i");
      } catch {
        return { error: `“${a.pattern}” isn't a valid regular expression.` };
      }
      entries = entries.filter((e) => re.test(e.text) || re.test(e.level));
    }
    if (a.only_errors)
      entries = entries.filter((e) => e.level === "error" || e.level === "exception");
    const limit = Math.max(1, Math.min(500, Number(a.limit ?? 100) || 100));
    entries = entries.slice(-limit);
    if (a.clear) consoleLog.length = 0;
    return { ok: true, entries, url: location.href };
  };

  // ---- Oxide's own UI in the page (closed shadow roots) ---------------------------------
  const host = (id: string) => {
    let el = document.getElementById(id);
    if (!el) {
      el = document.createElement("div");
      el.id = id;
      el.setAttribute("data-oxide-ui", "");
      (document.body || document.documentElement).appendChild(el);
    }
    return el;
  };
  const shadows = new WeakMap<Element, ShadowRoot>();
  const shadowOf = (el: Element) => {
    let s = shadows.get(el);
    if (!s) {
      s = el.attachShadow({ mode: "closed" });
      shadows.set(el, s);
    }
    return s;
  };
  const FONT = "font: 13px/1.4 system-ui, -apple-system, 'Segoe UI', sans-serif;";
  const ACCENT = "#c96442";

  const banner: Op = (a) => {
    const old = document.getElementById("oxide-banner");
    if (!a.on) {
      old?.remove();
      return { ok: true };
    }
    if (old) return { ok: true };
    const root = shadowOf(host("oxide-banner"));
    root.innerHTML = `
      <style>
        .frame { position: fixed; inset: 0; pointer-events: none; z-index: 2147483646;
                 box-shadow: inset 0 0 0 3px ${ACCENT}; }
        .bar { position: fixed; left: 50%; bottom: 16px; transform: translateX(-50%);
               z-index: 2147483647; display: flex; gap: 10px; align-items: center;
               background: #2b2a27; color: #f5f1ea; border-radius: 999px; padding: 6px 6px 6px 14px;
               box-shadow: 0 4px 18px rgba(0,0,0,.25); ${FONT} }
        .dot { width: 8px; height: 8px; border-radius: 50%; background: ${ACCENT};
               animation: pulse 1.6s ease-in-out infinite; }
        @keyframes pulse { 50% { opacity: .35; } }
        button { all: unset; cursor: pointer; background: #f5f1ea; color: #2b2a27; font-weight: 600;
                 border-radius: 999px; padding: 4px 12px; ${FONT} }
        button:hover { background: #fff; }
      </style>
      <div class="frame"></div>
      <div class="bar" role="status"><span class="dot"></span><span>Oxide is controlling this tab</span>
        <button type="button">Stop</button></div>`;
    root.querySelector("button")?.addEventListener("click", (e) => {
      if (!e.isTrusted) return;
      document.getElementById("oxide-banner")?.remove();
      void chrome.runtime.sendMessage({ type: "stop" });
    });
    return { ok: true };
  };

  /** Asks the user whether Oxide may work on this site. */
  const ask: Op = (a) =>
    new Promise<Answer>((resolve) => {
      document.getElementById("oxide-ask")?.remove();
      const site = String(a.site ?? location.host);
      const root = shadowOf(host("oxide-ask"));
      root.innerHTML = `
        <style>
          .back { position: fixed; inset: 0; z-index: 2147483647; background: rgba(20,18,15,.45);
                  display: flex; align-items: center; justify-content: center; ${FONT} }
          .card { background: #faf8f4; color: #2b2a27; border-radius: 14px; padding: 20px 22px;
                  width: min(420px, calc(100vw - 32px)); box-shadow: 0 10px 40px rgba(0,0,0,.3); }
          h2 { margin: 0 0 8px; font-size: 16px; }
          p { margin: 0 0 16px; color: #5d5a54; }
          .row { display: flex; gap: 8px; justify-content: flex-end; flex-wrap: wrap; }
          button { all: unset; cursor: pointer; border-radius: 8px; padding: 7px 14px; ${FONT}
                   border: 1px solid #d9d4ca; background: #fff; }
          button.main { background: ${ACCENT}; border-color: ${ACCENT}; color: #fff; font-weight: 600; }
          button:focus-visible { outline: 2px solid ${ACCENT}; outline-offset: 2px; }
        </style>
        <div class="back"><div class="card" role="dialog" aria-modal="true">
          <h2></h2><p></p>
          <div class="row">
            <button type="button" data-a="deny">Don't allow</button>
            <button type="button" data-a="once">Allow for this chat</button>
            <button type="button" class="main" data-a="always">Always allow</button>
          </div></div></div>`;
      const h2 = root.querySelector("h2");
      const p = root.querySelector("p");
      if (h2) h2.textContent = `Let Oxide use ${site}?`;
      if (p) {
        p.textContent =
          `Oxide (the desktop app) wants to ${String(a.what ?? "read and act on this page")} on ${site}. ` +
          "It can see what's on the page and click and type here as you.";
      }
      const timer = setTimeout(() => done("timeout"), 120_000);
      const done = (answer: string) => {
        clearTimeout(timer);
        document.getElementById("oxide-ask")?.remove();
        resolve({ ok: true, answer });
      };
      root.querySelectorAll("button").forEach((b) =>
        b.addEventListener("click", (e) => {
          if (!e.isTrusted) return;
          done((b as HTMLElement).dataset.a ?? "deny");
        }),
      );
    });

  const ops: Record<string, Op> = {
    read: () => readPage(),
    find,
    click,
    type: typeText,
    key: pressKey,
    scroll,
    formInput,
    console: getConsole,
    banner,
    ask,
    info: () => ({ ok: true, url: location.href, title: document.title }),
  };

  g.__oxideRun = async (op, args) => {
    const f = ops[op];
    if (!f) return { error: `Unknown page operation ${op}.` };
    try {
      return await f(args ?? {});
    } catch (e) {
      return { error: e instanceof Error ? e.message : String(e) };
    }
  };
})();
