import type {
  CommandError,
  DataExport,
  DataLocations,
  Result,
  UpdateStatus,
  commands as RealCommands,
} from "@/bindings";

/**
 * Settings → Data and updates in the fake backend. "Export all" answers with a made-up
 * zip unless a test cancels the dialog (`setFakeExportCancelled`); "Delete all" records the
 * call. The updater starts configured with nothing checked; `setFakeUpdate()` puts a newer
 * version on the fake server, and installing walks through downloading to installing
 * (sending `updateChanged` like the app does).
 */
interface FakeData {
  locations: DataLocations;
  exports: number;
  cancelExport: boolean;
  reveals: number;
  opens: number;
  deletes: string[];
  update: UpdateStatus;
  server: { version: string; notes: string | null } | null;
  failCheck: string | null;
  installs: number;
}

let d: FakeData;
let emit: (s: UpdateStatus) => void = () => {};

export function setUpdateEmitter(f: (s: UpdateStatus) => void): void {
  emit = f;
}

export function resetFakeData(): void {
  d = {
    locations: {
      data: "C:\\Users\\me\\AppData\\Roaming\\dev.oxide.app",
      config: "C:\\Users\\me\\AppData\\Roaming\\dev.oxide.app",
      logs: "C:\\Users\\me\\AppData\\Local\\dev.oxide.app\\logs",
      dataBytes: 52_428_800,
    },
    exports: 0,
    cancelExport: false,
    reveals: 0,
    opens: 0,
    deletes: [],
    update: {
      currentVersion: "0.1.0",
      configured: true,
      canInstall: true,
      phase: "idle",
      available: null,
      downloaded: 0,
      total: null,
      error: null,
      checkedAt: null,
      note: null,
    },
    server: null,
    failCheck: null,
    installs: 0,
  };
}

export function fakeDataState(): Readonly<FakeData> {
  return d;
}

export function setFakeExportCancelled(cancelled: boolean): void {
  d.cancelExport = cancelled;
}

/** Puts a version on the fake update server (`null`: none newer), or makes checks fail. */
export function setFakeUpdate(
  server: { version: string; notes?: string | null } | null,
  failCheck: string | null = null,
): void {
  d.server = server && { version: server.version, notes: server.notes ?? null };
  d.failCheck = failCheck;
}

/** Changes what the updater reports (e.g. not configured on this build). */
export function setFakeUpdateStatus(patch: Partial<UpdateStatus>): void {
  d.update = { ...d.update, ...patch };
}

function setUpdate(patch: Partial<UpdateStatus>): UpdateStatus {
  d.update = { ...d.update, ...patch };
  emit(d.update);
  return d.update;
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data });
const fail = (message: string): Promise<Result<never, CommandError>> =>
  Promise.resolve({ status: "error", error: { kind: "invalid", message } });

export const fakeDataCommands: Pick<
  typeof RealCommands,
  | "dataLocations"
  | "openDataFolder"
  | "exportAllData"
  | "revealDataExport"
  | "deleteAllData"
  | "updateStatus"
  | "checkForUpdate"
  | "installUpdate"
> = {
  dataLocations: () => ok(d.locations),
  openDataFolder: () => {
    d.opens += 1;
    return ok(null);
  },
  exportAllData: () => {
    if (d.cancelExport) return ok(null);
    d.exports += 1;
    const done: DataExport = {
      path: "C:\\Users\\me\\Downloads\\oxide-export-2026-11-13.zip",
      tables: 24,
      rows: 1312,
      files: 37,
      bytes: 8_912_896,
      skipped: [],
    };
    return ok(done);
  },
  revealDataExport: () => {
    d.reveals += 1;
    return ok(null);
  },
  deleteAllData: (confirm) => {
    if (confirm.trim().toLowerCase() !== "delete") return fail("Type “delete” to confirm.");
    d.deletes.push(confirm);
    return ok({ secrets: 6, leftovers: [] });
  },
  updateStatus: () => Promise.resolve(d.update),
  checkForUpdate: () => {
    if (!d.update.configured) return ok(d.update);
    const checkedAt = Date.now();
    if (d.failCheck) {
      return ok(setUpdate({ phase: "failed", error: d.failCheck, checkedAt }));
    }
    if (!d.server) {
      return ok(setUpdate({ phase: "upToDate", available: null, error: null, checkedAt }));
    }
    return ok(
      setUpdate({
        phase: "available",
        available: { version: d.server.version, date: null, notes: d.server.notes },
        error: null,
        checkedAt,
      }),
    );
  },
  installUpdate: () => {
    if (d.update.phase !== "available") return fail("Check for updates first.");
    d.installs += 1;
    setUpdate({ phase: "downloading", downloaded: 0, total: 4_000_000 });
    setUpdate({ downloaded: 2_000_000 });
    setUpdate({ phase: "installing", downloaded: 4_000_000 });
    return ok(null);
  },
};
