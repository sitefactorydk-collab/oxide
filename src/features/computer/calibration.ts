import type { PointerCalibration } from "@/bindings";

/** How far a calibration says the model points off, and what's left after correcting. */
export function describeCalibration(c: PointerCalibration): string {
  const parts: string[] = [];
  const dir = (n: number, neg: string, pos: string) =>
    Math.abs(n) >= 2 ? `${Math.abs(n)} px too far ${n < 0 ? neg : pos}` : null;
  const y = dir(c.y.offset, "up", "down");
  const x = dir(c.x.offset, "left", "right");
  if (y) parts.push(y);
  if (x) parts.push(x);
  for (const [axis, f] of [
    ["across", c.x],
    ["down", c.y],
  ] as const) {
    const pct = Math.round((f.scale - 10000) / 100);
    if (Math.abs(pct) >= 1) parts.push(`stretched ${pct > 0 ? "+" : ""}${pct} % ${axis}`);
  }
  const found = `${c.markers} of ${c.shown} dots found`;
  if (parts.length === 0) return `Points accurately (about ${c.errorBefore} px off) · ${found}`;
  return `Points ${parts.join(", ")}: corrected (about ${c.errorBefore} → ${c.errorAfter} px off) · ${found}`;
}
