import { useQuery } from "@tanstack/react-query";
import { Download, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, type ContentBlock } from "@/bindings";
import { ImageThumb } from "@/features/attachments/MessageAttachments";
import { errorMessage, unwrap } from "@/lib/api";
import { isPlayable } from "./media";

const mediaKeys = {
  bytes: (path: string) => ["attachments", "media", path] as const,
};

/** A stored file as a `blob:` URL for a player (released when the player goes). */
function useBlobUrl(path: string, mediaType: string): { url: string | null; error: string | null } {
  const q = useQuery({
    queryKey: mediaKeys.bytes(path),
    queryFn: () => unwrap(commands.attachmentMedia(path)),
    gcTime: 60_000,
  });
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!q.data) return;
    const bin = atob(q.data);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    const made = URL.createObjectURL(new Blob([bytes], { type: mediaType }));
    setUrl(made);
    return () => URL.revokeObjectURL(made);
  }, [q.data, mediaType]);
  return { url, error: q.isError ? errorMessage(q.error) : null };
}

/** Saves a stored file where the user picks. */
export function DownloadButton({ path, name }: { path: string; name: string }) {
  const [state, setState] = useState<string>("idle");
  const save = async () => {
    setState("saving");
    try {
      const where = await unwrap(commands.saveAttachmentFile(path, name));
      setState(where ? "saved" : "idle");
    } catch (e) {
      setState(errorMessage(e));
    }
  };
  return (
    <span className="inline-flex items-center gap-2">
      <button
        type="button"
        onClick={() => void save()}
        disabled={state === "saving"}
        aria-label={`Download ${name}`}
        className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-line bg-raised px-2.5 text-sm text-fg-muted hover:bg-hover hover:text-fg disabled:opacity-60"
      >
        <Download aria-hidden className="size-3.5" />
        Download
      </button>
      {state === "saved" && <span className="text-xs text-fg-subtle">Saved</span>}
      {state !== "idle" && state !== "saving" && state !== "saved" && (
        <span role="alert" className="text-xs text-danger">
          {state}
        </span>
      )}
    </span>
  );
}

/** A player for generated audio or video, with a download button. */
export function MediaPlayer({
  path,
  mediaType,
  name,
}: {
  path: string;
  mediaType: string;
  name: string;
}) {
  const { url, error } = useBlobUrl(path, mediaType);
  const video = mediaType.startsWith("video/");
  return (
    <figure className="flex flex-col items-start gap-2">
      {error ? (
        <span role="alert" className="text-sm text-danger">
          {name} can’t be played: {error}
        </span>
      ) : url ? (
        video ? (
          <video
            controls
            src={url}
            aria-label={name}
            className="max-h-80 max-w-full rounded-lg border border-line bg-sunken"
          />
        ) : (
          <audio controls src={url} aria-label={name} className="w-80 max-w-full" />
        )
      ) : (
        <span className="inline-flex items-center gap-2 text-sm text-fg-subtle">
          <Loader2 aria-hidden className="size-3.5 animate-spin" />
          Loading {video ? "the video" : "the audio"}
        </span>
      )}
      <DownloadButton path={path} name={name} />
    </figure>
  );
}

/** What a generation call made, under its line: images to open and download, players. */
export function GeneratedMedia({ content }: { content: ContentBlock[] }) {
  const items = content.filter((b) => b.type === "image" || isPlayable(b));
  if (items.length === 0) return null;
  let imageIndex = 0;
  return (
    <div className="flex flex-col gap-3 border-t border-line px-3 py-3">
      {items.map((b, i) => {
        if (b.type === "image" && b.source.kind === "path") {
          const n = imageIndex++;
          const ext = b.mediaType.split("/")[1] ?? "png";
          return (
            <div key={i} className="flex flex-col items-start gap-2">
              <ImageThumb path={b.source.path} index={n} />
              <DownloadButton
                path={b.source.path}
                name={`generated-image-${n + 1}.${ext === "jpeg" ? "jpg" : ext}`}
              />
            </div>
          );
        }
        if (isPlayable(b)) {
          return <MediaPlayer key={i} path={b.source.path} mediaType={b.mediaType} name={b.name} />;
        }
        return null;
      })}
    </div>
  );
}
