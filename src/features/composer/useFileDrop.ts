import { useEffect, useRef, useState } from "react";
import { inTauri } from "@/lib/platform";
import { onNativeFileDrop } from "@/lib/window";

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/**
 * Window-wide file drag-and-drop. In Tauri the webview intercepts drops and reports real
 * paths (what Phase 4 needs to read files in Rust); in a plain browser HTML5 events are used.
 * Calls `onFiles` with file names and returns whether a drag is over the window.
 */
export function useFileDrop(onFiles: (names: string[]) => void): boolean {
  const [dragging, setDragging] = useState(false);
  const cb = useRef(onFiles);
  useEffect(() => {
    cb.current = onFiles;
  }, [onFiles]);

  useEffect(() => {
    if (inTauri) {
      let alive = true;
      let unlisten: (() => void) | undefined;
      onNativeFileDrop((e) => {
        if (e.type === "enter" || e.type === "over") setDragging(true);
        else if (e.type === "leave") setDragging(false);
        else {
          setDragging(false);
          if (e.paths.length) cb.current(e.paths.map(baseName));
        }
      })
        .then((u) => {
          if (alive) unlisten = u;
          else u();
        })
        .catch(() => {});
      return () => {
        alive = false;
        unlisten?.();
      };
    }

    let depth = 0;
    const hasFiles = (e: DragEvent) => Array.from(e.dataTransfer?.types ?? []).includes("Files");
    const onEnter = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth += 1;
      setDragging(true);
    };
    const onOver = (e: DragEvent) => {
      if (hasFiles(e)) e.preventDefault();
    };
    const onLeave = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      depth = Math.max(0, depth - 1);
      if (depth === 0) setDragging(false);
    };
    const onDrop = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth = 0;
      setDragging(false);
      const names = Array.from(e.dataTransfer?.files ?? []).map((f) => f.name);
      if (names.length) cb.current(names);
    };
    window.addEventListener("dragenter", onEnter);
    window.addEventListener("dragover", onOver);
    window.addEventListener("dragleave", onLeave);
    window.addEventListener("drop", onDrop);
    return () => {
      window.removeEventListener("dragenter", onEnter);
      window.removeEventListener("dragover", onOver);
      window.removeEventListener("dragleave", onLeave);
      window.removeEventListener("drop", onDrop);
    };
  }, []);

  return dragging;
}
