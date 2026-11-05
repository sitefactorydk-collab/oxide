import { create } from "zustand";

export interface Toast {
  id: number;
  text: string;
  tone: "info" | "error";
}

interface ToastState {
  toasts: Toast[];
  dismiss: (id: number) => void;
}

let seq = 0;

export const useToasts = create<ToastState>()((set) => ({
  toasts: [],
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

/** Shows a short message at the bottom of the window for a few seconds. */
export function toast(text: string, tone: Toast["tone"] = "info"): void {
  seq += 1;
  const id = seq;
  useToasts.setState((s) => ({ toasts: [...s.toasts.slice(-2), { id, text, tone }] }));
  setTimeout(() => useToasts.getState().dismiss(id), tone === "error" ? 8000 : 4000);
}
