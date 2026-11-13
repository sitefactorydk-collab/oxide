import type {
  CommandError,
  ComputerActivity,
  ComputerChanged,
  ComputerStatus,
  Result,
  commands as RealCommands,
} from "@/bindings";

/**
 * Computer use in the fake backend: two screens, and chats a test sets acting or waiting
 * with `setFakeComputerActivity()` (which also sends `computerChanged`, as the app's sink
 * does). `computerStopAll` ends them all and records it.
 */
interface FakeComputer {
  status: ComputerStatus;
  chats: ComputerActivity[];
  stops: number;
}

let co: FakeComputer;
let emit: (c: ComputerChanged) => void = () => {};

export function setComputerEmitter(f: (c: ComputerChanged) => void): void {
  emit = f;
}

export function resetFakeComputer(): void {
  co = {
    status: {
      supported: true,
      displays: [
        {
          id: "\\\\.\\DISPLAY1",
          name: "Dell U2720Q",
          x: 0,
          y: 0,
          width: 2560,
          height: 1440,
          scaleFactor: 1.5,
          primary: true,
        },
        {
          id: "\\\\.\\DISPLAY2",
          name: "LG HDR",
          x: 2560,
          y: 0,
          width: 1920,
          height: 1080,
          scaleFactor: 1,
          primary: false,
        },
      ],
      error: null,
      active: [],
      panicShortcut: "Ctrl+Alt+Esc",
      panicError: null,
    },
    chats: [],
    stops: 0,
  };
}

export function fakeComputerState(): Readonly<FakeComputer> {
  return co;
}

/** Changes what the fake computer reports (e.g. `supported: false`, a panic error). */
export function setFakeComputerStatus(patch: Partial<ComputerStatus>): void {
  co.status = { ...co.status, ...patch };
}

/** A chat acts, waits or ends, as `ComputerSink` reports it. */
export function setFakeComputerActivity(changed: ComputerActivity): void {
  const others = co.chats.filter((c) => c.conversationId !== changed.conversationId);
  co.chats = changed.state === "ended" ? others : [...others, changed];
  co.status = { ...co.status, active: co.chats.map((c) => c.conversationId) };
  emit({ chats: co.chats, changed });
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data });

export const fakeComputerCommands: Pick<
  typeof RealCommands,
  "computerStatus" | "computerActivity" | "computerStopAll"
> = {
  computerStatus: () => ok(co.status),
  computerActivity: () => ok(co.chats),
  computerStopAll: () => {
    co.stops += 1;
    const stopped = co.chats.map((c) => c.conversationId);
    for (const id of stopped) {
      setFakeComputerActivity({ conversationId: id, state: "ended", action: null });
    }
    return ok(stopped);
  },
};
