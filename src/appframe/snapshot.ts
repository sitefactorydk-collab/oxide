/**
 * A picture of the app in its frame, for the builder's `app_screenshot` (Phase 21).
 *
 * The frame has no access to the screen, so the page draws itself: a copy of the document
 * goes into an SVG `<foreignObject>` with the page's styles (Tailwind's generated `<style>`
 * and the app's own), the SVG is drawn on a canvas, and the canvas becomes a PNG. Web fonts
 * and stylesheets the page links to don't come along, so text may use a fallback font.
 */

/** Longest side of the picture, in CSS pixels. */
const MAX_SIDE = 1600;

/** The CSS text of the page's stylesheets that can be read (inline ones always can). */
function styles(doc: Document): string {
  const out: string[] = [];
  for (const sheet of Array.from(doc.styleSheets)) {
    try {
      out.push(
        Array.from(sheet.cssRules)
          .map((r) => r.cssText)
          .join("\n"),
      );
    } catch {
      // A linked sheet from another origin: its rules can't be read.
    }
  }
  // The SVG is XML: `&` and `<` in CSS text must be escaped.
  return out.join("\n").replace(/&/g, "&amp;").replace(/</g, "&lt;");
}

/** Copies what a clone loses: form values, canvas pixels and scroll positions. */
function carryState(from: Element, to: Element) {
  const src = from.querySelectorAll("input, textarea, select, canvas");
  const dst = to.querySelectorAll("input, textarea, select, canvas");
  src.forEach((el, i) => {
    const copy = dst[i];
    if (!copy) return;
    if (el instanceof HTMLTextAreaElement) {
      copy.textContent = el.value;
    } else if (el instanceof HTMLInputElement) {
      if (el.type === "checkbox" || el.type === "radio") {
        if (el.checked) copy.setAttribute("checked", "");
        else copy.removeAttribute("checked");
      } else if (el.type !== "file") {
        copy.setAttribute("value", el.value);
      }
    } else if (el instanceof HTMLSelectElement) {
      Array.from((copy as HTMLSelectElement).options).forEach((o, j) => {
        if (j === el.selectedIndex) o.setAttribute("selected", "");
        else o.removeAttribute("selected");
      });
    } else if (el instanceof HTMLCanvasElement) {
      try {
        const img = document.createElement("img");
        img.src = el.toDataURL();
        img.width = el.width;
        img.height = el.height;
        img.setAttribute("style", el.getAttribute("style") ?? "");
        img.className = el.className;
        copy.replaceWith(img);
      } catch {
        // A tainted canvas stays blank.
      }
    }
  });
}

/** A PNG `data:` URL of what the frame shows now. */
export async function snapshot(doc: Document = document): Promise<string> {
  const root = doc.documentElement;
  const width = Math.max(1, Math.min(root.clientWidth || window.innerWidth, MAX_SIDE));
  const height = Math.max(1, Math.min(window.innerHeight || root.clientHeight, MAX_SIDE));
  const body = doc.body.cloneNode(true) as HTMLElement;
  carryState(doc.body, body);
  body.querySelectorAll("script").forEach((s) => s.remove());
  const dark = root.classList.contains("dark");
  const html = `<html xmlns="http://www.w3.org/1999/xhtml" class="${dark ? "dark" : ""}" style="color-scheme:${dark ? "dark" : "light"}"><head><style>${styles(doc)}</style></head>${new XMLSerializer().serializeToString(body)}</html>`;
  const offset = -(window.scrollY || 0);
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">` +
    `<foreignObject x="0" y="${offset}" width="${width}" height="${Math.max(height, root.scrollHeight)}">${html}</foreignObject></svg>`;
  const img = new Image();
  img.width = width;
  img.height = height;
  const loaded = new Promise<void>((resolve, reject) => {
    img.onload = () => resolve();
    img.onerror = () => reject(new Error("the page couldn't be drawn"));
  });
  img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  await loaded;
  const scale = Math.min(window.devicePixelRatio || 1, 2);
  const canvas = doc.createElement("canvas");
  canvas.width = Math.round(width * scale);
  canvas.height = Math.round(height * scale);
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no canvas");
  ctx.fillStyle = getComputedStyle(doc.body).backgroundColor || (dark ? "#1f1e1d" : "#faf9f7");
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.scale(scale, scale);
  ctx.drawImage(img, 0, 0, width, height);
  return canvas.toDataURL("image/png");
}
