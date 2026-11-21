import type { AutoMark, PermissionMode } from "@/bindings";

/** The permission modes' names. */
export const MODE_LABELS: Record<PermissionMode, string> = {
  manual: "Manual",
  auto: "Auto",
};

/** One line on what each mode does. */
export const MODE_HINTS: Record<PermissionMode, string> = {
  manual: "Oxide asks before tools that change things.",
  auto: "Oxide works on its own and stops to ask when an action looks risky.",
};

/** The short "why" after **Auto** on a tool line. */
export const AUTO_MARK_LABELS: Record<AutoMark, string> = {
  readOnly: "read-only",
  inFolder: "in your folder",
  sandboxed: "in the sandbox",
  allowed: "allowed",
  checked: "checked",
  stopped: "stopped",
};

/** What each Auto mark means (its tooltip). */
export const AUTO_MARK_HINTS: Record<AutoMark, string> = {
  readOnly: "Ran on its own in Auto: it only reads.",
  inFolder: "Ran on its own in Auto: a change inside a folder you shared with this chat.",
  sandboxed: "Ran on its own in Auto: inside the chat’s sandbox, which has no network.",
  allowed: "Ran on its own in Auto: you allowed this tool for the chat.",
  checked: "Ran on its own in Auto after the safety check looked at it.",
  stopped: "Auto’s safety check stopped this to ask you.",
};
