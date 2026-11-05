import { useQuery } from "@tanstack/react-query";
import { commands } from "@/bindings";
import { unwrap } from "@/lib/api";
import { DEFAULT_LIMITS } from "./limits";

export const attachmentKeys = {
  limits: ["attachments", "limits"] as const,
  preview: (path: string, maxSide: number | null) =>
    ["attachments", "preview", path, maxSide] as const,
};

export function useAttachmentLimits() {
  const q = useQuery({
    queryKey: attachmentKeys.limits,
    queryFn: () => commands.attachmentLimits(),
  });
  return q.data ?? DEFAULT_LIMITS;
}

/** A stored image as a data URL, scaled to fit `maxSide` (full size for `null`). */
export function useAttachmentPreview(path: string, maxSide: number | null, enabled = true) {
  return useQuery({
    queryKey: attachmentKeys.preview(path, maxSide),
    queryFn: () => unwrap(commands.attachmentPreview(path, maxSide)),
    enabled: enabled && path !== "",
    gcTime: 10 * 60_000,
  });
}
