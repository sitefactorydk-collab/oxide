import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import { cn } from "@/lib/cn";
import { Tooltip } from "./Tooltip";

type Props = ButtonHTMLAttributes<HTMLButtonElement> & {
  /** Accessible name; also shown as the tooltip. */
  label: string;
  shortcut?: string;
  active?: boolean;
  size?: "sm" | "md";
  tooltipSide?: "top" | "bottom" | "left" | "right";
  /** Set false when a parent already wraps this in a tooltip or menu trigger. */
  tooltip?: boolean;
  children: ReactNode;
};

export const IconButton = forwardRef<HTMLButtonElement, Props>(function IconButton(
  {
    label,
    shortcut,
    active,
    size = "md",
    tooltipSide,
    tooltip = true,
    className,
    children,
    type = "button",
    ...rest
  },
  ref,
) {
  const button = (
    <button
      ref={ref}
      type={type}
      aria-label={label}
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-md text-fg-muted transition-colors",
        "hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40",
        size === "md" ? "h-8 w-8" : "h-7 w-7",
        active && "bg-active text-fg",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
  return tooltip ? (
    <Tooltip label={label} shortcut={shortcut} side={tooltipSide}>
      {button}
    </Tooltip>
  ) : (
    button
  );
});
