import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type Attachment, type AttachmentSource } from "@/bindings";
import { formatBytes, maxAnyBytes } from "./limits";
import { useAttachmentLimits } from "./queries";

/** A file in the composer: being prepared, ready to send, or refused. */
export interface PendingAttachment {
  key: string;
  name: string;
  status: "loading" | "ready" | "error";
  attachment?: Attachment;
  error?: string;
}

let seq = 0;

/** Base64 of a browser File (clipboard, file input, HTML5 drop). */
function readBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("couldn't read the file"));
    reader.onload = () => {
      const url = String(reader.result ?? "");
      resolve(url.slice(url.indexOf(",") + 1));
    };
    reader.readAsDataURL(file);
  });
}

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/** A name for a pasted image without one ("image.png" from the clipboard is common). */
function pastedName(file: File): string {
  if (file.name && file.name !== "image.png") return file.name;
  const ext = file.type.split("/")[1] ?? "png";
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `Pasted image ${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}.${pad(d.getMinutes())}.${pad(d.getSeconds())}.${ext}`;
}

/**
 * The composer's attachments. Files are prepared by the backend as soon as they're added
 * (copied, resized, checked), so problems show on the chip before sending.
 */
export function useAttachments() {
  const limits = useAttachmentLimits();
  const [items, setItems] = useState<PendingAttachment[]>([]);
  const itemsRef = useRef(items);
  const alive = useRef(true);
  useEffect(() => {
    itemsRef.current = items;
  }, [items]);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const update = useCallback((key: string, patch: Partial<PendingAttachment>) => {
    if (!alive.current) return;
    setItems((prev) => prev.map((i) => (i.key === key ? { ...i, ...patch } : i)));
  }, []);

  const start = useCallback(
    (entries: Array<{ name: string; source: () => Promise<AttachmentSource> }>) => {
      const room = Math.max(0, limits.maxFiles - itemsRef.current.length);
      const added: PendingAttachment[] = entries.map((e, i) => {
        seq += 1;
        return i < room
          ? { key: `att-${seq}`, name: e.name, status: "loading" }
          : {
              key: `att-${seq}`,
              name: e.name,
              status: "error",
              error: `At most ${limits.maxFiles} files can be sent with one message.`,
            };
      });
      itemsRef.current = [...itemsRef.current, ...added];
      setItems(itemsRef.current);
      added.forEach((item, i) => {
        if (item.status !== "loading") return;
        void (async () => {
          try {
            const source = await entries[i]!.source();
            const res = await commands.prepareAttachment(source);
            if (res.status === "ok") {
              update(item.key, { status: "ready", attachment: res.data, name: res.data.name });
            } else {
              update(item.key, { status: "error", error: res.error.message });
            }
          } catch (e) {
            update(item.key, {
              status: "error",
              error: e instanceof Error ? e.message : `${item.name} couldn't be read.`,
            });
          }
        })();
      });
    },
    [limits.maxFiles, update],
  );

  /** Files on disk (native picker, Tauri drag and drop). */
  const addPaths = useCallback(
    (paths: string[]) =>
      start(
        paths.map((path) => ({
          name: baseName(path),
          source: () => Promise.resolve({ kind: "path", path } as AttachmentSource),
        })),
      ),
    [start],
  );

  /** Browser files (clipboard, file input, HTML5 drop). `pasted` names unnamed images. */
  const addFiles = useCallback(
    (files: File[], pasted = false) => {
      const max = maxAnyBytes(limits);
      start(
        files.map((file) => {
          const name = pasted ? pastedName(file) : file.name || "file";
          return {
            name,
            source: async () => {
              if (file.size > max) {
                throw new Error(
                  `${name} is too large (${formatBytes(file.size)}). The limit is ${formatBytes(max)}.`,
                );
              }
              return { kind: "bytes", name, data: await readBase64(file) } as AttachmentSource;
            },
          };
        }),
      );
    },
    [limits, start],
  );

  const remove = useCallback((key: string) => {
    setItems((prev) => prev.filter((i) => i.key !== key));
  }, []);

  const clear = useCallback(() => setItems([]), []);

  const ready = items.flatMap((i) => (i.status === "ready" && i.attachment ? [i.attachment] : []));
  const busy = items.some((i) => i.status === "loading");
  return { items, ready, busy, addPaths, addFiles, remove, clear, limits };
}
