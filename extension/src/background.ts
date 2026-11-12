// The extension's service worker: the link to Oxide (a WebSocket to 127.0.0.1 after pairing)
// and the tools Oxide's models call in Chrome.
//
// Protocol (JSON-RPC 2.0, see README.md):
//   → pair {code, name, version, protocol} / hello {browserId, token, name, version, protocol}
//   ← tools/call {session, name, arguments} → {content: [...], isError}
//   ← session/release {session}
//   → ping (every 20 s; keeps this worker awake), session/stopped {session}

import {
  DEFAULT_PORT,
  PROTOCOL,
  siteOrigin,
  type LinkState,
  type PairAnswer,
  type PopupStatus,
  type Request,
  type SiteChoice,
  type Stored,
} from "./protocol.js";

type Content = { type: "text"; text: string } | { type: "image"; data: string; mimeType: string };
interface ToolResult {
  content: Content[];
  isError?: boolean;
}
type Args = Record<string, unknown>;

interface Session {
  /** The tab this chat works in now. */
  tabId: number | null;
  /** Tabs Oxide opened for this chat (only these may be closed by it). */
  tabs: number[];
  /** Sites allowed "for this chat". */
  allowed: string[];
  /** The user pressed Stop; calls fail until Oxide releases the session. */
  stopped: boolean;
  /** The banner is up (the chat is working now). */
  active: boolean;
  /** Sites the chat works on (the console recorder runs there from the page's start). */
  origins?: string[];
}

/** An error the model should read (sent as an `isError` result). */
class ToolError extends Error {}

const HEARTBEAT_MS = 20_000;
/** Oxide pings every 20 s; nothing for this long means the link is gone. */
const SILENCE_MS = 70_000;
const READ_DEFAULT = 20_000;
const AFTER_ACTION_CHARS = 4_000;

let ws: WebSocket | null = null;
let state: LinkState = "unpaired";
let lastError: string | null = null;
let oxideVersion: string | null = null;
let nextId = 1;
let heartbeat: ReturnType<typeof setInterval> | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectDelay = 1_000;
let lastHeard = 0;
const waiting = new Map<
  number,
  (v: { result?: unknown; error?: { code: number; message: string } }) => void
>();
let sessions: Record<string, Session> = {};

const ready: Promise<void> = chrome.storage.session
  .get("sessions")
  .then((s) => {
    sessions = (s.sessions as Record<string, Session> | undefined) ?? {};
  })
  .catch(() => undefined);

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const stored = async () => (await chrome.storage.local.get(null)) as Stored;
const saveSessions = () => chrome.storage.session.set({ sessions }).catch(() => undefined);

function browserName(): string {
  const ua = navigator.userAgent;
  const edge = /Edg\/(\d+)/.exec(ua);
  const chrome = /Chrome\/(\d+)/.exec(ua);
  const os = /Windows/.test(ua)
    ? "Windows"
    : /Mac OS X/.test(ua)
      ? "macOS"
      : /Linux/.test(ua)
        ? "Linux"
        : "";
  const name = edge ? `Microsoft Edge ${edge[1]}` : chrome ? `Chrome ${chrome[1]}` : "Chrome";
  return os ? `${name} on ${os}` : name;
}

function setState(s: LinkState) {
  state = s;
  void updateBadge();
}

async function updateBadge() {
  const working = Object.values(sessions).some((s) => s.active && !s.stopped);
  try {
    await chrome.action.setBadgeBackgroundColor({ color: working ? "#c96442" : "#8a8580" });
    await chrome.action.setBadgeText({ text: working ? "ON" : state === "offline" ? "off" : "" });
    await chrome.action.setTitle({
      title:
        state === "connected"
          ? working
            ? "Oxide is working in Chrome"
            : "Oxide: connected"
          : state === "unpaired"
            ? "Oxide: not paired"
            : "Oxide: not connected",
    });
  } catch {
    // No action UI (tests).
  }
}

// ---- The link to Oxide ---------------------------------------------------------------------

function send(sock: WebSocket, msg: unknown) {
  if (sock.readyState === WebSocket.OPEN) sock.send(JSON.stringify(msg));
}

