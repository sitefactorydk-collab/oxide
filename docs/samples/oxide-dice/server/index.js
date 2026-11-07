#!/usr/bin/env node
// Oxide Dice: a tiny MCP server with no dependencies, packed as a desktop extension (.mcpb)
// to test Oxide's extension support. Speaks newline-delimited JSON-RPC over stdio with the
// `initialize` handshake (MCP 2025-06-18); unknown methods get "method not found", so newer
// clients fall back to the handshake.
"use strict";

const readline = require("node:readline");

const greeting = process.env.DICE_GREETING || "Hello";
const lucky = Number(process.env.DICE_LUCKY || "7");
const shout = process.env.DICE_SHOUT === "true";
const token = process.env.DICE_TOKEN || "";

const tools = [
  {
    name: "roll_dice",
    description: "Roll dice. Returns each roll and the total.",
    inputSchema: {
      type: "object",
      properties: {
        sides: { type: "integer", minimum: 2, maximum: 1000, description: "Sides per die (default 6)." },
        count: { type: "integer", minimum: 1, maximum: 100, description: "How many dice (default 1)." },
      },
    },
    annotations: { title: "Roll dice", readOnlyHint: true },
  },
  {
    name: "greet",
    description: "Greet someone using the extension's greeting setting.",
    inputSchema: {
      type: "object",
      properties: { name: { type: "string", description: "Who to greet." } },
      required: ["name"],
    },
    annotations: { title: "Greet", readOnlyHint: true },
  },
  {
    name: "settings_check",
    description:
      "Report which settings the extension received (the token is never shown, only whether it's set and its length).",
    inputSchema: { type: "object", properties: {} },
    annotations: { title: "Check settings", readOnlyHint: true },
  },
];

function send(msg) {
  process.stdout.write(JSON.stringify(msg) + "\n");
}

function text(t, isError = false) {
  return { content: [{ type: "text", text: t }], isError };
}

function call(name, args) {
  switch (name) {
    case "roll_dice": {
      const sides = Math.min(1000, Math.max(2, Math.floor(args.sides ?? 6)));
      const count = Math.min(100, Math.max(1, Math.floor(args.count ?? 1)));
      const rolls = Array.from({ length: count }, () => 1 + Math.floor(Math.random() * sides));
      const total = rolls.reduce((a, b) => a + b, 0);
      const luck = rolls.includes(lucky) ? ` Your lucky number ${lucky} came up!` : "";
      return text(`Rolled ${count}d${sides}: ${rolls.join(", ")} (total ${total}).${luck}`);
    }
    case "greet": {
      const who = String(args.name ?? "").trim() || "friend";
      const line = `${greeting}, ${who}!`;
      return text(shout ? line.toUpperCase() : line);
    }
    case "settings_check":
      return text(
        [
          `greeting: ${greeting}`,
          `lucky number: ${lucky}`,
          `shout: ${shout}`,
          `token: ${token ? `set (${token.length} characters)` : "not set"}`,
          `folder: ${process.cwd()}`,
        ].join("\n"),
      );
    default:
      return null;
  }
}

function handle(msg) {
  if (msg.id === undefined || msg.id === null) return; // a notification
  const { id, method, params = {} } = msg;
  switch (method) {
    case "initialize":
      return send({
        jsonrpc: "2.0",
        id,
        result: {
          protocolVersion: "2025-06-18",
          capabilities: { tools: {} },
          serverInfo: { name: "oxide-dice", version: "1.0.0" },
          instructions: "Dice and greetings, for testing Oxide extensions.",
        },
      });
    case "ping":
      return send({ jsonrpc: "2.0", id, result: {} });
    case "tools/list":
      return send({ jsonrpc: "2.0", id, result: { tools } });
    case "tools/call": {
      const res = call(params.name, params.arguments ?? {});
      if (!res) {
        return send({ jsonrpc: "2.0", id, error: { code: -32602, message: `Unknown tool: ${params.name}` } });
      }
      return send({ jsonrpc: "2.0", id, result: res });
    }
    default:
      return send({ jsonrpc: "2.0", id, error: { code: -32601, message: `Method not found: ${method}` } });
  }
}

readline.createInterface({ input: process.stdin }).on("line", (line) => {
  if (!line.trim()) return;
  try {
    handle(JSON.parse(line));
  } catch (e) {
    send({ jsonrpc: "2.0", id: null, error: { code: -32700, message: `Parse error: ${e.message}` } });
  }
});
