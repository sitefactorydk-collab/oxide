import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  commands,
  type AttachmentSource,
  type Conversation,
  type KnowledgeFile,
  type NewProject,
  type Project,
  type ProjectPatch,
} from "@/bindings";
import { errorMessage, unwrap } from "@/lib/api";
import { chatKeys, replaceConversation } from "@/features/chats/queries";

export const projectKeys = {
  all: ["projects"] as const,
  list: () => [...projectKeys.all, "list"] as const,
  allKnowledge: () => [...projectKeys.all, "knowledge"] as const,
  knowledge: (id: string) => [...projectKeys.all, "knowledge", id] as const,
  file: (id: string) => [...projectKeys.all, "file", id] as const,
};

const NO_PROJECTS: Project[] = [];

/** A patch that changes only the given fields. */
export function patchOf(change: Partial<ProjectPatch>): ProjectPatch {
  return { name: null, description: null, instructions: null, archived: null, ...change };
}

export function useProjects() {
  const q = useQuery({
    queryKey: projectKeys.list(),
    queryFn: () => unwrap(commands.listProjects()),
  });
  return { ...q, data: q.data ?? NO_PROJECTS };
}

export function useProject(id: string | undefined) {
  const q = useProjects();
  return { ...q, project: id ? q.data.find((p) => p.id === id) : undefined };
}

export function useCreateProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: NewProject) => unwrap(commands.createProject(input)),
    onSuccess: () => qc.invalidateQueries({ queryKey: projectKeys.all }),
  });
}

export function useUpdateProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: ProjectPatch }) =>
      unwrap(commands.updateProject(id, patch)),
    onSuccess: (p) => {
      qc.setQueryData<Project[]>(projectKeys.list(), (old) =>
        old?.map((x) => (x.id === p.id ? p : x)),
      );
      return qc.invalidateQueries({ queryKey: projectKeys.list() });
    },
  });
}

export function useDeleteProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(commands.deleteProject(id)),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: projectKeys.all });
      // Its chats are now outside any project; its memory items are gone.
      await qc.invalidateQueries({ queryKey: chatKeys.all });
      await qc.invalidateQueries({ queryKey: ["memory"] });
    },
  });
}

/** Moves a chat into a project, or out of one with `projectId: null`. */
export function useSetConversationProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, projectId }: { id: string; projectId: string | null }) =>
      unwrap(commands.setConversationProject(id, projectId)),
    onSuccess: (c: Conversation) => {
      replaceConversation(qc, c);
      void qc.invalidateQueries({ queryKey: chatKeys.all });
    },
  });
}

export function useProjectKnowledge(projectId: string) {
  return useQuery({
    queryKey: projectKeys.knowledge(projectId),
    queryFn: () => unwrap(commands.projectKnowledge(projectId)),
  });
}

export function useKnowledgeFile(id: string | null) {
  return useQuery({
    queryKey: projectKeys.file(id ?? ""),
    queryFn: () => unwrap(commands.getKnowledgeFile(id ?? "")),
    enabled: id !== null,
  });
}

/** Result of adding several files: what was added and what was refused (with why). */
export interface AddFilesResult {
  added: KnowledgeFile[];
  failed: Array<{ name: string; error: string }>;
}

/** Adds files one after another, so one bad file doesn't stop the rest. */
export function useAddProjectFiles(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (
      sources: Array<{ name: string; source: () => Promise<AttachmentSource> }>,
    ): Promise<AddFilesResult> => {
      const result: AddFilesResult = { added: [], failed: [] };
      for (const s of sources) {
        try {
          const source = await s.source();
          result.added.push(await unwrap(commands.addProjectFile(projectId, source)));
        } catch (e) {
          result.failed.push({ name: s.name, error: errorMessage(e) });
        }
      }
      return result;
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: projectKeys.knowledge(projectId) });
      void qc.invalidateQueries({ queryKey: projectKeys.list() });
    },
  });
}

export function useAddProjectText(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, text }: { name: string; text: string }) =>
      unwrap(commands.addProjectText(projectId, name, text)),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: projectKeys.knowledge(projectId) });
      void qc.invalidateQueries({ queryKey: projectKeys.list() });
    },
  });
}

export function useDeleteKnowledgeFile(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(commands.deleteKnowledgeFile(id)),
    onSuccess: (_r, id) => {
      qc.removeQueries({ queryKey: projectKeys.file(id) });
      void qc.invalidateQueries({ queryKey: projectKeys.knowledge(projectId) });
    },
  });
}
