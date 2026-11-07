import { useQuery } from "@tanstack/react-query";
import { commands } from "@/bindings";
import { desktopKeys } from "./keys";

/** Whether the shortcut is registered, start at login as the OS has it, and so on. */
export function useDesktopStatus() {
  return useQuery({
    queryKey: desktopKeys.status,
    queryFn: () => commands.desktopStatus(),
  });
}
