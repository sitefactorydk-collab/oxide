// Edit blocks (Phase 26) as a compact diff: `edit_file`, `app_edit_file` and the artifact
// tool's `update` send "exact old text → new text" blocks; tool lines and approval cards show
// those instead of the raw arguments.

export interface EditBlockView {
  old: string;
  new: string;
  replaceAll: boolean;
}

export interface FileEdits {
  /** The file's path (the artifact's id for an artifact update). */
  path: string;
  blocks: EditBlockView[];
}

export interface DiffRow {
  kind: "hunk" | "same" | "add" | "remove";
  text: string;
}

const OLD_KEYS = ["old_str", "old_text", "old_string", "old"];
const NEW_KEYS = ["new_str", "new_text", "new_string", "new"];
/** Unchanged lines shown around each change. */
const CONTEXT = 2;

function pick(o: Record<string, unknown>, keys: string[]): unknown {
  for (const k of keys) if (k in o) return o[k];
  return undefined;
}

/** The edit blocks of a call that has them, or null (other tools, or arguments still streaming). */
export function fileEditsOf(call: {
  tool: string;
  server: string | null;
  inputJson: string;
}): FileEdits | null {
  if (call.server !== null) return null;
  if (!["edit_file", "app_edit_file", "artifact"].includes(call.tool)) return null;
  let input: Record<string, unknown>;
  try {
    const v: unknown = JSON.parse(call.inputJson);
    if (!v || typeof v !== "object" || Array.isArray(v)) return null;
    input = v as Record<string, unknown>;
  } catch {
    return null;
  }
  let path = typeof input.path === "string" ? input.path : "";
  if (call.tool === "artifact") {
    if (input.command !== undefined && input.command !== "update") return null;
    path = typeof input.id === "string" ? input.id : "";
  }
  let list: unknown = input.edits;
  if (typeof list === "string") {
    try {
      list = JSON.parse(list);
    } catch {
      list = undefined;
    }
  }
  const items: unknown[] = Array.isArray(list) && list.length > 0 ? list : [input];
  const blocks: EditBlockView[] = [];
  for (const item of items) {
    if (!item || typeof item !== "object") continue;
    const o = item as Record<string, unknown>;
    const old = pick(o, OLD_KEYS);
    if (typeof old !== "string") continue;
    const next = pick(o, NEW_KEYS);
    blocks.push({
      old,
      new: typeof next === "string" ? next : "",
      replaceAll: o.replace_all === true || o.replaceAll === true,
    });
  }
  return blocks.length > 0 ? { path, blocks } : null;
}

const lines = (t: string) => (t === "" ? [] : t.replace(/\r\n/g, "\n").split("\n"));

/** Rows of a compact diff: a header per block, its changed lines, and a little context. */
export function editDiffRows(blocks: EditBlockView[]): DiffRow[] {
  const rows: DiffRow[] = [];
  blocks.forEach((b, i) => {
    const head = blocks.length > 1 ? `Edit ${i + 1} of ${blocks.length}` : "Edit";
    rows.push({ kind: "hunk", text: b.replaceAll ? `${head} · every place` : head });
    const a = lines(b.old);
    const c = lines(b.new);
    let start = 0;
    while (start < a.length && start < c.length && a[start] === c[start]) start++;
    let end = 0;
    while (
      end < a.length - start &&
      end < c.length - start &&
      a[a.length - 1 - end] === c[c.length - 1 - end]
    ) {
      end++;
    }
    for (const l of a.slice(Math.max(0, start - CONTEXT), start))
      rows.push({ kind: "same", text: l });
    for (const l of a.slice(start, a.length - end)) rows.push({ kind: "remove", text: l });
    for (const l of c.slice(start, c.length - end)) rows.push({ kind: "add", text: l });
    for (const l of a.slice(a.length - end, a.length - end + CONTEXT)) {
      rows.push({ kind: "same", text: l });
    }
  });
  return rows;
}

/** “+3 −1”: lines added and removed by the blocks. */
export function editCounts(blocks: EditBlockView[]): { added: number; removed: number } {
  const rows = editDiffRows(blocks);
  return {
    added: rows.filter((r) => r.kind === "add").length,
    removed: rows.filter((r) => r.kind === "remove").length,
  };
}
