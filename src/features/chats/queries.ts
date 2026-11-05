import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import { commands, type Conversation, type Message, type StartConversation } from "@/bindings";
import { unwrap } from "@/lib/api";

/** Query keys for chat data. */
export const chatKeys = {
  all: ["conversations"] as const,
  list: () => [...chatKeys.all, "list"] as const,
  messages: (id: string) => [...chatKeys.all, "messages", id] as const,
  search: (q: string) => [...chatKeys.all, "search", q] as const,
};

const NO_CONVERSATIONS: Conversation[] = [];

/** All chats, newest first. */
export function useConversations() {
  const q = useQuery({
    queryKey: chatKeys.list(),
    queryFn: () => unwrap(commands.listConversations()),
  });
  return { ...q, data: q.data ?? NO_CONVERSATIONS };
}

/** One chat, read from the list (which is always loaded for the sidebar). */
export function useConversation(id: string | undefined) {
  const q = useConversations();
  return { ...q, conversation: id ? q.data.find((c) => c.id === id) : undefined };
}

export function useMessages(conversationId: string) {
  return useQuery({
    queryKey: chatKeys.messages(conversationId),
    queryFn: () => unwrap(commands.listMessages(conversationId)),
    enabled: conversationId !== "",
  });
}

/** Full-text search. Empty queries don't hit the backend. */
export function useSearch(query: string, limit = 30) {
  const q = query.trim();
  return useQuery({
    queryKey: chatKeys.search(q),
    queryFn: () => unwrap(commands.searchConversations(q, limit)),
    enabled: q !== "",
    staleTime: 0,
    placeholderData: keepPreviousData,
  });
}

function sortByUpdated(list: Conversation[]): Conversation[] {
  return [...list].sort((a, b) => b.updatedAt - a.updatedAt);
}

function patchList(qc: QueryClient, fn: (list: Conversation[]) => Conversation[]) {
  qc.setQueryData<Conversation[]>(chatKeys.list(), (old) => (old ? fn(old) : old));
}

function replaceOne(qc: QueryClient, c: Conversation) {
  patchList(qc, (list) => sortByUpdated(list.map((x) => (x.id === c.id ? c : x))));
}

/**
 * Optimistic edit of one chat in the list, rolled back if the command fails. Search results
 * are refreshed afterwards either way.
 */
function useConversationEdit<V extends { id: string }>(
  apply: (c: Conversation, vars: V) => Conversation,
  run: (vars: V) => Promise<Conversation>,
) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: run,
    onMutate: async (vars: V) => {
      await qc.cancelQueries({ queryKey: chatKeys.list() });
      const previous = qc.getQueryData<Conversation[]>(chatKeys.list());
      patchList(qc, (list) => list.map((c) => (c.id === vars.id ? apply(c, vars) : c)));
      return { previous };
    },
    onError: (_e, _vars, ctx) => {
      if (ctx?.previous) qc.setQueryData(chatKeys.list(), ctx.previous);
    },
    onSuccess: (c) => replaceOne(qc, c),
    onSettled: () => qc.invalidateQueries({ queryKey: [...chatKeys.all, "search"] }),
  });
}

export function useRenameConversation() {
  return useConversationEdit<{ id: string; title: string }>(
    (c, { title }) => ({ ...c, title: title.trim() || c.title }),
    ({ id, title }) => unwrap(commands.renameConversation(id, title)),
  );
}

export function useStarConversation() {
  return useConversationEdit<{ id: string; starred: boolean }>(
    (c, { starred }) => ({ ...c, starred }),
    ({ id, starred }) => unwrap(commands.setConversationStarred(id, starred)),
  );
}

export function useDeleteConversation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(commands.deleteConversation(id)),
    onMutate: async (id: string) => {
      await qc.cancelQueries({ queryKey: chatKeys.list() });
      const previous = qc.getQueryData<Conversation[]>(chatKeys.list());
      patchList(qc, (list) => list.filter((c) => c.id !== id));
      return { previous };
    },
    onError: (_e, _id, ctx) => {
      if (ctx?.previous) qc.setQueryData(chatKeys.list(), ctx.previous);
    },
    onSuccess: (_r, id) => qc.removeQueries({ queryKey: chatKeys.messages(id) }),
    onSettled: () => qc.invalidateQueries({ queryKey: [...chatKeys.all, "search"] }),
  });
}

export function useStartConversation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: StartConversation) => unwrap(commands.startConversation(input)),
    onSuccess: (c) => {
      patchList(qc, (list) => sortByUpdated([c, ...list.filter((x) => x.id !== c.id)]));
      return qc.invalidateQueries({ queryKey: chatKeys.all });
    },
  });
}

/** Adds the new message to the cached thread and bumps the chat to the top of Recents. */
function useMessageMutation<V>(run: (vars: V) => Promise<Message>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSuccess: (m) => {
      qc.setQueryData<Message[]>(chatKeys.messages(m.conversationId), (old) =>
        old ? [...old, m] : [m],
      );
      patchList(qc, (list) =>
        sortByUpdated(
          list.map((c) => (c.id === m.conversationId ? { ...c, updatedAt: m.createdAt } : c)),
        ),
      );
      return qc.invalidateQueries({ queryKey: [...chatKeys.all, "search"] });
    },
  });
}

export function useAddUserMessage() {
  return useMessageMutation(
    (v: { conversationId: string; parentId: string | null; text: string }) =>
      unwrap(commands.addUserMessage(v.conversationId, v.parentId, v.text)),
  );
}

export function useEditUserMessage() {
  return useMessageMutation((v: { messageId: string; text: string }) =>
    unwrap(commands.editUserMessage(v.messageId, v.text)),
  );
}
