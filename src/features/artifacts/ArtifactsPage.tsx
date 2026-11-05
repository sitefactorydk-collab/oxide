import { Code, FileText, Globe, Network, PenTool, Atom, type LucideIcon } from "lucide-react";
import { useMemo, useState } from "react";
import { Link } from "react-router";
import { PageHeader, SearchField } from "@/components/PageHeader";
import { useMock } from "@/features/mock/store";
import type { ArtifactKind } from "@/features/mock/types";
import { formatRelative } from "@/lib/time";

const KIND: Record<ArtifactKind, { label: string; icon: LucideIcon }> = {
  code: { label: "Code", icon: Code },
  markdown: { label: "Document", icon: FileText },
  html: { label: "Web page", icon: Globe },
  svg: { label: "Image", icon: PenTool },
  mermaid: { label: "Diagram", icon: Network },
  react: { label: "Interactive", icon: Atom },
};

export function ArtifactsPage() {
  const artifacts = useMock((s) => s.artifacts);
  const conversations = useMock((s) => s.conversations);
  const [q, setQ] = useState("");

  const list = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return [...artifacts]
      .filter((a) => !needle || a.title.toLowerCase().includes(needle))
      .sort((a, b) => b.updatedAt - a.updatedAt);
  }, [artifacts, q]);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-page px-6 py-10">
        <PageHeader title="Artifacts" />
        <SearchField
          value={q}
          onChange={setQ}
          label="Filter artifacts"
          placeholder="Filter artifacts…"
        />
        <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {list.map((a) => {
            const { label, icon: Icon } = KIND[a.kind];
            const convo = conversations.find((c) => c.id === a.conversationId);
            return (
              <li key={a.id}>
                <Link
                  to={`/chat/${a.conversationId}`}
                  className="flex h-full flex-col overflow-hidden rounded-xl border border-line bg-raised shadow-sm transition-colors hover:border-line-strong"
                >
                  <div className="flex h-28 items-center justify-center bg-sunken text-fg-subtle">
                    <Icon className="size-8" strokeWidth={1.5} />
                  </div>
                  <div className="flex flex-col gap-0.5 p-3">
                    <span className="truncate text-md font-medium text-fg">{a.title}</span>
                    <span className="truncate text-sm text-fg-subtle">
                      {label} · {formatRelative(a.updatedAt)}
                      {convo ? ` · ${convo.title}` : ""}
                    </span>
                  </div>
                </Link>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
