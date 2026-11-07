// Packages the artifact frame loads that ship without type declarations.

declare module "@babel/standalone" {
  type Transform = (code: string, options: Record<string, unknown>) => { code?: string | null };
  export const transform: Transform | undefined;
  const babel: { transform?: Transform } | undefined;
  export default babel;
}

declare module "@tailwindcss/browser";
