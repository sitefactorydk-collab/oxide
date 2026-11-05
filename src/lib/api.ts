import { QueryClient } from "@tanstack/react-query";
import type { CommandError, Result } from "@/bindings";

/** A failed backend command. `kind` says what went wrong; `message` is safe to show. */
export class ApiError extends Error {
  readonly kind: CommandError["kind"];

  constructor(error: CommandError) {
    super(error.message);
    this.name = "ApiError";
    this.kind = error.kind;
  }
}

/** Unwraps a tauri-specta `Result`, throwing `ApiError` on failure (TanStack Query style). */
export async function unwrap<T>(pending: Promise<Result<T, CommandError>>): Promise<T> {
  const res = await pending;
  if (res.status === "error") throw new ApiError(res.error);
  return res.data;
}

/** Human-readable message for any thrown value. */
export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  return "Something went wrong.";
}

/**
 * Query client for backend data. IPC is local and cheap, so data is considered fresh until a
 * mutation invalidates it, and failed reads aren't retried (they won't succeed on retry).
 */
export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { staleTime: Infinity, retry: false, refetchOnWindowFocus: false },
      mutations: { retry: false },
    },
  });
}
