/** Query keys for desktop integration (kept import-free so settings can use them). */
export const desktopKeys = {
  status: ["desktop", "status"] as const,
};

/** Location state of `/new` when something outside the window (a deep link) fills the composer. */
export interface NewChatState {
  draft?: string;
}
