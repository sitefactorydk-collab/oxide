// The toolbar popup: pairing, the link's state, what Oxide is doing, and per-site choices.

import {
  DEFAULT_PORT,
  type PairAnswer,
  type PopupStatus,
  type Request,
  type SiteChoice,
} from "./protocol.js";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const ask = <T>(req: Request) => chrome.runtime.sendMessage<T>(req);

const STATE_TEXT: Record<PopupStatus["state"], string> = {
  unpaired: "Not paired",
  connecting: "Connecting…",
  connected: "Connected to Oxide",
  offline: "Paired · Oxide isn't reachable",
};

let showPairing = false;

function render(s: PopupStatus) {
  const state = $("state");
  state.dataset.state = s.state;
  $("state-text").textContent =
    STATE_TEXT[s.state] + (s.state === "connected" && s.oxideVersion ? ` ${s.oxideVersion}` : "");
  const error = $("error");
  error.hidden = !s.error || s.state === "connected";
  error.textContent = s.error ?? "";

  $("pair").hidden = !(s.state === "unpaired" || showPairing);
  const port = $<HTMLInputElement>("port");
  if (!port.value) port.value = String(s.port || DEFAULT_PORT);
  $("paired").hidden = s.state === "unpaired";
  $("port-line").textContent = `Oxide on port ${s.port}`;

  $("working").hidden = s.working.length === 0;
  const list = $("working-list");
  list.replaceChildren(
    ...s.working.map((w) => {
      const li = document.createElement("li");
      const span = document.createElement("span");
      span.className = "site";
      span.textContent = w.title || (w.tabId !== null ? `Tab ${w.tabId}` : "A tab");
      li.append(span);
      return li;
    }),
  );

  const sites = $("sites");
  const entries = Object.entries(s.sites).sort(([a], [b]) => a.localeCompare(b));
  if (entries.length === 0) {
    const li = document.createElement("li");
    li.className = "hint";
    li.textContent = "None yet.";
    sites.replaceChildren(li);
  } else {
    sites.replaceChildren(...entries.map(([origin, choice]) => siteRow(origin, choice)));
  }
}

function siteRow(origin: string, choice: SiteChoice) {
  const li = document.createElement("li");
  const name = document.createElement("span");
  name.className = "site";
  name.textContent = origin.replace(/^https:\/\//, "");
  name.title = origin;
  const tag = document.createElement("span");
  tag.className = `tag ${choice}`;
  tag.textContent = choice === "allow" ? "Allowed" : "Blocked";
  const remove = document.createElement("button");
  remove.type = "button";
  remove.title = `Forget ${origin} (Oxide asks again)`;
  remove.setAttribute("aria-label", remove.title);
  remove.textContent = "×";
  remove.addEventListener("click", async () => {
    render(await ask<PopupStatus>({ type: "setSite", origin, choice: null }));
  });
  li.append(name, tag, remove);
  return li;
}

async function refresh() {
  render(await ask<PopupStatus>({ type: "status" }));
}

$("pair-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const button = $<HTMLButtonElement>("pair-button");
  const code = $<HTMLInputElement>("code").value;
  const port = Number($<HTMLInputElement>("port").value) || DEFAULT_PORT;
  button.disabled = true;
  button.textContent = "Pairing…";
  try {
    const r = await ask<PairAnswer>({ type: "pair", code, port });
    if (r.ok) {
      showPairing = false;
      $<HTMLInputElement>("code").value = "";
    }
    const s = await ask<PopupStatus>({ type: "status" });
    if (!r.ok) s.error = r.error ?? "Pairing failed.";
    render(s);
  } finally {
    button.disabled = false;
    button.textContent = "Pair";
  }
});

$("unpair").addEventListener("click", async () => {
  showPairing = true;
  render(await ask<PopupStatus>({ type: "unpair" }));
});

$("reconnect").addEventListener("click", async () => {
  render(await ask<PopupStatus>({ type: "reconnect" }));
  setTimeout(refresh, 800);
});

$("stop-all").addEventListener("click", async () => {
  render(await ask<PopupStatus>({ type: "stopAll" }));
});

$("site-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const input = $<HTMLInputElement>("site");
  const choice = $<HTMLSelectElement>("site-choice").value as SiteChoice;
  const r = await ask<PopupStatus | { error: string }>({
    type: "setSite",
    origin: input.value,
    choice,
  });
  if ("error" in r) {
    const error = $("error");
    error.hidden = false;
    error.textContent = r.error;
    return;
  }
  input.value = "";
  render(r);
});

void refresh();
setInterval(refresh, 1500);
