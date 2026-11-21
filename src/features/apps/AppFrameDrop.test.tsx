import { act, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileDropEvent } from "@/lib/window";
import { overOwnDropZone } from "@/lib/window";
import { resetFakeApps } from "@/test/fakeApps";
import { renderApp } from "@/test/renderApp";

// Tauri takes file drops on the window; the test plays the native drop events.
const native = vi.hoisted(() => ({ cb: null as null | ((e: FileDropEvent) => void) }));
vi.mock("@/lib/window", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/window")>()),
  onNativeFileDrop: async (cb: (e: FileDropEvent) => void) => {
    native.cb = cb;
    return () => {
      if (native.cb === cb) native.cb = null;
    };
  },
}));

const play = (e: FileDropEvent) =>
  act(async () => {
    native.cb?.(e);
  });

describe("files dropped on a Utility App", () => {
  beforeEach(resetFakeApps);
  afterEach(() => {
    native.cb = null;
  });

  it("are forwarded to the frame where they land, read by Oxide", async () => {
    renderApp("/apps/ai-meta-tag-generator");
    const iframe = (await screen.findByTitle("AI Meta Tag Generator")) as HTMLIFrameElement;
    vi.spyOn(iframe, "getBoundingClientRect").mockReturnValue({
      left: 100,
      top: 50,
      width: 400,
      height: 300,
      right: 500,
      bottom: 350,
      x: 100,
      y: 50,
      toJSON: () => ({}),
    } as DOMRect);
    const win = iframe.contentWindow;
    if (!win) throw new Error("no frame window");
    const posted = vi.spyOn(win, "postMessage");
    const sent = (type: string) =>
      posted.mock.calls.map(([m]) => m as { type: string }).filter((m) => m.type === type);
    await waitFor(() => expect(native.cb).not.toBeNull());

    await play({ type: "enter", paths: ["C:\\pics\\table.png"], position: { x: 150, y: 80 } });
    expect(sent("drag").at(-1)).toEqual({
      source: "oxide-host",
      type: "drag",
      phase: "over",
      x: 50,
      y: 30,
      names: ["table.png"],
    });
    // Off the frame: the frame hears the drag left.
    await play({ type: "over", position: { x: 10, y: 10 } });
    expect(sent("drag").at(-1)).toEqual({ source: "oxide-host", type: "drag", phase: "leave" });
    // A drop outside the frame isn't the app's.
    await play({ type: "drop", paths: ["C:\\pics\\table.png"], position: { x: 10, y: 10 } });
    expect(sent("drop")).toHaveLength(0);

    await play({ type: "drop", paths: ["C:\\pics\\table.png"], position: { x: 200, y: 100 } });
    await waitFor(() => expect(sent("drop")).toHaveLength(1));
    expect(sent("drop")[0]).toEqual({
      source: "oxide-host",
      type: "drop",
      x: 100,
      y: 50,
      files: [{ name: "table.png", mediaType: "image/png", dataBase64: "cG5n" }],
    });
  });

  it("aren't taken by the chat composer when they land on an app frame", () => {
    const zone = document.createElement("iframe");
    zone.setAttribute("data-native-drop-zone", "");
    const elsewhere = document.createElement("div");
    document.body.append(zone, elsewhere);
    const original = document.elementFromPoint;
    document.elementFromPoint = (x: number) => (x < 100 ? zone : elsewhere);
    try {
      expect(overOwnDropZone({ x: 10, y: 10 })).toBe(true);
      expect(overOwnDropZone({ x: 200, y: 10 })).toBe(false);
      expect(overOwnDropZone(undefined)).toBe(false);
    } finally {
      document.elementFromPoint = original;
      zone.remove();
      elsewhere.remove();
    }
  });
});
