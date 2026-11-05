import { Tooltip as T } from "radix-ui";
import type { ReactNode } from "react";

export function Tooltip({
  label,
  shortcut,
  side = "bottom",
  children,
}: {
  label: ReactNode;
  shortcut?: string;
  side?: "top" | "bottom" | "left" | "right";
  children: ReactNode;
}) {
  return (
    <T.Root>
      <T.Trigger asChild>{children}</T.Trigger>
      <T.Portal>
        <T.Content
          side={side}
          sideOffset={6}
          className="ox-pop z-50 flex items-center gap-2 rounded-md bg-tooltip px-2 py-1 text-xs font-medium text-tooltip-fg shadow-md"
        >
          {label}
          {shortcut && <span className="opacity-60">{shortcut}</span>}
        </T.Content>
      </T.Portal>
    </T.Root>
  );
}