function request(
  sock: WebSocket,
  method: string,
  params: unknown,
  timeoutMs = 10_000,
): Promise<{ result?: unknown; error?: { code: number; message: string } }> {
  const id = nextId++;
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      waiting.delete(id);
      resolve({ error: { code: -1, message: "Oxide didn't answer." } });
    }, timeoutMs);
    waiting.set(id, (v) => {
      clearTimeout(timer);
      resolve(v);
    });
    send(sock, { jsonrpc: "2.0", id, method, params });
  });
}

function startHeartbeat(sock: WebSocket) {
  stopHeartbeat();
  lastHeard = Date.now();
  heartbeat = setInterval(() => {
    if (Date.now() - lastHeard > SILENCE_MS) {
      sock.close(4000, "No answer from Oxide");
      return;
    }
    void request(sock, "ping", {});
  }, HEARTBEAT_MS);
}

function stopHeartbeat() {
  if (heartbeat) clearInterval(heartbeat);
  heartbeat = null;
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    void connect();
  }, reconnectDelay);
  reconnectDelay = Math.min(reconnectDelay * 2, 30_000);
}

/**
 * Opens a socket and sends its first message (pair or hello). Resolves with the answer; on
 * success the socket becomes the link.
 */
function openLink(
  port: number,
  method: "pair" | "hello",
  params: Args,
): Promise<PairAnswer & { result?: Args }> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (a: PairAnswer & { result?: Args }) => {
      if (!settled) {
        settled = true;
        resolve(a);
      }
    };
    let sock: WebSocket;
    try {
      sock = new WebSocket(`ws://127.0.0.1:${port}/`);
    } catch (e) {
      finish({ ok: false, error: String(e) });
      return;
    }
    const timer = setTimeout(() => {
      sock.close();
      finish({ ok: false, error: "Oxide didn't answer in time." });
    }, 10_000);
    sock.onmessage = (e) => {
      lastHeard = Date.now();
      void onMessage(sock, String(e.data));
    };
    sock.onclose = (e) => {
      clearTimeout(timer);
      finish({
        ok: false,
        error: `Oxide isn't reachable on port ${port}. Check that Oxide is running and that Settings → Chrome is on there, with the same port.`,
      });
      onClosed(sock, e.code, e.reason);
    };
    sock.onopen = async () => {
      const r = await request(sock, method, {
        ...params,
        name: browserName(),
        version: chrome.runtime.getManifest().version,
        protocol: PROTOCOL,
      });
      clearTimeout(timer);
      if (r.error) {
        lastError = r.error.message;
        if (method === "hello" && (r.error.code === -32001 || r.error.code === -32004)) {
          await chrome.storage.local.remove(["token", "browserId"]);
          setState("unpaired");
        }
        finish({ ok: false, error: r.error.message });
        sock.close();
        return;
      }
      const result = (r.result ?? {}) as Args;
      if (ws && ws !== sock) ws.close(1000, "Replaced");
      ws = sock;
      oxideVersion = typeof result.oxideVersion === "string" ? result.oxideVersion : null;
      lastError = null;
      reconnectDelay = 1_000;
      setState("connected");
      startHeartbeat(sock);
      finish({ ok: true, result });
    };
  });
}

function onClosed(sock: WebSocket, code: number, reason: string) {
  if (ws !== sock) return;
  ws = null;
  stopHeartbeat();
  for (const [id, cb] of waiting) {
    waiting.delete(id);
    cb({ error: { code: -1, message: "Disconnected" } });
  }
  void (async () => {
    if (code === 4001 || code === 4003) {
      await chrome.storage.local.remove(["token", "browserId"]);
      lastError =
        code === 4003
          ? "Oxide removed this pairing. Pair again to reconnect."
          : reason || "Oxide refused the pairing.";
      setState("unpaired");
      return;
    }
    if (code === 4002) return; // This browser connected again on another socket.
    // Take banners down: nothing is controlling the tabs now.
    for (const sid of Object.keys(sessions)) await endSession(sid, false);
    setState("offline");
    scheduleReconnect();
  })();
}

async function connect() {
  await ready;
  const s = await stored();
  if (!s.token || !s.browserId) {
    setState("unpaired");
    return;
  }
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;
  if (state === "connecting") return;
  setState("connecting");
  const r = await openLink(s.port ?? DEFAULT_PORT, "hello", {
    browserId: s.browserId,
    token: s.token,
  });
  // `state` may have changed while waiting (a refused hello makes it "unpaired").
  if (!r.ok && (state as LinkState) === "connecting") {
    lastError = r.error ?? null;
    setState("offline");
    scheduleReconnect();
  }
}

