// Writes `src/appframe/oxide-sdk.d.ts`: the types of the `oxide` module Utility Apps import,
// generated from `src/appframe/sdk.ts` (Phase 21). The app builder gives them to its model
// (`crates/oxide-agent/src/apps/builder.rs` compiles the file in), so they must match the SDK:
// `npm run sdk-types` regenerates the file, and a Vitest test fails when it is stale.
//
// The frame's own wiring (`connect`, `deliver`, `setTheme`) is left out: apps don't call it.
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import * as prettier from "prettier";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const SDK_FILE = path.join(ROOT, "src/appframe/sdk.ts");
export const DTS_FILE = path.join(ROOT, "src/appframe/oxide-sdk.d.ts");

/** Statements apps don't use (the frame calls them). */
const INTERNAL = new Set(["connect", "deliver", "setTheme", "Post"]);

const HEADER = `// Types of the \`oxide\` module that Utility Apps import (generated from sdk.ts by
// scripts/sdk-dts.mjs; run \`npm run sdk-types\` after changing the SDK; don't edit by hand).
`;

function declarations() {
  const out = {};
  const options = {
    declaration: true,
    emitDeclarationOnly: true,
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    strict: true,
    skipLibCheck: true,
    noEmitOnError: false,
    removeComments: false,
    types: [],
    lib: ["lib.es2022.d.ts", "lib.dom.d.ts"],
  };
  const host = ts.createCompilerHost(options);
  host.writeFile = (name, text) => {
    out[path.basename(name)] = text;
  };
  ts.createProgram([SDK_FILE], options, host).emit();
  const sdk = out["sdk.d.ts"];
  const protocol = out["protocol.d.ts"];
  if (!sdk || !protocol) throw new Error("tsc emitted no declarations for sdk.ts");
  return { sdk, protocol };
}

function nameOf(stmt) {
  if (stmt.name && ts.isIdentifier(stmt.name)) return stmt.name.text;
  if (ts.isVariableStatement(stmt)) {
    const d = stmt.declarationList.declarations[0];
    if (d && ts.isIdentifier(d.name)) return d.name.text;
  }
  return undefined;
}

/** The SDK's declarations as one self-contained module, formatted like the repo. */
export async function sdkTypes() {
  const { sdk, protocol } = declarations();
  const errorCode = protocol.match(/export type SdkErrorCode =[^;]+;/);
  if (!errorCode) throw new Error("SdkErrorCode is missing from protocol.ts");
  const file = ts.createSourceFile("sdk.d.ts", sdk, ts.ScriptTarget.ES2022, true);
  const parts = [];
  for (const stmt of file.statements) {
    if (ts.isImportDeclaration(stmt)) {
      // The file's own comment sits before the import; the type it imported goes here.
      parts.push(
        sdk.slice(stmt.getFullStart(), stmt.getStart()).replace(/^\n+/, "") + errorCode[0],
      );
      continue;
    }
    const name = nameOf(stmt);
    if (name && INTERNAL.has(name)) continue;
    parts.push(sdk.slice(stmt.getFullStart(), stmt.end).replace(/^\n+/, ""));
  }
  const text = HEADER + "\n" + parts.join("\n");
  const config = (await prettier.resolveConfig(DTS_FILE)) ?? {};
  return prettier.format(text, { ...config, parser: "typescript" });
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const text = await sdkTypes();
  let old = "";
  try {
    old = readFileSync(DTS_FILE, "utf8");
  } catch {
    // first run
  }
  if (old !== text) {
    writeFileSync(DTS_FILE, text);
    console.log("Wrote src/appframe/oxide-sdk.d.ts");
  } else {
    console.log("src/appframe/oxide-sdk.d.ts is up to date");
  }
}
