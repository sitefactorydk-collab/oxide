import { Dialog } from "radix-ui";
import { X } from "lucide-react";
import { useEffect, useState } from "react";
import type { AppDraft } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { AppIcon } from "@/features/apps/AppIcon";
import { APP_ICONS } from "@/features/apps/appIcons";
import { useApps } from "@/features/apps/queries";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { useUpdateDetails } from "./queries";

const field =
  "h-9 w-full rounded-lg border border-line bg-raised px-3 text-base text-fg outline-none focus:border-accent";

/** The app's name, description, icon and category (saved into the draft's manifest). */
export function DetailsDialog({
  draft,
  open,
  onOpenChange,
}: {
  draft: AppDraft;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const m = draft.app.manifest;
  const update = useUpdateDetails(m.id);
  const { data: apps } = useApps();
  const [name, setName] = useState(m.name);
  const [description, setDescription] = useState(m.description ?? "");
  const [icon, setIcon] = useState(m.icon ?? "sparkles");
  const [category, setCategory] = useState(m.category ?? "Other");
  useEffect(() => {
    if (!open) return;
    setName(m.name);
    setDescription(m.description ?? "");
    setIcon(m.icon ?? "sparkles");
    setCategory(m.category ?? "Other");
  }, [open, m.name, m.description, m.icon, m.category]);
  const categories = [...new Set((apps ?? []).map((a) => a.manifest.category ?? "Other"))].sort();

  const save = () =>
    update.mutate(
      {
        name: name.trim(),
        description: description.trim(),
        icon: icon.trim(),
        category: category.trim(),
      },
      {
        onSuccess: () => onOpenChange(false),
        onError: (e) => toast(errorMessage(e), "error"),
      },
    );

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="ox-pop fixed left-1/2 top-1/2 z-50 w-[min(560px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-line bg-app p-6 shadow-lg outline-none"
        >
          <div className="mb-4 flex items-center justify-between">
            <Dialog.Title className="font-serif text-xl font-normal text-fg">
              App details
            </Dialog.Title>
            <Dialog.Close asChild>
              <IconButton tooltip={false} size="sm" label="Close">
                <X className="size-4" />
              </IconButton>
            </Dialog.Close>
          </div>
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              save();
            }}
          >
            <label className="flex flex-col gap-1.5 text-sm text-fg-muted">
              Name
              <input className={field} value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            <label className="flex flex-col gap-1.5 text-sm text-fg-muted">
              Description
              <textarea
                className={cn(field, "h-20 resize-none py-2")}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
              />
            </label>
            <label className="flex flex-col gap-1.5 text-sm text-fg-muted">
              Category
              <input
                className={field}
                list="app-categories"
                value={category}
                onChange={(e) => setCategory(e.target.value)}
              />
              <datalist id="app-categories">
                {categories.map((c) => (
                  <option key={c} value={c} />
                ))}
              </datalist>
            </label>
            <fieldset className="flex flex-col gap-1.5">
              <legend className="mb-1.5 text-sm text-fg-muted">Icon (or type an emoji)</legend>
              <div className="flex items-center gap-2">
                <AppIcon icon={icon || "sparkles"} className="size-9" />
                <input
                  className={field}
                  aria-label="Icon"
                  value={icon}
                  onChange={(e) => setIcon(e.target.value)}
                />
              </div>
              <div
                role="listbox"
                aria-label="Icons"
                className="grid max-h-36 grid-cols-10 gap-1 overflow-y-auto"
              >
                {Object.keys(APP_ICONS).map((name) => (
                  <button
                    key={name}
                    type="button"
                    role="option"
                    aria-selected={icon === name}
                    title={name}
                    aria-label={name}
                    onClick={() => setIcon(name)}
                    className={cn(
                      "flex items-center justify-center rounded-lg p-0.5",
                      icon === name ? "ring-2 ring-accent" : "hover:bg-hover",
                    )}
                  >
                    <AppIcon icon={name} className="size-8" />
                  </button>
                ))}
              </div>
            </fieldset>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => onOpenChange(false)}
                className="h-9 rounded-lg border border-line px-3.5 text-base text-fg hover:bg-hover"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={update.isPending || !name.trim()}
                className="h-9 rounded-lg bg-fg px-3.5 text-base font-medium text-app hover:opacity-90 disabled:opacity-40"
              >
                Save details
              </button>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
