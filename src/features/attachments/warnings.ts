import type { Attachment, ModelCapabilities, ProviderKind } from "@/bindings";

/** What the chosen model will do with an attachment it can't take as is. */
export function modelWarning(
  a: Attachment,
  caps: ModelCapabilities | undefined,
  kind: ProviderKind | undefined,
): string | null {
  if (!caps) return null;
  if (a.kind === "image" && !caps.images && kind !== "strata") {
    return "This model can’t see images; it will be told one was attached.";
  }
  if (a.kind === "pdf" && !(kind === "anthropic" && caps.pdfs)) {
    return a.pages === null
      ? "This model gets PDFs as text, and none could be extracted from this one."
      : "This model gets the PDF’s extracted text (no images or layout).";
  }
  return null;
}