async function pair(code: string, port: number): Promise<PairAnswer> {
  await ready;
  await chrome.storage.local.set({ port });
  if (ws) {
    const old = ws;
    ws = null;
    old.close(1000, "Pairing again");
  }
  setState("connecting");
  const r = await openLink(port, "pair", { code });
  if (r.ok && r.result) {
    await chrome.storage.local.set({ browserId: r.result.browserId, token: r.result.token });
    return { ok: true };
  }
  const s = await stored();
  setState(s.token ? "offline" : "unpaired");
  lastError = r.error ?? null;
  return { ok: false, error: r.error };
}

async function onMessage(sock: WebSocket, text: string) {
  let msg: {
    id?: number | string;
    method?: string;
    params?: Args;
    result?: unknown;
    error?: { code: number; message: string };
  };
  try {
    msg = JSON.parse(text);
  } catch {
    return;
  }
  if (msg.method) {
    const p = msg.params ?? {};
    if (msg.method === "tools/call") {
      const result = await runTool(
        String(p.session ?? ""),
        String(p.name ?? ""),
        (p.arguments as Args) ?? {},
      );
      send(sock, { jsonrpc: "2.0", id: msg.id, result });
    } else if (msg.method === "session/release") {
      await endSession(String(p.session ?? ""), true);
    } else if (msg.id !== undefined) {
      send(sock, {
        jsonrpc: "2.0",
        id: msg.id,
        error: { code: -32601, message: "Unknown method." },
      });
    }
    return;
  }
  if (typeof msg.id === "number") {
    const cb = waiting.get(msg.id);
    if (cb) {
      waiting.delete(msg.id);
      cb({ result: msg.result, error: msg.error });
    }
  }
}

// ---- Sessions and tabs -----------------------------------------------------------------------

function session(sid: string): Session {
  let s = sessions[sid];
  if (!s) {
    s = { tabId: null, tabs: [], allowed: [], stopped: false, active: false };
    sessions[sid] = s;
  }
  return s;
}

async function setBanner(tabId: number, on: boolean) {
  try {
    if (on) await inject(tabId);
    await page(tabId, "banner", { on });
  } catch {
    // The tab is gone or can't be scripted.
  }
}

/** The chat stops working in Chrome: banners come down. `release`: Oxide ended it. */
async function endSession(sid: string, release: boolean) {
  const s = sessions[sid];
  if (!s) return;
  // Before any await: a call that follows the release must already see it.
  s.active = false;
  if (release) s.stopped = false;
  const tabs = new Set([...(s.tabId !== null ? [s.tabId] : []), ...s.tabs]);
  for (const t of tabs) await setBanner(t, false);
  await saveSessions();
  await syncConsoleRecorder();
  void updateBadge();
}

async function stopSessions(sids: string[]) {
  for (const sid of sids) {
    const s = sessions[sid];
    if (!s) continue;
    s.stopped = true;
    await endSession(sid, false);
    if (ws) send(ws, { jsonrpc: "2.0", method: "session/stopped", params: { session: sid } });
  }
  await saveSessions();
}

chrome.tabs.onRemoved.addListener((tabId) => {
  void ready.then(() => {
    for (const s of Object.values(sessions)) {
      s.tabs = s.tabs.filter((t) => t !== tabId);
      if (s.tabId === tabId) s.tabId = s.tabs[s.tabs.length - 1] ?? null;
    }
    return saveSessions();
  });
});

/**
 * Records console messages from the very start of pages on the sites a working chat is on: a
 * MAIN-world script at document_start, registered only while a chat works there.
 */
let recorderKey = "";
async function syncConsoleRecorder() {
  const hosts = new Set<string>();
  for (const s of Object.values(sessions)) {
    if (!s.active || s.stopped) continue;
    for (const o of s.origins ?? []) {
      try {
        const u = new URL(o);
        hosts.add(`${u.protocol}//${u.hostname}/*`);
      } catch {
        // Not a web origin.
      }
    }
  }
  const key = [...hosts].sort().join(" ");
  if (key === recorderKey) return;
  recorderKey = key;
  try {
    await chrome.scripting.unregisterContentScripts({ ids: ["oxide-console"] });
  } catch {
    // Wasn't registered.
  }
  if (hosts.size === 0) return;
  try {
    await chrome.scripting.registerContentScripts([
      {
        id: "oxide-console",
        js: ["console-hook.js"],
        matches: [...hosts],
        runAt: "document_start",
        world: "MAIN",
        persistAcrossSessions: false,
      },
    ]);
  } catch {
    recorderKey = "";
  }
}

