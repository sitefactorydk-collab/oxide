# Built-in Utility Apps

Each folder is one app that ships with Oxide (compiled into the binary by
`crates/oxide-agent/src/apps/builtin.rs`, which lists the files). An app is an
`oxide-app.json` manifest plus React + TypeScript files; the entry file's default export is
the app. Apps import only `react`, `lucide-react`, `react-markdown`, `remark-gfm`, `jszip` and
`oxide` (the SDK in `src/appframe/sdk.ts`), and their own files.

`npm run typecheck` checks these apps against the SDK's types (`tsconfig.json` here).
