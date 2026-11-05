/**
 * `localStorage` if it works, otherwise an in-memory stand-in. Storage can be missing or throw
 * (private contexts, Node 25+ test runs where Node's own stub shadows jsdom's), and UI
 * preferences must never take the app down.
 */
export function createMemoryStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null,
    removeItem: (k) => {
      map.delete(k);
    },
    setItem: (k, v) => {
      map.set(k, String(v));
    },
  };
}

export function isUsableStorage(s: Storage | undefined | null): s is Storage {
  try {
    if (!s || typeof s.setItem !== "function") return false;
    s.setItem("__oxide_probe__", "1");
    s.removeItem("__oxide_probe__");
    return true;
  } catch {
    return false;
  }
}

let fallback: Storage | null = null;

export function safeLocalStorage(): Storage {
  let candidate: Storage | undefined;
  try {
    candidate = globalThis.localStorage;
  } catch {
    candidate = undefined;
  }
  if (isUsableStorage(candidate)) return candidate;
  fallback ??= createMemoryStorage();
  return fallback;
}
