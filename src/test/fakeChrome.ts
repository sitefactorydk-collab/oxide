import type {
  ChromeStatus,
  CommandError,
  PairedBrowser,
  Result,
  commands as RealCommands,
} from "@/bindings";

/**
 * The Chrome bridge in the fake backend. It "listens" while Settings → Chrome is on (the fake
 * backend calls `configureFakeChrome` on save); a test pairs a browser with
 * `pairFakeChrome()` and connects or disconnects it with `setFakeChromeConnected()`.
 */
interface FakeChrome {
  status: ChromeStatus;
  forgotten: string[];
  pairings: number;
}

let ch: FakeChrome;
let emit: (s: ChromeStatus) => void = () => {};

export function setChromeEmitter(f: (s: ChromeStatus) => void): void {
  emit = f;
}

function initial(): ChromeStatus {
  return {
    enabled: false,
    listening: false,
    port: 47820,
    error: null,
    connected: null,
    extensionVersion: null,
    paired: [],
    pairing: null,
    sessions: [],
    extensionDir: "C:/AI Apps/Oxide/extension/dist",
  };
}

export function resetFakeChrome(): void {
  ch = { status: initial(), forgotten: [], pairings: 0 };
}

export function fakeChromeState(): Readonly<FakeChrome> {
  return ch;
}

function update(patch: Partial<ChromeStatus>): void {
  ch.status = { ...ch.status, ...patch };
  emit(ch.status);
}

/** Settings → Chrome was saved (`taken`: the port is in use). */
export function configureFakeChrome(enabled: boolean, port: number, taken = false): void {
  update({
    enabled,
    port,
    listening: enabled && !taken,
    error:
      enabled && taken
        ? `Port ${port} is in use by another program. Pick another port below.`
        : null,
    pairing: enabled ? ch.status.pairing : null,
    connected: enabled ? ch.status.connected : null,
  });
}

/** The extension typed the code: a paired, connected browser. */
export function pairFakeChrome(name = "Chrome 142 on Windows"): PairedBrowser {
  const b: PairedBrowser = {
    id: `b${ch.status.paired.length + 1}`,
    name,
    extensionId: "abcdefghijklmnopabcdefghijklmnop",
    pairedAt: Date.now(),
    lastSeen: Date.now(),
  };
  update({
    paired: [...ch.status.paired, b],
    pairing: null,
    connected: name,
    extensionVersion: "0.1.0",
  });
  return b;
}

export function setFakeChromeConnected(name: string | null): void {
  update({ connected: name, extensionVersion: name ? "0.1.0" : null });
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data });
const fail = (message: string): Promise<Result<never, CommandError>> =>
  Promise.resolve({ status: "error", error: { kind: "invalid", message } });

export const fakeChromeCommands: Pick<
  typeof RealCommands,
  | "chromeStatus"
  | "chromeStartPairing"
  | "chromeCancelPairing"
  | "chromeForget"
  | "chromeOpenExtensionFolder"
> = {
  chromeStatus: () => ok(ch.status),
  chromeStartPairing: () => {
    if (!ch.status.listening) return fail("Oxide isn't listening for Chrome.");
    ch.pairings += 1;
    const pairing = { code: "amber river toast mint lake ocean", expiresAt: Date.now() + 300_000 };
    update({ pairing });
    return ok(pairing);
  },
  chromeCancelPairing: () => {
    update({ pairing: null });
    return ok(null);
  },
  chromeForget: (browserId) => {
    ch.forgotten.push(browserId);
    const gone = ch.status.paired.find((p) => p.id === browserId);
    update({
      paired: ch.status.paired.filter((p) => p.id !== browserId),
      connected: gone && gone.name === ch.status.connected ? null : ch.status.connected,
    });
    return ok(null);
  },
  chromeOpenExtensionFolder: () => ok(null),
};
