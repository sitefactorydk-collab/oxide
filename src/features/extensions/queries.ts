import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  commands,
  type Extension,
  type ExtensionValue,
  type PickKind,
  type Skill,
  type SkillInput,
} from "@/bindings";
import { mcpKeys } from "@/features/mcp/queries";
import { unwrap } from "@/lib/api";

export type ExtensionValues = Partial<Record<string, ExtensionValue>>;

export const extensionKeys = {
  all: ["extensions"] as const,
  list: ["extensions", "list"] as const,
  skills: ["skills"] as const,
  skillList: ["skills", "list"] as const,
  skill: (folder: string) => ["skills", "detail", folder] as const,
};

const NO_EXTENSIONS: Extension[] = [];
const NO_SKILLS: Skill[] = [];

export function useExtensions() {
  const q = useQuery({
    queryKey: extensionKeys.list,
    queryFn: () => unwrap(commands.listExtensions()),
  });
  return { ...q, data: q.data ?? NO_EXTENSIONS };
}

export function pickPaths(kind: PickKind, multiple = false): Promise<string[]> {
  return unwrap(commands.pickPaths(kind, multiple));
}

/** Changes to extensions also change the connector list. */
function useExtensionMutation<V, R>(run: (v: V) => Promise<R>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: extensionKeys.all });
      void qc.invalidateQueries({ queryKey: mcpKeys.servers });
      void qc.invalidateQueries({ queryKey: mcpKeys.config });
    },
  });
}

export function usePreviewExtension() {
  return useMutation({ mutationFn: (path: string) => unwrap(commands.previewExtension(path)) });
}

export function useInstallExtension() {
  return useExtensionMutation((v: { stagingId: string; values: ExtensionValues }) =>
    unwrap(commands.installExtension(v.stagingId, v.values)),
  );
}

export function discardExtension(stagingId: string): void {
  void commands.discardExtension(stagingId);
}

export function useConfigureExtension() {
  return useExtensionMutation((v: { name: string; values: ExtensionValues }) =>
    unwrap(commands.configureExtension(v.name, v.values)),
  );
}

export function useSetExtensionEnabled() {
  return useExtensionMutation((v: { name: string; enabled: boolean }) =>
    unwrap(commands.setExtensionEnabled(v.name, v.enabled)),
  );
}

export function useUninstallExtension() {
  return useExtensionMutation((name: string) => unwrap(commands.uninstallExtension(name)));
}

// ----- Skills -----

export function useSkills() {
  const q = useQuery({
    queryKey: extensionKeys.skillList,
    queryFn: () => unwrap(commands.listSkills()),
  });
  return { ...q, data: q.data ?? NO_SKILLS };
}

export function useSkill(folder: string | null) {
  return useQuery({
    queryKey: extensionKeys.skill(folder ?? ""),
    queryFn: () => unwrap(commands.getSkill(folder ?? "")),
    enabled: folder !== null,
    staleTime: 0,
  });
}

function useSkillMutation<V, R>(run: (v: V) => Promise<R>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSettled: () => void qc.invalidateQueries({ queryKey: extensionKeys.skills }),
  });
}

export function useCreateSkill() {
  return useSkillMutation((input: SkillInput) => unwrap(commands.createSkill(input)));
}

export function useUpdateSkill() {
  return useSkillMutation((v: { folder: string; input: SkillInput }) =>
    unwrap(commands.updateSkill(v.folder, v.input)),
  );
}

export function useAddSkill() {
  return useSkillMutation((path: string) => unwrap(commands.addSkill(path)));
}

export function useSetSkillEnabled() {
  return useSkillMutation((v: { folder: string; enabled: boolean }) =>
    unwrap(commands.setSkillEnabled(v.folder, v.enabled)),
  );
}

export function useRemoveSkill() {
  return useSkillMutation((folder: string) => unwrap(commands.removeSkill(folder)));
}
