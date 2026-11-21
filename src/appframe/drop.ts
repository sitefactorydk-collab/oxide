/**
 * Replays file drags and drops inside the app frame.
 *
 * In Oxide's window Tauri takes every file drop (it hands the window real paths), so the
 * frame's own `dragenter`/`dragover`/`drop` events never fire. The main window forwards
 * what happens over the frame instead, and this module dispatches the same DOM events at
 * the element under the pointer, so an app's ordinary `onDragOver`/`onDrop` code works.
 */

/** The element a drag at frame coordinates `x`/`y` is over, and the last one it was over. */
let current: Element | null = null;

function target(x: number, y: number): Element {
  return document.elementFromPoint(x, y) ?? document.body;
}

function transfer(files: File[]): DataTransfer {
  const dt = new DataTransfer();
  for (const f of files) dt.items.add(f);
  dt.dropEffect = "copy";
  return dt;
}

function fire(el: Element, type: string, dt: DataTransfer, x: number, y: number): void {
  el.dispatchEvent(
    new DragEvent(type, {
      bubbles: true,
      cancelable: true,
      composed: true,
      clientX: x,
      clientY: y,
      dataTransfer: dt,
    }),
  );
}

/** Files are dragged over `x`/`y` (empty stand-ins with the files' names). */
export function dragOver(x: number, y: number, names: string[]): void {
  const dt = transfer(names.map((n) => new File([], n)));
  const el = target(x, y);
  if (el !== current) {
    if (current) fire(current, "dragleave", dt, x, y);
    fire(el, "dragenter", dt, x, y);
    current = el;
  }
  fire(el, "dragover", dt, x, y);
}

/** The drag left the frame (or was cancelled). */
export function dragLeave(): void {
  if (current) fire(current, "dragleave", transfer([]), 0, 0);
  current = null;
}

function bytes(base64: string): Uint8Array<ArrayBuffer> {
  const bin = atob(base64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Files were dropped at `x`/`y`. */
export function drop(
  x: number,
  y: number,
  files: Array<{ name: string; mediaType: string; dataBase64: string }>,
): void {
  const list = files.map((f) => new File([bytes(f.dataBase64)], f.name, { type: f.mediaType }));
  const el = target(x, y);
  current = null;
  fire(el, "drop", transfer(list), x, y);
}
