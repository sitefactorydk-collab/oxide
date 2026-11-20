// The builder's file list as a tree.

export interface TreeRow {
  path: string;
  name: string;
  depth: number;
  folder: boolean;
}

/** The files as a tree: folders first, then files, each sorted by name. */
export function fileTree(paths: string[]): TreeRow[] {
  interface Node {
    files: string[];
    dirs: Map<string, Node>;
  }
  const root: Node = { files: [], dirs: new Map() };
  for (const p of paths) {
    const parts = p.split("/");
    let node = root;
    for (const dir of parts.slice(0, -1)) {
      let next = node.dirs.get(dir);
      if (!next) {
        next = { files: [], dirs: new Map() };
        node.dirs.set(dir, next);
      }
      node = next;
    }
    node.files.push(p);
  }
  const rows: TreeRow[] = [];
  const walk = (node: Node, prefix: string, depth: number) => {
    for (const [name, child] of [...node.dirs.entries()].sort(([a], [b]) => a.localeCompare(b))) {
      rows.push({ path: `${prefix}${name}/`, name, depth, folder: true });
      walk(child, `${prefix}${name}/`, depth + 1);
    }
    for (const f of [...node.files].sort((a, b) => a.localeCompare(b))) {
      rows.push({ path: f, name: f.split("/").pop() ?? f, depth, folder: false });
    }
  };
  walk(root, "", 0);
  return rows;
}