/** Notes a site the chat works on. */
async function workOn(s: Session, url: string | undefined) {
  const origin = url ? siteOrigin(url) : null;
  if (!origin) return;
  s.origins ??= [];
  if (!s.origins.includes(origin)) s.origins.push(origin);
  await syncConsoleRecorder();
}

/** Tabs opened by Oxide's tab (a link with target=_blank) become the chat's tab. */
const followed = new Map<number, number>();
chrome.tabs.onCreated.addListener((tab) => {
  void ready.then(() => {
    if (tab.openerTabId === undefined || tab.id === undefined) return;
    for (const s of Object.values(sessions)) {
      if (s.active && s.tabId === tab.openerTabId) {
        s.tabs.push(tab.id);
        s.tabId = tab.id;
        followed.set(tab.openerTabId, tab.id);
        void addToGroup(tab.id);
        void saveSessions();
      }
    }
  });
});

async function addToGroup(tabId: number) {
  try {
    const groupId = await chrome.tabs.group({ tabIds: tabId });
    await chrome.tabGroups.update(groupId, { title: "Oxide", color: "orange" });
  } catch {
    // Tab groups aren't available (or the tab is in another window type).
  }
}

async function currentTab(s: Session): Promise<chrome.tabs.Tab> {
  if (s.tabId !== null) {
    try {
      return await chrome.tabs.get(s.tabId);
    } catch {
      s.tabId = null;
    }
  }
  throw new ToolError(
    "No tab is open for this chat yet. Use navigate or tabs_create to open one, or tabs_list and tabs_select to work in one of the user's tabs.",
  );
}

async function newTab(s: Session, url?: string): Promise<chrome.tabs.Tab> {
  const tab = await chrome.tabs.create({ url: url ?? "about:blank", active: true });
  if (tab.id === undefined) throw new ToolError("Chrome didn't open a tab.");
  s.tabs.push(tab.id);
  s.tabId = tab.id;
  await addToGroup(tab.id);
  await saveSessions();
  return tab;
}

/** Waits for a tab to finish loading (`expectChange`: a navigation was just started). */
async function waitLoad(
  tabId: number,
  expectChange: boolean,
  ms = 30_000,
): Promise<chrome.tabs.Tab> {
  const start = Date.now();
  let sawLoading = !expectChange;
  while (Date.now() - start < ms) {
    let t: chrome.tabs.Tab;
    try {
      t = await chrome.tabs.get(tabId);
    } catch {
      throw new ToolError("The tab was closed.");
    }
    if (t.status === "loading") sawLoading = true;
    if (t.status === "complete" && (sawLoading || Date.now() - start > 1_500)) return t;
    await sleep(100);
  }
  return chrome.tabs.get(tabId);
}

function restricted(url: string): boolean {
  return (
    (/^(chrome|edge|about|devtools|view-source|chrome-extension|chrome-search):/i.test(url) &&
      url !== "about:blank") ||
    /^https:\/\/(chrome\.google\.com\/webstore|chromewebstore\.google\.com)/i.test(url)
  );
}

/** Puts Oxide's content script (and the console recorder) into a tab. */
async function inject(tabId: number) {
  const tab = await chrome.tabs.get(tabId);
  const url = tab.url ?? tab.pendingUrl ?? "";
  if (restricted(url)) {
    throw new ToolError(
      `Chrome doesn't let extensions read or act on ${url} (browser pages and the Chrome Web Store are off limits). Open a normal web page instead.`,
    );
  }
  try {
    await chrome.scripting.executeScript({
      target: { tabId, frameIds: [0] },
      files: ["content.js"],
    });
    await chrome.scripting.executeScript({
      target: { tabId, frameIds: [0] },
      files: ["console-hook.js"],
      world: "MAIN",
    });
  } catch (e) {
    throw new ToolError(
      `Oxide can't work in this tab (${url}): ${e instanceof Error ? e.message : String(e)}`,
    );
  }
}

