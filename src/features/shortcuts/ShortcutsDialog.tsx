import { X } from "lucide-react";
import { Dialog } from "radix-ui";
import { COMPOSER_HINTS, SHORTCUTS, shortcutKeys } from "@/app/shortcuts";
import { IconButton } from "@/components/ui/IconButton";
import { Kbd } from "@/components/ui/Kbd";
import { useUi } from "@/stores/ui";

export function ShortcutsDialog() {
  const open = useUi((s) => s.shortcutsOpen);
  const setOpen = useUi((s) => s.setShortcutsOpen);
  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="ox-pop fixed left-1/2 top-1/2 z-50 w-[min(480px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-line bg-raised p-6 shadow-lg outline-none"
        >
          <div className="mb-4 flex items-center justify-between">
            <Dialog.Title className="font-serif text-xl font-normal text-fg">
              Keyboard shortcuts
            </Dialog.Title>
            <Dialog.Close asChild>
              <IconButton label="Close">
                <X className="size-[18px]" />
              </IconButton>
            </Dialog.Close>
          </div>
          <ul className="flex flex-col">
            {SHORTCUTS.map((s) => (
              <li
                key={s.id}
                className="flex h-10 items-center justify-between border-b border-line text-md"
              >
                <span className="text-fg">{s.label}</span>
                <Kbd keys={shortcutKeys(s.id)} />
              </li>
            ))}
            {COMPOSER_HINTS.map((h) => (
              <li
                key={h.label}
                className="flex h-10 items-center justify-between border-b border-line text-md last:border-0"
              >
                <span className="text-fg">{h.label}</span>
                <Kbd keys={h.keys} />
              </li>
            ))}
          </ul>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
