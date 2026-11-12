# Oxide for Chrome

A Manifest V3 extension that lets the Oxide desktop app work in tabs of your everyday Chrome
(or Edge, Brave, or another Chromium browser), after you pair it with Oxide.

## Install (load unpacked)

1. Build it: `npm run extension` in the Oxide folder. The result is `extension/dist`.
2. Open `chrome://extensions`, switch on **Developer mode** (top right), click **Load unpacked**
   and pick the `extension/dist` folder.
3. Pin the Oxide icon (puzzle-piece menu → pin) so its popup is one click away.

After `git pull` or a rebuild, click the reload arrow on the extension's card in
`chrome://extensions`.

## Pair

1. In Oxide: **Settings → Chrome**, switch on **Let models use Chrome**, click **Pair**. Oxide
   shows six words (valid for 5 minutes, 5 tries).
2. In Chrome: click the Oxide icon, type the six words (and the port, if you changed it in Oxide),
   click **Pair**. The popup says **Connected to Oxide**.

Pairing gives the extension a long random token. Oxide keeps it in the system keychain, the
extension in `chrome.storage.local`. It only works from this extension (its id) on this
computer. **Unpair** in the popup, or **Forget** in Oxide's Settings → Chrome, ends it.

## What it can do

Oxide's models get these tools (as `chrome__<tool>`) while a paired Chrome is connected:

| Tool | Does |
|---|---|
| `tabs_list` | Lists the open tabs (id, title, address). |
| `tabs_create` | Opens a new tab (optionally at an address); the chat works in it. |
| `tabs_select` | Makes one of your tabs the one the chat works in. |
| `tabs_close` | Closes a tab Oxide opened for this chat (never yours). |
| `navigate` | Opens an address (or searches Google for words), or goes `back` / `forward` / `reload`. |
| `read_page` | The page as text, with links, buttons and fields numbered `[3] button "Search"`. |
| `find` | Finds numbered elements matching words. |
| `click` | Clicks an element by number, by its text, or at a spot of a screenshot. |
| `type` | Types into a field (optionally pressing Enter). |
| `press_key` | Presses a key or combination (`Enter`, `Escape`, `Tab`, `ctrl+a`). |
| `scroll` | Scrolls the page or an element into view. |
| `form_input` | Sets a select, checkbox, radio button or field. |
| `screenshot` | A JPEG of the visible part of the tab (`chrome.tabs.captureVisibleTab`). |
| `get_console` | Console messages and uncaught errors of the page. |

Tabs Oxide opens go into an orange **Oxide** tab group.

## Safety

- **Per-site permission.** The first time a chat uses a site, the tab shows a prompt: *Don't
  allow*, *Allow for this chat* or *Always allow*. The popup lists your choices; you can remove
  them, or allow or block a site in advance.
- **A banner and Stop.** While Oxide works in a tab, the tab has an orange frame and an "Oxide is
  controlling this tab" pill with **Stop**. Stop ends Oxide's control and stops the reply in
  Oxide. The popup's **Stop Oxide** does the same for every tab.
- **No passwords or payment cards.** `type` and `form_input` refuse password, one-time-code and
  card fields; the model is told to ask you.
- **Browser pages are off limits.** `chrome://` pages and the Chrome Web Store can't be read or
  scripted by any extension.
- Oxide itself asks before acting (opening, clicking, typing) unless you switch that off in
  Settings → Chrome.
- The prompt and the banner live in closed shadow roots and only react to real clicks, so a page
  can't press them for you.

## Limits

- Clicks and keys are synthetic DOM events: they work on nearly all sites, but a page can tell them
  from real input, and keys have no default action except the ones the extension carries out
  (Enter submits, Tab moves focus, PageUp/PageDown/Home/End scroll).
- Only the top frame of a page is read and acted on (not iframes).
- Console messages are recorded on the sites a chat works on, from the start of each page load
  while it works there (a small script in the page's own world). Messages from before that are
  not seen. While it records, DevTools shows `console-hook.js` as the source of console lines.

## How it talks to Oxide

A WebSocket to `ws://127.0.0.1:<port>/` (default 47820). Oxide accepts only
`chrome-extension://` origins with a `Host` of this computer, and only `pair` or `hello` as the
first message (within 10 seconds). Messages are JSON-RPC 2.0:

| Direction | Message |
|---|---|
| → Oxide | `pair {code, name, version, protocol: 1}` → `{browserId, token, oxideVersion}` |
| → Oxide | `hello {browserId, token, name, version, protocol: 1}` → `{browserId, oxideVersion}` |
| → Oxide | `ping` every 20 s → `"pong"` (keeps the service worker awake) |
| → Oxide | `session/stopped {session}` (notification: Stop was pressed) |
| ← Oxide | `tools/call {session, name, arguments}` → `{content: [{type: "text" \| "image", …}], isError}` |
| ← Oxide | `session/release {session}` (notification: the chat's reply ended; banners come down) |

Errors: `-32001` unknown pairing (the extension forgets its token), `-32002` no code waiting,
`-32003` wrong code, `-32004` protocol mismatch. Close codes: `4001` unauthorized, `4002`
replaced by a newer connection of the same browser, `4003` unpaired in Oxide.

**Keep-alive:** Chrome keeps a service worker running while WebSocket messages flow (Chrome 116+);
Oxide pings every 20 s and the extension pings back. If Chrome stops the worker anyway, a
30-second `chrome.alarms` alarm wakes it and it reconnects (with backoff up to 30 s).

## Develop and test

- `npm run extension` builds; `npm run typecheck` checks it with the rest of the frontend.
- `crates/oxide-bridge/tests/real_chrome.rs` pairs the built extension in a real Chrome through its
  popup and drives every tool against a local test site (needs Python Playwright and a Chrome that
  loads unpacked extensions from the command line, such as Chrome for Testing):
  `OXIDE_TEST_CHROME=/path/to/chrome cargo test -p oxide-bridge --test real_chrome -- --ignored`.

## Chrome Web Store (later)

Not published yet. Notes for when it is:

- `npm run extension`, then zip the **contents** of `extension/dist` (manifest at the zip's root).
- Store review will ask why it needs `<all_urls>`, `scripting`, `tabs` and `tabGroups`: Oxide
  reads and acts on whatever site the user asks for, per site after the user allows it.
- The store gives the extension a fixed id. Pairings are bound to the id, so users of an unpacked
  build pair again after switching.
- Privacy: nothing leaves the computer except to Oxide on `127.0.0.1`; what Oxide sends to its
  model provider is up to the user's settings there.
- Store listing icons (128×128) and screenshots (1280×800) need making; the icons here come from
  Oxide's own logo.
