declare module "*.css" {
  const cssUrl: string;
  export default cssUrl;
  /*
   * CDXC:Typecheck 2026-04-27-17:03
   * Native sidebar entrypoints import bundled CSS for side effects. Keep CSS
   * modules typed in the root project so editor checks do not treat styling
   * imports as missing runtime modules.
   */
}

declare module "*.svg" {
  const svgUrl: string;
  export default svgUrl;
}

declare module "*.webp" {
  const webpUrl: string;
  export default webpUrl;
}

declare module "*.png" {
  const pngUrl: string;
  export default pngUrl;
  /*
   * CDXC:FirstLaunchSetup 2026-05-26-06:23
   * First-launch onboarding uses generated raster artwork inside the production
   * modal and Storybook renders that same component. Keep PNG imports typed so
   * the image asset can be bundled instead of referenced through a mock-only path.
   */
}
