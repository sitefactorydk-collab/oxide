// The parts of the Chrome extension API Oxide's extension uses, typed by hand (kept small so
// the extension builds with TypeScript alone). See https://developer.chrome.com/docs/extensions/reference/api.

declare namespace chrome {
  namespace runtime {
    interface MessageSender {
      tab?: tabs.Tab;
      frameId?: number;
      id?: string;
      url?: string;
    }
    const id: string;
    const lastError: { message?: string } | undefined;
    function getManifest(): { version: string; name: string };
    function getURL(path: string): string;
    function sendMessage<T = unknown>(message: unknown): Promise<T>;
    const onMessage: {
      addListener(
        cb: (
          message: unknown,
          sender: MessageSender,
          sendResponse: (response?: unknown) => void,
        ) => boolean | void,
      ): void;
    };
    const onStartup: { addListener(cb: () => void): void };
    const onInstalled: { addListener(cb: () => void): void };
  }

  namespace storage {
    interface Area {
      get(keys?: string | string[] | null): Promise<Record<string, unknown>>;
      set(items: Record<string, unknown>): Promise<void>;
      remove(keys: string | string[]): Promise<void>;
    }
    const local: Area;
    const session: Area;
    const onChanged: {
      addListener(
        cb: (changes: Record<string, { newValue?: unknown }>, area: string) => void,
      ): void;
    };
  }

  namespace tabs {
    interface Tab {
      id?: number;
      windowId: number;
      index: number;
      url?: string;
      pendingUrl?: string;
      title?: string;
      active: boolean;
      status?: "loading" | "complete" | "unloaded";
      groupId: number;
      openerTabId?: number;
    }
    interface ChangeInfo {
      status?: string;
      url?: string;
    }
    function get(tabId: number): Promise<Tab>;
    function query(q: {
      active?: boolean;
      currentWindow?: boolean;
      lastFocusedWindow?: boolean;
      windowId?: number;
    }): Promise<Tab[]>;
    function create(p: { url?: string; active?: boolean; windowId?: number }): Promise<Tab>;
    function update(tabId: number, p: { url?: string; active?: boolean }): Promise<Tab>;
    function remove(tabId: number | number[]): Promise<void>;
    function goBack(tabId: number): Promise<void>;
    function goForward(tabId: number): Promise<void>;
    function reload(tabId: number): Promise<void>;
    function group(p: { tabIds: number | number[]; groupId?: number }): Promise<number>;
    function captureVisibleTab(
      windowId: number,
      options: { format: "jpeg" | "png"; quality?: number },
    ): Promise<string>;
    const onUpdated: {
      addListener(cb: (tabId: number, info: ChangeInfo, tab: Tab) => void): void;
      removeListener(cb: (tabId: number, info: ChangeInfo, tab: Tab) => void): void;
    };
    const onRemoved: { addListener(cb: (tabId: number) => void): void };
    const onCreated: { addListener(cb: (tab: Tab) => void): void };
  }

  namespace tabGroups {
    function update(
      groupId: number,
      p: { title?: string; color?: string; collapsed?: boolean },
    ): Promise<unknown>;
  }

  namespace windows {
    function update(windowId: number, p: { focused?: boolean }): Promise<unknown>;
  }

  namespace scripting {
    interface InjectionResult<T> {
      result?: T;
      frameId: number;
    }
    function executeScript<T = unknown>(p: {
      target: { tabId: number; frameIds?: number[]; allFrames?: boolean };
      files?: string[];
      func?: (...args: never[]) => unknown;
      args?: unknown[];
      world?: "ISOLATED" | "MAIN";
      injectImmediately?: boolean;
    }): Promise<InjectionResult<T>[]>;
  }

  namespace scripting {
    interface RegisteredContentScript {
      id: string;
      js?: string[];
      matches?: string[];
      runAt?: "document_start" | "document_end" | "document_idle";
      world?: "ISOLATED" | "MAIN";
      persistAcrossSessions?: boolean;
      allFrames?: boolean;
    }
    function registerContentScripts(scripts: RegisteredContentScript[]): Promise<void>;
    function unregisterContentScripts(filter?: { ids?: string[] }): Promise<void>;
  }

  namespace alarms {
    function create(
      name: string,
      info: { periodInMinutes?: number; delayInMinutes?: number },
    ): void;
    const onAlarm: { addListener(cb: (alarm: { name: string }) => void): void };
  }

  namespace action {
    function setBadgeText(p: { text: string }): Promise<void>;
    function setBadgeBackgroundColor(p: { color: string }): Promise<void>;
    function setTitle(p: { title: string }): Promise<void>;
  }
}
