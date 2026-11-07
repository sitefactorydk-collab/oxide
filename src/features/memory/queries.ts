import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type MemoryItem, type NewMemory } from "@/bindings";
import { unwrap } from "@/lib/api";

export const memoryKeys = { all: ["memory"] as const };

const NONE: MemoryItem[] = [];

export function useMemory() {
  const q = useQuery({
    queryKey: memoryKeys.all,
    queryFn: () => unwrap(commands.listMemory()),
  });
  return { ...q, data: q.data ?? NONE };
}

function useMemoryMutation<V, R>(run: (vars: V) => Promise<R>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSettled: () => qc.invalidateQueries({ queryKey: memoryKeys.all }),
  });
}

export function useAddMemory() {
  return useMemoryMutation((input: NewMemory) => unwrap(commands.addMemory(input)));
}

export function useUpdateMemory() {
  return useMemoryMutation(({ id, text }: { id: string; text: string }) =>
    unwrap(commands.updateMemory(id, text)),
  );
}

export function useDeleteMemory() {
  return useMemoryMutation((id: string) => unwrap(commands.deleteMemory(id)));
}

export function useClearMemory() {
  return useMemoryMutation((projectId: string | null) => unwrap(commands.clearMemory(projectId)));
}

/** Items grouped for display: everywhere first, then each project (by name). */
export function groupMemory(
  items: MemoryItem[],
  projectName: (id: string) => string | undefined,
): Array<{ key: string; title: string; projectId: string | null; items: MemoryItem[] }> {
  const global = items.filter((m) => m.projectId === null);
  const byProject = new Map<string, MemoryItem[]>();
  for (const m of items) {
    if (m.projectId === null) continue;
    byProject.set(m.projectId, [...(byProject.get(m.projectId) ?? []), m]);
  }
  const groups = [...byProject.entries()]
    .map(([id, list]) => ({
      key: id,
      title: projectName(id) ?? "A deleted project",
      projectId: id as string | null,
      items: list,
    }))
    .sort((a, b) => a.title.localeCompare(b.title));
  return [{ key: "global", title: "All chats", projectId: null, items: global }, ...groups];
}
