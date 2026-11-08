import type {
  CommandError,
  PythonStatus,
  PythonStep,
  Result,
  commands as RealCommands,
} from "@/bindings";

/**
 * The managed Python environment in the fake backend. Setup and repair stop at the first step
 * ("installing") until a test calls `finishFakePython()` or `failFakePython()`, so the progress
 * UI can be checked mid-way.
 */
const PACKAGES = [
  { name: "pandas", version: "2.2.3" },
  { name: "numpy", version: "2.1.3" },
  { name: "matplotlib", version: "3.9.2" },
  { name: "openpyxl", version: "3.1.5" },
];

interface FakePython {
  status: PythonStatus;
  setups: number;
  repairs: number;
  resets: number;
  folderOpened: number;
}

let py: FakePython;
let emit: (s: PythonStatus) => void = () => {};

export function setPythonEmitter(fn: (s: PythonStatus) => void): void {
  emit = fn;
}

function missing(): PythonStatus {
  return {
    state: "missing",
    step: null,
    message: null,
    error: null,
    log: [],
    uvPath: "C:/Program Files/Oxide/uv.exe",
    uvVersion: "0.8.0",
    pythonVersion: null,
    packages: [],
    folder: "C:/Users/me/AppData/Roaming/Oxide/python",
    diskBytes: null,
  };
}

export function resetFakePython(): void {
  py = { status: missing(), setups: 0, repairs: 0, resets: 0, folderOpened: 0 };
}
resetFakePython();

export function fakePythonState(): Readonly<FakePython> {
  return py;
}

function set(patch: Partial<PythonStatus>): void {
  py.status = { ...py.status, ...patch };
  emit(py.status);
}

/**
 * The environment is installed (as if setup had finished earlier) and the event is sent.
 * Call it after `renderApp`, which resets the fake backend.
 */
export function fakePythonReady(): void {
  set({
    ...missing(),
    state: "ready",
    pythonVersion: "3.12.8",
    packages: PACKAGES,
    diskBytes: 412 * 1024 * 1024,
  });
}

/** Moves a running setup to `step` and sends the event. */
export function fakePythonStep(step: PythonStep, message: string, line?: string): void {
  set({
    state: "installing",
    step,
    message,
    log: line ? [...py.status.log, line] : py.status.log,
  });
}

/** Setup finishes. */
export function finishFakePython(): void {
  set({
    state: "ready",
    step: null,
    message: null,
    error: null,
    pythonVersion: "3.12.8",
    packages: PACKAGES,
    diskBytes: 412 * 1024 * 1024,
  });
}

/** Setup fails with `error`. */
export function failFakePython(error: string, step: PythonStep = "packages"): void {
  set({ state: "failed", step, message: null, error });
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data: JSON.parse(JSON.stringify(data)) as T });

function begin(): Promise<Result<null, CommandError>> {
  set({
    state: "installing",
    step: "uv",
    message: "Looking for uv",
    error: null,
    log: [],
    pythonVersion: null,
    packages: [],
  });
  return ok(null);
}

export const fakePythonCommands: Pick<
  typeof RealCommands,
  "pythonStatus" | "pythonSetup" | "pythonRepair" | "pythonReset" | "openPythonFolder"
> = {
  pythonStatus: () => ok(py.status),
  pythonSetup: () => {
    py.setups += 1;
    return begin();
  },
  pythonRepair: () => {
    py.repairs += 1;
    return begin();
  },
  pythonReset: () => {
    py.resets += 1;
    py.status = missing();
    emit(py.status);
    return ok(null);
  },
  openPythonFolder: () => {
    py.folderOpened += 1;
    return ok(null);
  },
};
