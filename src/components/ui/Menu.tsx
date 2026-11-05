import { Check, ChevronRight } from "lucide-react";
import { DropdownMenu as M } from "radix-ui";
import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/cn";

/** Styled Radix dropdown pieces shared by every menu in the app. */

export const MenuRoot = M.Root;
export const MenuTrigger = M.Trigger;
export const MenuSub = M.Sub;
export const MenuRadioGroup = M.RadioGroup;

const panel =
  "ox-pop z-50 min-w-[220px] rounded-xl border border-line bg-raised p-1 text-base text-fg shadow-lg outline-none";
const item =
  "relative flex h-8 cursor-default items-center gap-2.5 rounded-lg px-2.5 outline-none select-none " +
  "data-[highlighted]:bg-hover data-[disabled]:pointer-events-none data-[disabled]:opacity-45 " +
  "[&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-fg-muted";

export function MenuContent({
  className,
  sideOffset = 6,
  ...props
}: ComponentProps<typeof M.Content>) {
  return (
    <M.Portal>
      <M.Content sideOffset={sideOffset} className={cn(panel, className)} {...props} />
    </M.Portal>
  );
}

export function MenuItem({
  className,
  danger,
  shortcut,
  children,
  ...props
}: ComponentProps<typeof M.Item> & { danger?: boolean; shortcut?: string }) {
  return (
    <M.Item className={cn(item, danger && "text-danger [&_svg]:text-danger", className)} {...props}>
      {children}
      {shortcut && <span className="ml-auto pl-4 text-xs text-fg-subtle">{shortcut}</span>}
    </M.Item>
  );
}

export function MenuCheckboxItem({
  className,
  children,
  ...props
}: ComponentProps<typeof M.CheckboxItem>) {
  return (
    <M.CheckboxItem className={cn(item, "pr-9", className)} {...props}>
      {children}
      <M.ItemIndicator className="absolute right-2.5 flex">
        <Check className="!text-accent" />
      </M.ItemIndicator>
    </M.CheckboxItem>
  );
}

export function MenuRadioItem({
  className,
  children,
  ...props
}: ComponentProps<typeof M.RadioItem>) {
  return (
    <M.RadioItem className={cn(item, "pr-9", className)} {...props}>
      {children}
      <M.ItemIndicator className="absolute right-2.5 flex">
        <Check className="!text-accent" />
      </M.ItemIndicator>
    </M.RadioItem>
  );
}

export function MenuLabel({ className, ...props }: ComponentProps<typeof M.Label>) {
  return (
    <M.Label
      className={cn("px-2.5 pb-1 pt-2 text-xs font-medium text-fg-subtle", className)}
      {...props}
    />
  );
}

export function MenuSeparator({ className, ...props }: ComponentProps<typeof M.Separator>) {
  return <M.Separator className={cn("mx-1 my-1 h-px bg-line", className)} {...props} />;
}

export function MenuSubTrigger({
  className,
  children,
  ...props
}: ComponentProps<typeof M.SubTrigger> & { children: ReactNode }) {
  return (
    <M.SubTrigger className={cn(item, "data-[state=open]:bg-hover", className)} {...props}>
      {children}
      <ChevronRight className="ml-auto" />
    </M.SubTrigger>
  );
}

export function MenuSubContent({ className, ...props }: ComponentProps<typeof M.SubContent>) {
  return (
    <M.Portal>
      <M.SubContent sideOffset={6} className={cn(panel, className)} {...props} />
    </M.Portal>
  );
}
