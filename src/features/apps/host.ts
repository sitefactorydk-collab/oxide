/**
 * The main window's side of a Utility App's SDK (Phase 20): answers the frame's calls.
 *
 * Most calls go to the backend (`app_call`), which checks the app's manifest and grants.
 * When a capability isn't allowed yet the backend says `needsPermission`; this asks the user
 * once (concurrent calls share the question), grants it on “Allow” and calls again. “Deny”
 * holds for the rest of this visit (the next visit asks again). Saving files, picking the
 * app's folder and copying are done here; generated media come back as paths and are read into Blobs for the frame.
 */
import { commands, type AppCallError, type AppCapability } from "@/bindings";
import { unwrap } from "@/lib/api";
import type { SdkError } from "@/appframe/protocol";

/** Asks the user about a capability; resolves to whether they allowed it. */
export type Ask = (capability: AppCapability, message: string) => Promise<boolean>;

export interface AppHost {
  /** Answers one call. Rejects with an {@link SdkError}. */
  call(id: string, method: string, params: unknown): Promise<unknown>;
  cancel(id: string): void;
  /** The backend id of a frame call (events carry it). */
  backendId(id: string): string;
}

const MEDIA = new Set(["media.image", "media.video", "media.music"]);

function sdkError(e: unknown): SdkError {
  if (typeof e === "object" && e !== null && "code" in e && "message" in e) {
    const x = e as AppCallError;
    return { code: x.code, message: x.message };
  }
  return { code: "failed", message: e instanceof Error ? e.message : String(e) };
}

function base64ToBlob(data: string, type: string): Blob {
  const bin = atob(data);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new Blob([bytes], { type });
}

export function createAppHost(appId: string, ask: Ask, instance: string): AppHost {
  const denied = new Set<AppCapability>();
  const asking = new Map<AppCapability, Promise<boolean>>();
  const backendId = (id: string) => `${instance}:${id}`;

  const askOnce = (cap: AppCapability, message: string): Promise<boolean> => {
    const open = asking.get(cap);
    if (open) return open;
    const p = ask(cap, message)
      .then(async (allowed) => {
        if (allowed) await unwrap(commands.grantAppCapability(appId, cap));
        else denied.add(cap);
        return allowed;
      })
      .finally(() => asking.delete(cap));
    asking.set(cap, p);
    return p;
  };

  async function media(value: {
    files?: Array<{ name: string; mediaType: string; path: string }>;
  }) {
    const files = await Promise.all(
      (value.files ?? []).map(async (f) => ({
        name: f.name,
        mediaType: f.mediaType,
        blob: base64ToBlob(await unwrap(commands.attachmentMedia(f.path)), f.mediaType),
      })),
    );
    return { ...value, files };
  }

  async function call(id: string, method: string, params: unknown): Promise<unknown> {
    if (method === "app.copy") {
      const text = (params as { text?: unknown })?.text;
      await navigator.clipboard.writeText(typeof text === "string" ? text : "");
      return null;
    }
    if (method === "folder.choose") {
      // The system's folder picker; picking also allows the app to use the folder.
      const path = await unwrap(commands.chooseAppFolder(appId));
      if (path === null) return null;
      const r = await commands.appCall(appId, backendId(id), "folder.get", "{}");
      return r.status === "ok"
        ? (JSON.parse(r.data) as unknown)
        : { path, name: path, exists: true };
    }
    if (method === "files.save") {
      const p = params as { name?: string; data?: string };
      const path = await unwrap(commands.saveAppOutput(p.name ?? "download", p.data ?? ""));
      return { path };
    }
    for (let attempt = 0; attempt < 3; attempt++) {
      const r = await commands.appCall(appId, backendId(id), method, JSON.stringify(params ?? {}));
      if (r.status === "ok") {
        const value: unknown = JSON.parse(r.data);
        return MEDIA.has(method) ? media(value as Parameters<typeof media>[0]) : value;
      }
      const e = r.error;
      if (e.code !== "needsPermission" || !e.capability) throw sdkError(e);
      if (denied.has(e.capability)) {
        throw { code: "denied", message: "You didn't allow this for the app." } satisfies SdkError;
      }
      const allowed = await askOnce(e.capability, e.message);
      if (!allowed) {
        throw { code: "denied", message: "You didn't allow this for the app." } satisfies SdkError;
      }
    }
    throw { code: "failed", message: "The permission couldn't be saved." } satisfies SdkError;
  }

  return {
    call: (id, method, params) =>
      call(id, method, params).catch((e) => {
        throw sdkError(e);
      }),
    cancel: (id) => void commands.cancelAppCall(backendId(id)),
    backendId,
  };
}
