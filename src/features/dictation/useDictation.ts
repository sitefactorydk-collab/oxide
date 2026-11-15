import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "@/bindings";
import { unwrap } from "@/lib/api";

export type DictationState = "idle" | "recording" | "transcribing";

/** Longest recording before it stops by itself (OpenAI takes up to 25 MB). */
export const MAX_RECORDING_MS = 5 * 60_000;

/** The first container the web view can record into (WebView2 and Chromium: webm/opus). */
export function pickMime(isSupported: (m: string) => boolean): string {
  const candidates = ["audio/webm;codecs=opus", "audio/webm", "audio/ogg;codecs=opus", "audio/mp4"];
  return candidates.find((m) => isSupported(m)) ?? "";
}

async function toBase64(blob: Blob): Promise<string> {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  const step = 0x8000;
  for (let i = 0; i < bytes.length; i += step) {
    binary += String.fromCharCode(...bytes.subarray(i, i + step));
  }
  return btoa(binary);
}

/**
 * Records from the microphone and turns the speech into text (Settings → Voice decides the
 * endpoint). `onText` gets the transcript; `onError` a message for the user.
 */
export function useDictation(onText: (text: string) => void, onError: (message: string) => void) {
  const [state, setState] = useState<DictationState>("idle");
  const recorder = useRef<MediaRecorder | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const handlers = useRef({ onText, onError });
  handlers.current = { onText, onError };

  const stop = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    const r = recorder.current;
    if (r && r.state !== "inactive") r.stop();
  }, []);

  const start = useCallback(async () => {
    if (recorder.current) return;
    const media = typeof navigator === "undefined" ? undefined : navigator.mediaDevices;
    if (!media?.getUserMedia || typeof MediaRecorder === "undefined") {
      handlers.current.onError("This window can’t record sound.");
      return;
    }
    let stream: MediaStream;
    try {
      stream = await media.getUserMedia({ audio: true });
    } catch {
      handlers.current.onError(
        "Oxide couldn’t use the microphone. Check that one is connected and that Oxide may use it.",
      );
      return;
    }
    const mime = pickMime((m) => MediaRecorder.isTypeSupported(m));
    const r = mime ? new MediaRecorder(stream, { mimeType: mime }) : new MediaRecorder(stream);
    const chunks: Blob[] = [];
    r.ondataavailable = (e) => {
      if (e.data.size > 0) chunks.push(e.data);
    };
    r.onstop = () => {
      stream.getTracks().forEach((t) => t.stop());
      recorder.current = null;
      const type = r.mimeType || mime || "audio/webm";
      const blob = new Blob(chunks, { type });
      if (blob.size === 0) {
        setState("idle");
        return;
      }
      setState("transcribing");
      void toBase64(blob)
        .then((audio) => unwrap(commands.transcribeAudio(audio, type)))
        .then((text) => {
          if (text.trim()) handlers.current.onText(text.trim());
        })
        .catch((e: unknown) => handlers.current.onError(e instanceof Error ? e.message : String(e)))
        .finally(() => setState("idle"));
    };
    recorder.current = r;
    r.start();
    setState("recording");
    timer.current = setTimeout(stop, MAX_RECORDING_MS);
  }, [stop]);

  // Leaving the page stops the microphone.
  useEffect(() => stop, [stop]);

  return { state, start, stop };
}
