import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import {
  commands,
  events,
  type Conversation,
  type ScheduledTask,
  type TaskInput,
  type TaskSchedule,
} from "@/bindings";
import { chatKeys } from "@/features/chats/queries";
import { unwrap } from "@/lib/api";
import { toast } from "@/lib/toast";

export const scheduleKeys = {
  all: ["scheduled"] as const,
  list: () => [...scheduleKeys.all, "list"] as const,
  runs: (taskId: string) => [...scheduleKeys.all, "runs", taskId] as const,
  preview: (s: TaskSchedule) => [...scheduleKeys.all, "preview", s] as const,
  chatRun: (conversationId: string) => [...scheduleKeys.all, "chat", conversationId] as const,
};

const NO_TASKS: ScheduledTask[] = [];

/** Every task, by name, each with its latest run. */
export function useScheduledTasks() {
  const q = useQuery({
    queryKey: scheduleKeys.list(),
    queryFn: () => unwrap(commands.listScheduledTasks()),
  });
  return { ...q, data: q.data ?? NO_TASKS };
}

export function useScheduledTask(id: string | undefined) {
  const q = useScheduledTasks();
  return { ...q, task: id ? q.data.find((t) => t.id === id) : undefined };
}

/** A task's runs, newest first. */
export function useTaskRuns(taskId: string) {
  return useQuery({
    queryKey: scheduleKeys.runs(taskId),
    queryFn: () => unwrap(commands.scheduledTaskRuns(taskId, 50)),
  });
}

/** The next three runs of a schedule (an error says what's wrong with it). */
export function useSchedulePreview(schedule: TaskSchedule) {
  return useQuery({
    queryKey: scheduleKeys.preview(schedule),
    queryFn: () => unwrap(commands.previewSchedule(schedule)),
    placeholderData: keepPreviousData,
    // Times move on: never trust a cached preview.
    staleTime: 0,
  });
}

/** The scheduled run that wrote in a chat (`null` for ordinary chats). */
export function useTaskRunForChat(conversationId: string | undefined) {
  return useQuery({
    queryKey: scheduleKeys.chatRun(conversationId ?? ""),
    queryFn: () => unwrap(commands.taskRunForChat(conversationId ?? "")),
    enabled: !!conversationId,
  });
}

function useInvalidate() {
  const qc = useQueryClient();
  return () => qc.invalidateQueries({ queryKey: scheduleKeys.all });
}

export function useCreateTask() {
  const done = useInvalidate();
  return useMutation({
    mutationFn: (input: TaskInput) => unwrap(commands.createScheduledTask(input)),
    onSuccess: done,
  });
}

export function useUpdateTask() {
  const done = useInvalidate();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: TaskInput }) =>
      unwrap(commands.updateScheduledTask(id, input)),
    onSuccess: done,
  });
}

export function useSetTaskEnabled() {
  const done = useInvalidate();
  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      unwrap(commands.setScheduledTaskEnabled(id, enabled)),
    onSuccess: done,
    onError: (e) => toast(e.message, "error"),
  });
}

export function useDeleteTask() {
  const done = useInvalidate();
  return useMutation({
    mutationFn: (id: string) => unwrap(commands.deleteScheduledTask(id)),
    onSuccess: done,
    onError: (e) => toast(e.message, "error"),
  });
}

export function useRunTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(commands.runScheduledTask(id)),
    onSuccess: async (run) => {
      if (run.status === "skipped") toast("The previous run is still going.");
      await qc.invalidateQueries({ queryKey: scheduleKeys.all });
      await qc.invalidateQueries({ queryKey: chatKeys.list() });
    },
    onError: (e) => toast(e.message, "error"),
  });
}

/**
 * Keeps scheduled tasks fresh app-wide: runs start, wait, finish and fail in the background.
 * While Oxide is in front, a run that needs the user or ends says so in a toast (the system
 * notification only comes while it's in the background).
 */
export function useScheduleEvents(): void {
  const qc = useQueryClient();
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void events.scheduleChanged
      .listen(({ payload }) => {
        void qc.invalidateQueries({ queryKey: scheduleKeys.all });
        const run = payload.run;
        if (!run) return;
        // A run's new chat isn't in the sidebar's list yet.
        const chats = qc.getQueryData<Conversation[]>(chatKeys.list());
        if (run.conversationId && !chats?.some((c) => c.id === run.conversationId)) {
          void qc.invalidateQueries({ queryKey: chatKeys.list() });
        }
        if (typeof document !== "undefined" && document.hasFocus()) {
          const name =
            qc
              .getQueryData<ScheduledTask[]>(scheduleKeys.list())
              ?.find((t) => t.id === payload.taskId)?.name ?? "A scheduled task";
          if (run.status === "waitingApproval") toast(`“${name}” needs your approval.`);
          else if (run.status === "done") toast(`“${name}” finished.`);
          else if (run.status === "failed") toast(`“${name}” failed.`, "error");
        }
      })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [qc]);
}
