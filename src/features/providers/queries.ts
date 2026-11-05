import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type ProviderConfig, type ProviderInput, type TestProvider } from "@/bindings";
import { modelKeys } from "@/features/models/queries";
import { unwrap } from "@/lib/api";

export const providerKeys = { list: ["providers", "list"] as const };

const NO_PROVIDERS: ProviderConfig[] = [];

export function useProviders() {
  const q = useQuery({
    queryKey: providerKeys.list,
    queryFn: () => unwrap(commands.listProviders()),
  });
  return { ...q, data: q.data ?? NO_PROVIDERS };
}

function useProviderMutation<V, R>(run: (v: V) => Promise<R>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: run,
    // The backend forgets its model listings when endpoints change; so does the picker.
    onSettled: () =>
      Promise.all([
        qc.invalidateQueries({ queryKey: providerKeys.list }),
        qc.invalidateQueries({ queryKey: modelKeys.all }),
      ]),
  });
}

export function useSaveProvider() {
  return useProviderMutation((input: ProviderInput) => unwrap(commands.saveProvider(input)));
}

export function useDeleteProvider() {
  return useProviderMutation((id: string) => unwrap(commands.deleteProvider(id)));
}

export function useSetProviderKey() {
  return useProviderMutation(({ id, key }: { id: string; key: string }) =>
    unwrap(commands.setProviderKey(id, key)),
  );
}

export function useClearProviderKey() {
  return useProviderMutation((id: string) => unwrap(commands.clearProviderKey(id)));
}

export function useTestProvider() {
  return useMutation({
    mutationFn: (input: TestProvider) => unwrap(commands.testProvider(input)),
  });
}
