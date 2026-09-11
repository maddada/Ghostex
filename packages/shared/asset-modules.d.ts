declare module '*.css' {
  const cssUrl: string;
  export default cssUrl;
  /*
   * CDXC:Build 2026-04-27-17:03
   * Native sidebar entrypoints import bundled CSS for side effects. Keep CSS
   * modules typed in the root project so editor checks do not treat styling
   * imports as missing runtime modules.
   */
}

declare module '*.svg' {
  const svgUrl: string;
  export default svgUrl;
}

declare module '*.webp' {
  const webpUrl: string;
  export default webpUrl;
}

declare module '*.png' {
  const pngUrl: string;
  export default pngUrl;
  /*
   * CDXC:Onboarding 2026-05-26-06:23
   * First-launch onboarding uses generated raster artwork inside the production
   * modal and Storybook renders that same component. Keep PNG imports typed so
   * the image asset can be bundled instead of referenced through a mock-only path.
   */
}

declare module '*.ttf' {
  const fontUrl: string;
  export default fontUrl;
  /*
   * CDXC:Onboarding 2026-09-11 WHY:
   * The onboarding modal ships its own Manrope, DM Sans and IBM Plex Mono files and registers them with runtime
   * @font-face rules built from these imports, because CSS url() assets get emitted beside the CSS file while the
   * CEF build inlines the stylesheet into the HTML entry, which breaks relative font paths. JS imports become data
   * URLs in that bundle (see the '.ttf' loader in apps/desktop/vite.config.ts) and plain asset URLs in Vite/Storybook.
   */
}
