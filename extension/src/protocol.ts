// What the service worker and the popup share: settings, the popup's messages, and the
// bridge protocol's constants (see README.md and crates/oxide-bridge in Oxide).

/** Version of the bridge protocol; Oxide refuses another. */
export const PROTOCOL = 1;
/** Oxide's default port (Settings → Chrome in Oxide). */
export const DEFAULT_PORT = 47820;

/** What `chrome.storage.local` holds. */
export interface Stored {
  port?: number;
  browserId?: string;
  token?: string;
  /** Per-site choices: origin → "allow" | "block". */
  sites?: Record<string, SiteChoice>;
  /** Ask in the tab before Oxide uses a site not chosen yet (off: every site but blocked ones). */
  askSites?: boolean;
}

export type SiteChoice = "allow" | "block";

export type LinkState =
  /** Never paired (or the pairing was removed). */
  | "unpaired"
  | "connecting"
  | "connected"
  /** Paired, but Oxide isn't reachable (not running, or Chrome is off in its settings). */
  | "offline";

export interface PopupStatus {
  state: LinkState;
  port: number;
  /** The last problem, written for users. */
  error: string | null;
  oxideVersion: string | null;
  sites: Record<string, SiteChoice>;
  askSites: boolean;
  /** Tabs Oxide is working in now. */
  working: { session: string; tabId: number | null; title: string }[];
}

/** Messages the popup and content scripts send to the service worker. */
export type Request =
  | { type: "status" }
  | { type: "pair"; code: string; port: number }
  | { type: "unpair" }
  | { type: "setPort"; port: number }
  | { type: "reconnect" }
  | { type: "setSite"; origin: string; choice: SiteChoice | null }
  | { type: "setAskSites"; on: boolean }
  | { type: "stopAll" }
  /** From a tab's banner. */
  | { type: "stop" };

export interface PairAnswer {
  ok: boolean;
  error?: string;
}

/** The origin of a site the user typed ("example.com", "http://127.0.0.1:8080/x"). */
export function siteOrigin(input: string): string | null {
  const s = input.trim();
  if (!s) return null;
  try {
    const local = /^(localhost|127\.|\[::1\])/i.test(s);
    const u = new URL(
      /^[a-z][a-z0-9+.-]*:\/\//i.test(s) ? s : `${local ? "http" : "https"}://${s}`,
    );
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    return u.origin;
  } catch {
    return null;
  }
}
