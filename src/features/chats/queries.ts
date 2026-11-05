import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import {
  commands,
  type Attachment,
  type Conversation,
  type ExportFormat,
  type Feedback,
  type GenerateOptions,
  type Message,
  type StartConversation,
} from "@/bindings";
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

/** Replaces a chat in the cached list (e.g. after it got an automatic title). */
export function replaceConversation(qc: QueryClient, c: Conversation) {
  patchList(qc, (list) => sortByUpdated(list.map((x) => (x.id === c.id ? c : x))));
}

/** Adds a saved message to its chat's cached thread and bumps the chat in Recents. */
export function addMessageToCache(qc: QueryClient, m: Message) {
  qc.setQueryData<Message[]>(chatKeys.messages(m.conversationId), (old) =>
    old ? (old.some((x) => x.id === m.id) ? old : [...old, m]) : old,
  );
  patchList(qc, (list) =>
    sortByUpdated(
      list.map((c) =>
        c.id === m.conversationId ? { ...c, updatedAt: Math.max(c.updatedAt, m.createdAt) } : c,
      ),
    ),
  );
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
    onSuccess: (c) => replaceConversation(qc, c),
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

export function useSetConversationModel() {
  return useConversationEdit<{ id: string; model: string }>(
    (c, { model }) => ({ ...c, model }),
    ({ id, model }) => unwrap(commands.setConversationModel(id, model)),
  );
}

/** Creates a chat with its first message and starts the reply. */
export function useStartConversation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { input: StartConversation; options: GenerateOptions }) =>
      unwrap(commands.startConversation(v.input, v.options)),
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
      addMessageToCache(qc, m);
      return qc.invalidateQueries({ queryKey: [...chatKeys.all, "search"] });
    },
  });
}

/** Sends a message and starts the reply (which streams as events). */
export function useSendMessage() {
  return useMessageMutation(
    (v: {
      conversationId: string;
      parentId: string | null;
      text: string;
      attachments: Attachment[];
      options: GenerateOptions;
    }) =>
      unwrap(commands.sendMessage(v.conversationId, v.parentId, v.text, v.attachments, v.options)),
  );
}

/** Edits a message as a new branch and answers the edited version. */
export function useEditAndResend() {
  return useMessageMutation((v: { messageId: string; text: string; options: GenerateOptions }) =>
    unwrap(commands.editAndResend(v.messageId, v.text, v.options)),
  );
}

/** A new reply next to an assistant message, or an answer to a user message. */
export function useRegenerate() {
  return useMutation({
    mutationFn: (v: { messageId: string; options: GenerateOptions }) =>
      unwrap(commands.regenerate(v.messageId, v.options)),
  });
}

export function useStopGeneration() {
  return useMutation({
    mutationFn: (conversationId: string) => unwrap(commands.stopGeneration(conversationId)),
  });
}

/** Replaces a message in its chat's cached thread (e.g. after it was rated). */
function replaceMessageInCache(qc: QueryClient, m: Message) {
  qc.setQueryData<Message[]>(chatKeys.messages(m.conversationId), (old) =>
    old?.map((x) => (x.id === m.id ? m : x)),
  );
}

/** Rates a reply (thumbs up/down with an optional note), or clears the rating. */
export function useSetFeedback() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { message: Message; feedback: Feedback | null }) =>
      unwrap(commands.setMessageFeedback(v.message.id, v.feedback)),
    onMutate: ({ message, feedback }) => {
      const previous = qc.getQueryData<Message[]>(chatKeys.messages(message.conversationId));
      replaceMessageInCache(qc, { ...message, feedback });
      return { previous, conversationId: message.conversationId };
    },
    onError: (_e, _v, ctx) => {
      if (ctx?.previous) qc.setQueryData(chatKeys.messages(ctx.conversationId), ctx.previous);
    },
    onSuccess: (m) => replaceMessageInCache(qc, m),
  });
}

/** Saves a chat as Markdown (the visible thread) or JSON (every branch). */
export function useExportConversation() {
  return useMutation({
    mutationFn: (v: { id: string; leafId: string | null; format: ExportFormat }) =>
      unwrap(commands.exportConversation(v.id, v.leafId, v.format)),
  });
}

/** Creates the 50-message demo chat (Settings → About, development builds). */
export function useCreateDemoConversation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (model: string) => unwrap(commands.createDemoConversation(model)),
    onSuccess: (c) => {
      patchList(qc, (list) => sortByUpdated([c, ...list.filter((x) => x.id !== c.id)]));
    },
  });
}
