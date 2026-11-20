import { MessageSquare, Sparkles } from "lucide-react";

/**
 * The starting point of a new app. It only says where to start: the app is described in the
 * builder's chat (on the left), and the builder replaces this file with the real app.
 */
export default function App() {
  return (
    <div className="flex min-h-screen items-center justify-center p-8 text-neutral-900 dark:text-neutral-100">
      <div className="flex max-w-md flex-col items-center gap-3 text-center">
        <Sparkles className="size-8 text-orange-600" />
        <h1 className="text-xl font-semibold">Your new app will appear here</h1>
        <p className="text-sm text-neutral-500 dark:text-neutral-400">
          Describe the app you want in the builder&apos;s chat on the left. The builder writes the
          code, and this preview updates as it works.
        </p>
        <p className="flex items-center gap-1.5 text-sm text-neutral-500 dark:text-neutral-400">
          <MessageSquare className="size-4" /> For example: “An app that turns a pasted CSV into a
          chart and lets me download it as PNG.”
        </p>
      </div>
    </div>
  );
}
