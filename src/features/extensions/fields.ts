import type { ExtensionField, ExtensionValue } from "@/bindings";
import type { ExtensionValues } from "./queries";

/** Whether a value counts as filled in. */
export function isFilled(v: ExtensionValue | undefined): boolean {
  if (v === undefined) return false;
  if (typeof v === "string") return v.trim() !== "";
  if (Array.isArray(v)) return v.some((s) => s.trim() !== "");
  return true;
}

/** Titles of required fields that are still empty. */
export function missingRequired(
  fields: ExtensionField[],
  values: ExtensionValues,
  secretsSet: string[],
): string[] {
  return fields
    .filter((f) => f.required)
    .filter((f) => !isFilled(values[f.key]))
    .filter((f) => !(f.sensitive && secretsSet.includes(f.key)))
    .filter((f) => !isFilled(f.default ?? undefined))
    .map((f) => f.title);
}
