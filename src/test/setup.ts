import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import { resetStreams } from "@/features/chat/streams";
import { createMemoryStorage, isUsableStorage } from "@/lib/storage";

// Every test talks to the in-memory fake backend instead of Tauri IPC.
vi.mock("@/bindings", async () => {
  const { fakeCommands, fakeEvents } = await import("./fakeBackend");
  return { commands: fakeCommands, events: fakeEvents };
});

// Node 25+ ships its own `localStorage` global (undefined without --localstorage-file) that
// shadows jsdom's. Give tests a working in-memory one.
let ls: Storage | undefined;
try {
  ls = globalThis.localStorage;
} catch {
  ls = undefined;
}
if (!isUsableStorage(ls)) {
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    writable: true,
    value: createMemoryStorage(),
  });
}

// jsdom lacks a few browser APIs that Radix and the theme hook use.
if (typeof window.matchMedia !== "function") {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
}

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as unknown as typeof ResizeObserver;

Element.prototype.scrollIntoView ??= function scrollIntoView() {};
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};

afterEach(() => {
  cleanup();
  localStorage.clear();
  resetStreams();
});
