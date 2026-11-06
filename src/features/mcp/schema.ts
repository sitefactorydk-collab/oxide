/** Small JSON-schema helpers for the tool runner and elicitation forms. */

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };
type Schema = { [k: string]: unknown };

export function parseJson(text: string | null | undefined): unknown {
  if (!text) return undefined;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return undefined;
  }
}

function asSchema(v: unknown): Schema | undefined {
  return v && typeof v === "object" && !Array.isArray(v) ? (v as Schema) : undefined;
}

/** Example arguments for a tool: each property's default, or an empty value of its type. */
export function argumentSkeleton(schemaText: string): string {
  const schema = asSchema(parseJson(schemaText));
  const value = schema ? skeleton(schema, 0) : {};
  return JSON.stringify(value ?? {}, null, 2);
}

function skeleton(schema: Schema, depth: number): Json {
  if ("default" in schema) return schema.default as Json;
  const en = schema.enum;
  if (Array.isArray(en) && en.length > 0) return en[0] as Json;
  // "Either/or" schemas: use the first choice that isn't null.
  for (const key of ["anyOf", "oneOf"] as const) {
    const choices = schema[key];
    if (Array.isArray(choices)) {
      const first = choices.map(asSchema).find((c) => c !== undefined && c.type !== "null");
      if (first) return skeleton({ ...first, ...withoutChoices(schema) }, depth);
    }
  }
  const types = Array.isArray(schema.type)
    ? (schema.type as unknown[]).filter((t) => t !== "null")
    : [schema.type];
  const type = types[0];
  switch (type) {
    case "string":
      return "";
    case "integer":
    case "number":
      return 0;
    case "boolean":
      return false;
    case "array":
      return [];
    case "object":
    case undefined: {
      const props = asSchema(schema.properties);
      if (!props || depth > 4) return {};
      const out: { [k: string]: Json } = {};
      for (const [k, v] of Object.entries(props)) {
        const s = asSchema(v);
        if (s) out[k] = skeleton(s, depth + 1);
      }
      return out;
    }
    default:
      return null;
  }
}

/** A schema's own keywords without its `anyOf` / `oneOf` choices. */
function withoutChoices(schema: Schema): Schema {
  const rest = { ...schema };
  delete rest.anyOf;
  delete rest.oneOf;
  return rest;
}

/** One field of an elicitation form (MCP limits these to flat primitive properties). */
export interface FormField {
  name: string;
  label: string;
  description?: string;
  kind: "text" | "number" | "integer" | "boolean" | "select" | "multiselect";
  required: boolean;
  options?: Array<{ value: string; label: string }>;
  format?: string;
  defaultValue?: Json;
  min?: number;
  max?: number;
}

function options(s: Schema): FormField["options"] {
  if (Array.isArray(s.oneOf)) {
    return (s.oneOf as unknown[]).flatMap((o) => {
      const opt = asSchema(o);
      if (!opt || typeof opt.const !== "string") return [];
      return [{ value: opt.const, label: typeof opt.title === "string" ? opt.title : opt.const }];
    });
  }
  if (Array.isArray(s.enum)) {
    const names = Array.isArray(s.enumNames) ? (s.enumNames as unknown[]) : [];
    return (s.enum as unknown[]).map((v, i) => ({
      value: String(v),
      label: typeof names[i] === "string" ? (names[i] as string) : String(v),
    }));
  }
  return undefined;
}

export function formFields(schemaText: string | null): FormField[] {
  const schema = asSchema(parseJson(schemaText));
  const props = asSchema(schema?.properties);
  if (!props) return [];
  const required = new Set(Array.isArray(schema?.required) ? (schema.required as string[]) : []);
  return Object.entries(props).flatMap(([name, raw]): FormField[] => {
    const s = asSchema(raw);
    if (!s) return [];
    const base = {
      name,
      label: typeof s.title === "string" ? s.title : name,
      description: typeof s.description === "string" ? s.description : undefined,
      required: required.has(name),
      defaultValue: s.default as Json | undefined,
    };
    const type = s.type;
    if (type === "array") {
      const items = asSchema(s.items);
      return [{ ...base, kind: "multiselect" as const, options: items ? options(items) : [] }];
    }
    const opts = options(s);
    if (opts && opts.length > 0) return [{ ...base, kind: "select" as const, options: opts }];
    if (type === "boolean") return [{ ...base, kind: "boolean" as const }];
    if (type === "integer" || type === "number") {
      return [
        {
          ...base,
          kind: type,
          min: typeof s.minimum === "number" ? s.minimum : undefined,
          max: typeof s.maximum === "number" ? s.maximum : undefined,
        },
      ];
    }
    return [
      {
        ...base,
        kind: "text" as const,
        format: typeof s.format === "string" ? s.format : undefined,
      },
    ];
  });
}

/** Text inputs → typed form content, or a message naming the first problem. */
export function formContent(
  fields: FormField[],
  values: Record<string, string | boolean | string[]>,
): { content: Record<string, Json> } | { error: string } {
  const content: Record<string, Json> = {};
  for (const f of fields) {
    const v = values[f.name];
    const empty = v === undefined || v === "" || (Array.isArray(v) && v.length === 0);
    if (empty) {
      if (f.required && f.kind !== "boolean") return { error: `${f.label} is required.` };
      if (f.kind === "boolean") content[f.name] = false;
      continue;
    }
    if (f.kind === "integer" || f.kind === "number") {
      const n = Number(v);
      if (!Number.isFinite(n) || (f.kind === "integer" && !Number.isInteger(n))) {
        return {
          error: `${f.label} must be ${f.kind === "integer" ? "a whole number" : "a number"}.`,
        };
      }
      content[f.name] = n;
    } else if (f.kind === "boolean") {
      content[f.name] = v === true;
    } else if (f.kind === "multiselect") {
      content[f.name] = Array.isArray(v) ? v : [String(v)];
    } else {
      content[f.name] = String(v);
    }
  }
  return { content };
}

// --- Config form text fields ------------------------------------------------------------

/** `KEY=value` lines (env) or `Name: value` lines (headers) → a map. Blank lines skip. */
export function parsePairs(text: string, sep: "=" | ":"): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const i = line.indexOf(sep);
    if (i <= 0) continue;
    out[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  return out;
}

export function formatPairs(map: Partial<Record<string, string>>, sep: "=" | ":"): string {
  return Object.entries(map)
    .map(([k, v]) => (sep === "=" ? `${k}=${v ?? ""}` : `${k}: ${v ?? ""}`))
    .join("\n");
}

/** One argument per line, so paths with spaces need no quoting. */
export function parseArgs(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
}
