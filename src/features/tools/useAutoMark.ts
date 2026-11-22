import { useParams } from "react-router";
import type { AutoMark } from "@/bindings";
import type { ToolCallView } from "@/features/chat/streams";
import { useToolUsage } from "./queries";

/** The call's **Auto** mark: live from the stream, else from the chat's tool log. */
export function useAutoMark(call: ToolCallView): AutoMark | null {
  const { id: conversationId = "" } = useParams();
  const { data } = useToolUsage(call.auto ? "" : conversationId);
  const mark = call.auto ?? data?.find((u) => u.toolUseId === call.id)?.auto ?? null;
  return mark && call.status !== "awaitingApproval" ? mark : null;
}
