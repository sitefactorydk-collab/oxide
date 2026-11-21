import { useEffect, useRef, useState } from "react";
import { inTauri } from "@/lib/platform";
import { onNativeFileDrop, overOwnDropZone } from "@/lib/window";

export interface FileDropHandlers {
  /** Real paths (Tauri: the webview intercepts drops and reports paths). */
  onPaths: (paths: string[]) => void;
  /** Browser files (plain browser preview, tests). */
  onFiles: (files: File[]) => void;
}

/**
 * Window-wide file drag-and-drop. Returns whether files are being dragged over the window.
 */
export function useFileDrop(handlers: FileDropHandlers): boolean {
  const [dragging, setDragging] = useState(false);
  const cb = useRef(handlers);
  useEffect(() => {
    cb.current = handlers;
  }, [handlers]);

  useEffect(() => {
    if (inTauri) {
      let alive = true;
      let unlisten: (() => void) | undefined;
      onNativeFileDrop((e) => {
        // Drags over a Utility App's frame are the app's (AppFrame forwards them).
        if (e.type === "enter" || e.type === "over") setDragging(!overOwnDropZone(e.position));
        else if (e.type === "leave") setDragging(false);
        else {
          setDragging(false);
          if (e.paths.length && !overOwnDropZone(e.position)) cb.current.onPaths(e.paths);
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
      const files = Array.from(e.dataTransfer?.files ?? []);
      if (files.length) cb.current.onFiles(files);
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
