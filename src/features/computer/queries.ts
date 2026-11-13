import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, events, type ComputerActivity } from "@/bindings";
import { unwrap } from "@/lib/api";

export const computerKeys = {
  status: ["computer", "status"] as const,
  activity: ["computer", "activity"] as const,
};

/** Settings → Computer use: whether it works here, the screens, the panic shortcut. */
export function useComputerStatus() {
  return useQuery({
    queryKey: computerKeys.status,
    queryFn: () => unwrap(commands.computerStatus()),
    staleTime: 0,
  });
}

/** What chats controlling the computer do now, kept current by `computerChanged`. */
export function useComputerActivity(): ComputerActivity[] {
  const qc = useQueryClient();
  const { data } = useQuery({
    queryKey: computerKeys.activity,
    queryFn: () => unwrap(commands.computerActivity()),
    staleTime: Infinity,
  });
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | null = null;
    events.computerChanged
      .listen(({ payload }) => {
        qc.setQueryData(computerKeys.activity, payload.chats);
        void qc.invalidateQueries({ queryKey: computerKeys.status });
      })
      .then((unlisten) => (disposed ? unlisten() : (stop = unlisten)))
      .catch(() => undefined);
    return () => {
      disposed = true;
      stop?.();
    };
  }, [qc]);
  return data ?? [];
}

/** Stops every chat that controls the computer. */
export function useStopComputer() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.computerStopAll()),
    onSettled: () => qc.invalidateQueries({ queryKey: computerKeys.status }),
  });
}