/** Runs one of the content script's operations in a tab. */
async function page(tabId: number, op: string, args: Args = {}): Promise<Args> {
  let res: chrome.scripting.InjectionResult<Args>[];
  try {
    res = await chrome.scripting.executeScript<Args>({
      target: { tabId, frameIds: [0] },
      func: (o: string, a: Args) =>
        (globalThis as unknown as { __oxideRun: (o: string, a: Args) => Promise<Args> }).__oxideRun(
          o,
          a,
        ),
      args: [op, args],
    });
  } catch (e) {
    throw new ToolError(
      `The page couldn't be reached: ${e instanceof Error ? e.message : String(e)}`,
    );
  }
  const r = res[0]?.result;
  if (!r) throw new ToolError("The page didn't answer (it may have navigated away). Try again.");
  if (typeof r.error === "string") throw new ToolError(r.error);
  return r;
}

const WHAT: Record<string, string> = {
  navigate: "read and act on this page",
  read_page: "read this page",
  find: "look for something on this page",
  click: "click on this page",
  type: "type on this page",
  press_key: "press keys on this page",
  scroll: "scroll this page",
  form_input: "fill in a form on this page",
  screenshot: "take a screenshot of this page",
  get_console: "read this page's console messages",
};

/** Gets the tab ready for a tool: content script, banner, and the site's permission. */
async function prepare(s: Session, tab: chrome.tabs.Tab, tool: string) {
  if (tab.id === undefined) throw new ToolError("That tab can't be used.");
  await inject(tab.id);
  s.active = true;
  await workOn(s, tab.url);
  await page(tab.id, "banner", { on: true });
  void updateBadge();
  await allowSite(s, tab, WHAT[tool] ?? "work");
}

async function allowSite(s: Session, tab: chrome.tabs.Tab, what: string) {
  const url = tab.url ?? "";
  if (!/^https?:/i.test(url)) return; // about:blank and the like: nothing to protect.
  const origin = new URL(url).origin;
  const host = new URL(url).host;
  const sites = (await stored()).sites ?? {};
  if (sites[origin] === "block") {
    throw new ToolError(
      `The user blocked Oxide on ${host} (in the Oxide extension's popup). Don't use this site; tell the user if you need it.`,
    );
  }
  if (sites[origin] === "allow" || s.allowed.includes(origin)) return;
  if (tab.id === undefined) return;
  try {
    await chrome.tabs.update(tab.id, { active: true });
    await chrome.windows.update(tab.windowId, { focused: true });
  } catch {
    // Still ask in the tab.
  }
  const r = await page(tab.id, "ask", { site: host, what });
  if (r.answer === "always") {
    await chrome.storage.local.set({ sites: { ...sites, [origin]: "allow" } });
  } else if (r.answer === "once") {
    s.allowed.push(origin);
    await saveSessions();
  } else if (r.answer === "timeout") {
    throw new ToolError(
      `The user didn't answer whether Oxide may use ${host} (asked in Chrome). Ask them in the chat.`,
    );
  } else {
    throw new ToolError(
      `The user didn't allow Oxide to use ${host}. Don't try this site again unless they ask.`,
    );
  }
}

// ---- Tools -------------------------------------------------------------------------------------

const text = (t: string): ToolResult => ({ content: [{ type: "text", text: t }] });

function slice(t: string, start: number, max: number, tool = "read_page"): string {
  const total = t.length;
  const from = Math.max(0, Math.min(start, total));
  const end = Math.min(total, from + max);
  let part = t.slice(from, end);
  const notes: string[] = [];
  if (from > 0) notes.push(`[Starts at character ${from} of ${total}.]`);
  if (end < total)
    notes.push(`[Cut at character ${end} of ${total}: call ${tool} with start=${end} to read on.]`);
  if (notes.length) part += "\n\n" + notes.join(" ");
  return part;
}

async function pageText(
  tabId: number,
  max: number,
  start = 0,
  onlyInteractive = false,
): Promise<string> {
  const r = await page(tabId, "read");
  let body = String(r.text ?? "");
  if (onlyInteractive)
    body = body
      .split("\n")
      .filter((l) => /\[\d+\]/.test(l))
      .join("\n");
  if (!body.trim()) body = "[The page has no readable text yet.]";
  return `# ${String(r.title ?? "")}\nURL: ${String(r.url ?? "")}\n\n${slice(body, start, max)}`;
}

