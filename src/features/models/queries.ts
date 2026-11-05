import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type ProviderModels } from "@/bindings";
import { unwrap } from "@/lib/api";
import { useUi } from "@/stores/ui";
import { findModel, modelNameFromId, pickDefaultModel } from "./models";

export const modelKeys = { all: ["models"] as const };

const NO_MODELS: ProviderModels[] = [];

/** Models per endpoint. The backend caches listings, so this stays cheap to refetch. */
export function useModels() {
  const q = useQuery({
    queryKey: modelKeys.all,
    queryFn: () => unwrap(commands.listModels(false)),
    staleTime: 5 * 60_000,
  });
  return { ...q, data: q.data ?? NO_MODELS };
}

/** Lists every endpoint again, skipping caches and re-checking probed tool support. */
export function useRefreshModels() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.listModels(true)),
    onSuccess: (lists) => qc.setQueryData(modelKeys.all, lists),
  });
}

/** Display name of a stored model id. */
export function useModelLabel(id: string): string {
  const { data } = useModels();
  return findModel(data, id)?.label ?? modelNameFromId(id);
}

/**
 * The model for new chats: the one picked last, or a sensible default when that one isn't
 * available (any more).
 */
export function useNewChatModel(): [string, (id: string) => void] {
  const preferred = useUi((s) => s.modelId);
  const setModelId = useUi((s) => s.setModelId);
  const { data } = useModels();
  return [pickDefaultModel(data, preferred) ?? preferred, setModelId];
}
