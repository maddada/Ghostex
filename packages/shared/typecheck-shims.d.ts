/*
 * CDXC:Typecheck 2026-04-26-03:25
 * Three and vite-plus are not installed in this native repo by default, but
 * some source files still reference their modules. These ambient declarations
 * keep repo-wide typecheck focused on local source correctness.
 */
declare module "three" {
  const three: any;
  export = three;
}

declare module "vite-plus/test" {
  export * from "vitest";
}
