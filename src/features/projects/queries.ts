import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type NewProject, type Project, type ProjectPatch } from "@/bindings";
import { unwrap } from "@/lib/api";
import { chatKeys } from "@/features/chats/queries";

export const projectKeys = {
  all: ["projects"] as const,
  list: () => [...projectKeys.all, "list"] as const,
};

const NO_PROJECTS: Project[] = [];

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
    onSuccess: () => qc.invalidateQueries({ queryKey: projectKeys.all }),
  });
}

export function useDeleteProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(commands.deleteProject(id)),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: projectKeys.all });
      // Its chats are now outside any project.
      await qc.invalidateQueries({ queryKey: chatKeys.all });
    },
  });
}
