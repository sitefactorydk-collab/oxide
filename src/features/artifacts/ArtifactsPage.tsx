import { Shapes } from "lucide-react";
import { PageHeader } from "@/components/PageHeader";

/** Artifact gallery. Artifacts are stored and rendered from Phase 8; until then it's empty. */
export function ArtifactsPage() {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-page px-6 py-10">
        <PageHeader title="Artifacts" />
        <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-line-strong px-6 py-14 text-center">
          <Shapes className="size-8 text-fg-subtle" strokeWidth={1.5} />
          <p className="text-md text-fg">No artifacts yet</p>
          <p className="max-w-sm text-base text-fg-muted">
            Documents, code, diagrams and small apps made in your chats will collect here. They
            arrive in Phase 8.
          </p>
        </div>
      </div>
    </div>
  );
}