/** What an address or search words become. */
function target(input: string): string {
  const s = input.trim();
  if (/^[a-z][a-z0-9+.-]*:/i.test(s) && !/^(https?):/i.test(s) && !/^localhost:\d/i.test(s)) {
    throw new ToolError(`Oxide only opens http and https addresses, not “${s.slice(0, 80)}”.`);
  }
  if (/^https?:\/\//i.test(s)) return s;
  if (
    !/\s/.test(s) &&
    (/^[\w-]+(\.[\w-]+)+(:\d+)?([/?#]|$)/.test(s) || /^localhost(:\d+)?([/?#]|$)/i.test(s))
  ) {
    return (/^(localhost|127\.)/i.test(s) ? "http://" : "https://") + s;
  }
  return `https://www.google.com/search?hl=en&q=${encodeURIComponent(s)}`;
}

/** After a click or key: wait for a navigation it started, and follow a new tab. */
async function settle(s: Session, tabId: number): Promise<number> {
  await sleep(350);
  const opened = followed.get(tabId);
  if (opened !== undefined) {
    followed.delete(tabId);
    await waitLoad(opened, false, 15_000);
    return opened;
  }
  try {
    const t = await chrome.tabs.get(tabId);
    if (t.status === "loading") await waitLoad(tabId, false, 15_000);
  } catch {
    // Closed by the page.
  }
  return s.tabId ?? tabId;
}

async function afterAction(
  s: Session,
  tabId: number,
  did: string,
  chars = AFTER_ACTION_CHARS,
): Promise<ToolResult> {
  const now = await settle(s, tabId);
  let note = "";
  if (now !== tabId) note = " It opened a new tab, which Oxide works in now.";
  const tab = await chrome.tabs.get(now);
  await prepare(s, tab, "read_page");
  return text(`${did}.${note}\n\nThe page now:\n${await pageText(now, chars)}`);
}

async function tool(sid: string, name: string, a: Args): Promise<ToolResult> {
  const s = session(sid);
  if (s.stopped) {
    throw new ToolError(
      "The user pressed Stop in Chrome, so Oxide's control of the browser ended. Don't continue in Chrome unless the user asks again.",
    );
  }
  // Working now: a page this chat's tab starts loading gets the console recorder at once.
  if (!s.active) {
    s.active = true;
    await saveSessions();
  }
  switch (name) {
    case "tabs_list": {
      const tabs = await chrome.tabs.query({});
      const lines = tabs.map((t) => {
        const marks = [
          t.active ? "active" : "",
          t.id === s.tabId
            ? "this chat's tab"
            : s.tabs.includes(t.id ?? -1)
              ? "opened by Oxide"
              : "",
        ].filter(Boolean);
        return `[${t.id}] ${t.title || "(no title)"} — ${t.url ?? ""}${marks.length ? ` (${marks.join(", ")})` : ""}`;
      });
      return text(`${tabs.length} tabs:\n${lines.join("\n")}`);
    }
    case "tabs_create": {
      const url = typeof a.url === "string" && a.url.trim() ? target(a.url) : undefined;
      if (url) await checkBlocked(url);
      if (url) await workOn(s, url);
      const tab = await newTab(s, url);
      if (!url || tab.id === undefined)
        return text(`Opened a new empty tab [${tab.id}]; this chat works in it now.`);
      const t = await waitLoad(tab.id, true);
      await prepare(s, t, "navigate");
      return text(`Opened a new tab [${tab.id}].\n\n${await pageText(tab.id, 8_000)}`);
    }
    case "tabs_select": {
      const id = Number(a.tab_id);
      let tab: chrome.tabs.Tab;
      try {
        tab = await chrome.tabs.get(id);
      } catch {
        throw new ToolError(
          `There is no tab [${a.tab_id}]. Call tabs_list for the tabs there are.`,
        );
      }
      s.tabId = id;
      await chrome.tabs.update(id, { active: true });
      await saveSessions();
      return text(`This chat works in tab [${id}] now: ${tab.title ?? ""} — ${tab.url ?? ""}`);
    }
    case "tabs_close": {
      const id = a.tab_id === undefined || a.tab_id === null ? s.tabId : Number(a.tab_id);
      if (id === null) throw new ToolError("No tab to close.");
      if (!s.tabs.includes(id)) {
        throw new ToolError(
          `Oxide only closes tabs it opened for this chat; tab [${id}] is the user's. Ask them to close it.`,
        );
      }
      await chrome.tabs.remove(id);
      s.tabs = s.tabs.filter((t) => t !== id);
      if (s.tabId === id) s.tabId = s.tabs[s.tabs.length - 1] ?? null;
      await saveSessions();
      return text(
        `Closed tab [${id}].${s.tabId !== null ? ` This chat works in tab [${s.tabId}] now.` : ""}`,
      );
    }
    case "navigate": {
      const raw = String(a.url ?? "").trim();
      if (!raw) throw new ToolError("navigate needs a url.");
      let tab = s.tabId !== null ? await currentTab(s).catch(() => null) : null;
      if (["back", "forward", "reload"].includes(raw.toLowerCase())) {
        if (!tab || tab.id === undefined) throw new ToolError("No tab to go back or forward in.");
        const id = tab.id;
        const where = raw.toLowerCase();
        if (where === "reload") {
          await chrome.tabs.reload(id);
        } else {
          // The page's own history: chrome.tabs.goBack skips entries made without a user
          // gesture (a form Oxide submitted), which is not what the model means.
          const before = tab.url;
          await chrome.scripting
            .executeScript({
              target: { tabId: id, frameIds: [0] },
              func: (back: boolean) => (back ? history.back() : history.forward()),
              args: [where === "back"],
            })
            .catch(() => (where === "back" ? chrome.tabs.goBack(id) : chrome.tabs.goForward(id)));
          for (let i = 0; i < 20 && (await chrome.tabs.get(id)).url === before; i++)
            await sleep(100);
        }
        const t = await waitLoad(id, true, 30_000);
        await prepare(s, t, "navigate");
        return text(await pageText(id, 8_000));
      }
      const url = target(raw);
      await checkBlocked(url);
      await workOn(s, url);
      if (!tab || tab.id === undefined) tab = await newTab(s, url);
      else await chrome.tabs.update(tab.id, { url });
      const id = tab.id as number;
      const t = await waitLoad(id, true);
      await prepare(s, t, "navigate");
      return text(`${url !== raw ? `(${raw} → ${url})\n` : ""}${await pageText(id, 8_000)}`);
    }
    case "read_page": {
      const tab = await currentTab(s);
      await prepare(s, tab, name);
      const max = Math.max(
        1_000,
        Math.min(100_000, Number(a.max_chars ?? READ_DEFAULT) || READ_DEFAULT),
      );
      const start = Math.max(0, Number(a.start ?? 0) || 0);
      return text(await pageText(tab.id as number, max, start, a.filter === "interactive"));
    }
    case "find": {
      const tab = await currentTab(s);
      await prepare(s, tab, name);
      const r = await page(tab.id as number, "find", { query: a.query, limit: a.limit });
      const matches = (r.matches as string[]) ?? [];
      const lines = (r.text as string[]) ?? [];
      let out = matches.length
        ? `Elements matching “${String(a.query)}”:\n${matches.join("\n")}`
        : `No links, buttons or fields match “${String(a.query)}”.`;
      if (lines.length)
        out += `\n\nText on the page that mentions it:\n${lines.map((l) => `- ${l}`).join("\n")}`;
      return text(out);
    }
    case "click":
    case "type":
    case "press_key":
    case "form_input":
    case "scroll": {
      const tab = await currentTab(s);
      await prepare(s, tab, name);
      const id = tab.id as number;
      const op = {
        click: "click",
        type: "type",
        press_key: "key",
        form_input: "formInput",
        scroll: "scroll",
      }[name];
      const r = await page(id, op, a);
      const did = String(r.did ?? "Done");
      if (name === "scroll" || name === "form_input") return text(`${did}.`);
      if (name === "type" && !a.submit) return text(`${did}.`);
      return afterAction(s, id, did);
    }
    case "screenshot": {
      const tab = await currentTab(s);
      await prepare(s, tab, name);
      const id = tab.id as number;
      await chrome.tabs.update(id, { active: true });
      await page(id, "banner", { on: false });
      await sleep(150);
      let dataUrl: string;
      try {
        dataUrl = await chrome.tabs.captureVisibleTab(tab.windowId, {
          format: "jpeg",
          quality: 70,
        });
      } finally {
        await page(id, "banner", { on: true }).catch(() => undefined);
      }
      const info = await chrome.tabs.get(id);
      return {
        content: [
          {
            type: "text",
            text: `Screenshot of “${info.title ?? ""}” (${info.url ?? ""}). To click a spot in it, pass its pixel x and y to click.`,
          },
          {
            type: "image",
            data: dataUrl.replace(/^data:image\/\w+;base64,/, ""),
            mimeType: "image/jpeg",
          },
        ],
      };
    }
    case "get_console": {
      const tab = await currentTab(s);
      await prepare(s, tab, name);
      const r = await page(tab.id as number, "console", a);
      const entries = (r.entries as { level: string; text: string }[]) ?? [];
      const head = "Console messages since Oxide started working in this tab";
      return text(
        entries.length
          ? `${head} (${entries.length}):\n${entries.map((e) => `[${e.level}] ${e.text}`).join("\n")}`
          : `${head}: none.`,
      );
    }
    default:
      throw new ToolError(`Unknown tool ${name}.`);
  }
}

async function checkBlocked(url: string) {
  const origin = siteOrigin(url);
  if (!origin) return;
  const sites = (await stored()).sites ?? {};
  if (sites[origin] === "block") {
    throw new ToolError(
      `The user blocked Oxide on ${new URL(origin).host} (in the Oxide extension's popup).`,
    );
  }
}

async function runTool(sid: string, name: string, a: Args): Promise<ToolResult> {
  await ready;
  try {
    return await tool(sid, name, a);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return { content: [{ type: "text", text: msg }], isError: true };
  } finally {
    await saveSessions();
  }
}

// ---- The popup and the tabs' banners ---------------------------------------------------------

async function status(): Promise<PopupStatus> {
  await ready;
  const s = await stored();
  const working: PopupStatus["working"] = [];
  for (const [sid, sess] of Object.entries(sessions)) {
    if (!sess.active || sess.stopped) continue;
    let title = "";
    if (sess.tabId !== null)
      title = (await chrome.tabs.get(sess.tabId).catch(() => null))?.title ?? "";
    working.push({ session: sid, tabId: sess.tabId, title });
  }
  return {
    state,
    port: s.port ?? DEFAULT_PORT,
    error: lastError,
    oxideVersion,
    sites: s.sites ?? {},
    working,
  };
}

async function handle(req: Request, sender: chrome.runtime.MessageSender): Promise<unknown> {
  switch (req.type) {
    case "status":
      return status();
    case "pair":
      return pair(req.code, req.port);
    case "unpair": {
      await chrome.storage.local.remove(["token", "browserId"]);
      if (ws) {
        const old = ws;
        ws = null;
        old.close(1000, "Unpaired");
      }
      lastError = null;
      setState("unpaired");
      return status();
    }
    case "setPort":
      await chrome.storage.local.set({ port: req.port });
      if (ws) ws.close(1000, "Port changed");
      void connect();
      return status();
    case "reconnect":
      reconnectDelay = 1_000;
      void connect();
      return status();
    case "setSite": {
      const origin = siteOrigin(req.origin);
      if (!origin) return { error: "That isn't a web address." };
      const sites: Record<string, SiteChoice> = { ...((await stored()).sites ?? {}) };
      if (req.choice) sites[origin] = req.choice;
      else delete sites[origin];
      await chrome.storage.local.set({ sites });
      return status();
    }
    case "stopAll":
      await stopSessions(Object.keys(sessions).filter((k) => sessions[k]?.active));
      return status();
    case "stop": {
      const tabId = sender.tab?.id;
      if (tabId === undefined) return null;
      await ready;
      const sids = Object.entries(sessions)
        .filter(([, s]) => s.tabId === tabId || s.tabs.includes(tabId))
        .map(([k]) => k);
      await stopSessions(sids);
      return null;
    }
  }
}

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  void handle(message as Request, sender).then(sendResponse, (e) =>
    sendResponse({ error: String(e) }),
  );
  return true;
});

// ---- Waking up ----------------------------------------------------------------------------------
// A WebSocket with traffic every 20 s keeps an MV3 service worker alive (Chrome 116+). If Chrome
// still stops it, the alarm wakes it within 30 s and it reconnects.

chrome.alarms.create("oxide-keepalive", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "oxide-keepalive" && !ws) void connect();
});
chrome.runtime.onStartup.addListener(() => void connect());
chrome.runtime.onInstalled.addListener(() => void connect());
void connect();
