import { FileArchive, FolderOpen, FolderPlus, Pencil, Plus, Sparkles, Trash2 } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { commands, type Skill, type SkillInput } from "@/bindings";
import { ConfirmDialog, Modal } from "@/features/projects/ProjectDialogs";
import { Button, inputClass, Switch, textareaClass } from "@/features/mcp/ui";
import { errorMessage } from "@/lib/api";
import { toast } from "@/lib/toast";
import {
  pickPaths,
  useAddSkill,
  useCreateSkill,
  useRemoveSkill,
  useSetSkillEnabled,
  useSkill,
  useSkills,
  useUpdateSkill,
} from "./queries";
import { useSkillFocus } from "./store";
import { skillSlug } from "@/features/tools/toolView";

const MAX_DESCRIPTION = 1024;

/** Settings → Skills: write, add, switch off and remove skills. */
export function SkillsSection() {
  const { data: skills, isPending } = useSkills();
  const add = useAddSkill();
  const [creating, setCreating] = useState(false);
  // A skill a chat just saved ("Open … in Settings → Skills").
  const focusWanted = useSkillFocus((s) => s.folder);
  const clearFocus = useSkillFocus((s) => s.clear);
  useEffect(() => clearFocus, [clearFocus]);
  const focused =
    focusWanted === null
      ? null
      : (skills.find(
          (s) =>
            s.folder === focusWanted ||
            s.name.toLowerCase() === focusWanted.toLowerCase() ||
            skillSlug(s.name) === skillSlug(focusWanted),
        ) ?? null);

  const addFrom = async (kind: "folder" | "skillArchive") => {
    try {
      const [p] = await pickPaths(kind);
      if (!p) return;
      const s = await add.mutateAsync(p);
      toast(`Added the skill “${s.name}”.`);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  return (
    <div className="flex max-w-3xl flex-col gap-3">
      <div className="mb-2 flex flex-wrap gap-2">
        <Button primary onClick={() => setCreating(true)}>
          <Plus />
          New skill
        </Button>
        <Button onClick={() => void addFrom("folder")} disabled={add.isPending}>
          <FolderPlus />
          Add folder…
        </Button>
        <Button onClick={() => void addFrom("skillArchive")} disabled={add.isPending}>
          <FileArchive />
          Add .zip or .skill…
        </Button>
        <Button onClick={() => void commands.openSkillsFolder(null)}>
          <FolderOpen />
          Open skills folder
        </Button>
      </div>
      <p className="-mt-1 mb-2 text-sm text-fg-subtle">
        A skill is a folder with a SKILL.md: a name and a description, then instructions. Models see
        the name and description of every skill that’s on, and read the rest when a task calls for
        it.
      </p>

      {isPending ? (
        <p className="text-fg-muted">Loading…</p>
      ) : skills.length === 0 ? (
        <div className="rounded-xl border border-dashed border-line-strong p-6 text-center text-fg-muted">
          <Sparkles aria-hidden className="mx-auto mb-2 size-6 text-fg-subtle" />
          <p>No skills yet.</p>
          <p className="mt-1 text-sm text-fg-subtle">Write one, or add a skill folder or zip.</p>
        </div>
      ) : (
        <ul aria-label="Skills" className="flex flex-col gap-2">
          {skills.map((s) => (
            <SkillRow key={s.folder} skill={s} />
          ))}
        </ul>
      )}

      {creating && <SkillDialog folder={null} onClose={() => setCreating(false)} />}
      {focused && <SkillDialog folder={focused.folder} onClose={clearFocus} />}
    </div>
  );
}

function SkillRow({ skill }: { skill: Skill }) {
  const setEnabled = useSetSkillEnabled();
  const remove = useRemoveSkill();
  const [editing, setEditing] = useState(false);
  const [confirm, setConfirm] = useState(false);
  return (
    <li className="rounded-xl border border-line bg-raised p-4 shadow-sm">
      <div className="flex items-start gap-3">
        <Sparkles aria-hidden className="mt-0.5 size-4 shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-baseline gap-x-2">
            <span className="font-mono text-md text-fg">{skill.name}</span>
            {skill.files > 0 && (
              <span className="text-sm text-fg-subtle">
                + {skill.files} file{skill.files === 1 ? "" : "s"}
              </span>
            )}
          </div>
          {skill.description && (
            <p className="mt-0.5 line-clamp-3 text-base text-fg-muted">{skill.description}</p>
          )}
          {skill.problem && <p className="mt-1 text-sm text-danger">{skill.problem}</p>}
        </div>
        <Switch
          label={`Turn ${skill.name} ${skill.enabled ? "off" : "on"}`}
          checked={skill.enabled}
          disabled={setEnabled.isPending}
          onChange={(enabled) =>
            setEnabled
              .mutateAsync({ folder: skill.folder, enabled })
              .catch((e: unknown) => toast(errorMessage(e), "error"))
          }
        />
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-1 pl-7">
        <button
          type="button"
          onClick={() => setEditing(true)}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-sm text-fg-muted hover:bg-hover hover:text-fg"
        >
          <Pencil className="size-4" />
          View and edit
        </button>
        <button
          type="button"
          onClick={() => void commands.openSkillsFolder(skill.folder)}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-sm text-fg-muted hover:bg-hover hover:text-fg"
        >
          <FolderOpen className="size-4" />
          Folder
        </button>
        <button
          type="button"
          onClick={() => setConfirm(true)}
          className="ml-auto inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-sm text-danger hover:bg-danger-soft"
        >
          <Trash2 className="size-4" />
          Remove
        </button>
      </div>
      {editing && <SkillDialog folder={skill.folder} onClose={() => setEditing(false)} />}
      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        title={`Remove ${skill.name}?`}
        body="Its folder and every file in it are deleted."
        confirmLabel="Remove"
        onConfirm={() =>
          remove
            .mutateAsync(skill.folder)
            .then(() => toast(`Removed the skill “${skill.name}”.`))
            .catch((e: unknown) => toast(errorMessage(e), "error"))
        }
      />
    </li>
  );
}

/** Writes a new skill (`folder` null) or edits one. */
function SkillDialog({ folder, onClose }: { folder: string | null; onClose: () => void }) {
  const detail = useSkill(folder);
  return (
    <Modal
      open
      onOpenChange={(o) => !o && onClose()}
      title={folder === null ? "New skill" : (detail.data?.skill.name ?? "Skill")}
      wide
    >
      {folder !== null && !detail.data ? (
        <p className="text-fg-muted">{detail.error ? errorMessage(detail.error) : "Loading…"}</p>
      ) : (
        <SkillForm
          folder={folder}
          initial={
            detail.data
              ? {
                  name: detail.data.skill.name,
                  description: detail.data.skill.description,
                  body: detail.data.body,
                }
              : { name: "", description: "", body: "" }
          }
          files={detail.data?.fileList ?? []}
          onClose={onClose}
        />
      )}
    </Modal>
  );
}

function SkillForm({
  folder,
  initial,
  files,
  onClose,
}: {
  folder: string | null;
  initial: SkillInput;
  files: string[];
  onClose: () => void;
}) {
  const [input, setInput] = useState<SkillInput>(initial);
  const create = useCreateSkill();
  const update = useUpdateSkill();
  const save = folder === null ? create : update;
  const ids = { name: useId(), description: useId(), body: useId() };
  const ready =
    input.name.trim() !== "" && input.description.trim() !== "" && input.body.trim() !== "";
  return (
    <form
      className="flex min-h-0 flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        const run =
          folder === null ? create.mutateAsync(input) : update.mutateAsync({ folder, input });
        run
          .then((s) => {
            toast(folder === null ? `Created the skill “${s.name}”.` : "Skill saved.");
            onClose();
          })
          .catch(() => {});
      }}
    >
      <div className="flex min-h-0 flex-col gap-4 overflow-y-auto">
        <div>
          <label htmlFor={ids.name} className="mb-1.5 block text-sm font-medium text-fg-muted">
            Name
          </label>
          <input
            id={ids.name}
            data-autofocus={folder === null ? "" : undefined}
            value={input.name}
            onChange={(e) => setInput({ ...input, name: e.target.value })}
            placeholder="e.g. meeting-notes"
            className={`${inputClass} font-mono`}
          />
          <p className="mt-1 text-sm text-fg-subtle">
            Lowercase letters, digits and hyphens (other characters become hyphens).
          </p>
        </div>
        <div>
          <label
            htmlFor={ids.description}
            className="mb-1.5 block text-sm font-medium text-fg-muted"
          >
            Description
          </label>
          <textarea
            id={ids.description}
            rows={2}
            maxLength={MAX_DESCRIPTION}
            value={input.description}
            onChange={(e) => setInput({ ...input, description: e.target.value })}
            placeholder="What it does and when to use it, e.g. “Turns rough notes into tidy meeting minutes. Use when the user shares meeting notes.”"
            className={`${textareaClass} font-sans text-base`}
          />
          <p className="mt-1 text-sm text-fg-subtle">
            Models decide from this when to load the skill. {input.description.length}/
            {MAX_DESCRIPTION}
          </p>
        </div>
        <div>
          <label htmlFor={ids.body} className="mb-1.5 block text-sm font-medium text-fg-muted">
            Instructions (Markdown)
          </label>
          <textarea
            id={ids.body}
            rows={12}
            value={input.body}
            onChange={(e) => setInput({ ...input, body: e.target.value })}
            placeholder="# Meeting notes&#10;&#10;1. Start with the date and who was there.&#10;2. …"
            className={textareaClass}
          />
        </div>
        {files.length > 0 && <FileList files={files} />}
      </div>
      {save.error && (
        <p role="alert" className="text-base text-danger">
          {errorMessage(save.error)}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onClose}
          className="h-9 rounded-lg px-3.5 text-base text-fg-muted hover:bg-hover"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={!ready || save.isPending}
          className="h-9 rounded-lg bg-fg px-3.5 text-base font-medium text-app transition-opacity hover:opacity-90 disabled:opacity-40"
        >
          {save.isPending ? "Saving…" : folder === null ? "Create skill" : "Save"}
        </button>
      </div>
    </form>
  );
}

function FileList({ files }: { files: string[] }) {
  const shown = files.slice(0, 50);
  return (
    <div>
      <p className="mb-1 text-sm font-medium text-fg-muted">Other files in the skill</p>
      <ul className="flex flex-col gap-0.5 font-mono text-sm text-fg-subtle">
        {shown.map((f) => (
          <li key={f}>{f}</li>
        ))}
        {files.length > shown.length && <li>… and {files.length - shown.length} more</li>}
      </ul>
    </div>
  );
}
