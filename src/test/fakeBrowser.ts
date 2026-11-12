import type {
  BrowserFrame,
  BrowserStatus,
  CommandError,
  Result,
  commands as RealCommands,
} from "@/bindings";

/**
 * The browser agent in the fake backend. Nothing is installed at first; Microsoft Edge is
 * "installed on this computer". Setup stops at the packages step until a test calls
 * `finishFakeBrowserSetup()` or `failFakeBrowserSetup()`, so progress can be checked mid-way.
 */
interface FakeBrowser {
  status: BrowserStatus;
  setups: Array<{ chromium: boolean }>;
  closed: string[];
  chromiumRemoved: number;
}

let br: FakeBrowser;
let emitStatus: (s: BrowserStatus) => void = () => {};
let emitFrame: (f: BrowserFrame) => void = () => {};

export function setBrowserEmitters(
  status: (s: BrowserStatus) => void,
  frame: (f: BrowserFrame) => void,
): void {
  emitStatus = status;
  emitFrame = frame;
}

function initial(): BrowserStatus {
  return {
    packages: false,
    versions: [],
    chromium: null,
    installed: [
      {
        name: "Microsoft Edge",
        path: "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
      },
    ],
    using: "Microsoft Edge",
    installing: false,
    step: null,
    message: null,
    percent: null,
    error: null,
    log: [],
    openChats: [],
    browsersDir: "C:/Users/me/AppData/Roaming/Oxide/python/browsers",
  };
}

export function resetFakeBrowser(): void {
  br = { status: initial(), setups: [], closed: [], chromiumRemoved: 0 };
}

export function fakeBrowserState(): Readonly<FakeBrowser> {
  return br;
}

function update(patch: Partial<BrowserStatus>): void {
  br.status = { ...br.status, ...patch };
  emitStatus(br.status);
}

/** Pretends the browser is set up (packages installed). */
export function fakeBrowserReady(): void {
  update({
    packages: true,
    versions: [
      { name: "browser-use", version: "1.0.2" },
      { name: "playwright", version: "1.56.0" },
      { name: "mcp", version: "2.3.0" },
    ],
  });
}

export function finishFakeBrowserSetup(chromium = false): void {
  update({
    installing: false,
    step: null,
    message: null,
    percent: null,
    error: null,
    packages: true,
    versions: [{ name: "browser-use", version: "1.0.2" }],
    chromium: chromium ? `${br.status.browsersDir}/chromium-1200/chrome-win/chrome.exe` : null,
  });
}

export function failFakeBrowserSetup(error: string): void {
  update({ installing: false, step: null, message: null, percent: null, error });
}

/** Opens or closes a chat's browser, as the backend reports it. */
export function setFakeBrowserOpen(conversationId: string, open: boolean): void {
  const rest = br.status.openChats.filter((c) => c !== conversationId);
  update({ openChats: open ? [...rest, conversationId] : rest });
}

/** A live screenshot of a running browser call. */
export function emitBrowserFrame(frame: Partial<BrowserFrame> & { toolUseId: string }): void {
  emitFrame({
    conversationId: "c1",
    text: "Browsing: The Little Bakery",
    url: "https://bakery.example/",
    image: "/9j/4AAQ",
    ...frame,
  });
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data });
const fail = (message: string): Promise<Result<never, CommandError>> =>
  Promise.resolve({ status: "error", error: { kind: "invalid", message } });

export const fakeBrowserCommands: Pick<
  typeof RealCommands,
  "browserStatus" | "browserSetup" | "browserClose" | "browserRemoveChromium"
> = {
  browserStatus: () => ok(br.status),
  browserSetup: (chromium) => {
    br.setups.push({ chromium });
    update({
      installing: true,
      step: "packages",
      message: "Installing the browser packages (Browser-Use, Playwright)",
      percent: null,
      error: null,
      log: ["▸ Installing the browser packages (Browser-Use, Playwright)"],
    });
    return ok(null);
  },
  browserClose: (conversationId) => {
    br.closed.push(conversationId);
    setFakeBrowserOpen(conversationId, false);
    return ok(null);
  },
  browserRemoveChromium: () => {
    if (br.status.openChats.length > 0) return fail("A chat's browser is open. Close it first.");
    br.chromiumRemoved += 1;
    update({ chromium: null });
    return ok(null);
  },
};
