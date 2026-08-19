import react from "@vitejs/plugin-react";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";
import { sandboxDevCertificate } from "./dev-cert";
import { sandboxYouTubeProxy } from "./yt-proxy";

const sandboxRoot = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = path.resolve(sandboxRoot, "..", "..", "..");
const monacoVsSource = path.join(repoRoot, "node_modules", "monaco-editor", "min", "vs");

/*
 * The Agents Hub modal loads Monaco at runtime from /monaco/vs via the AMD
 * loader (same reason as ghostex-web/vite.config.ts): serve it straight from
 * node_modules in dev. Dev-server-only app, so no build-time copy is needed.
 */
function sandboxMonacoVs(): Plugin {
  const contentTypeFor = (filePath: string): string => {
    if (filePath.endsWith(".js")) return "text/javascript";
    if (filePath.endsWith(".css")) return "text/css";
    if (filePath.endsWith(".json")) return "application/json";
    if (filePath.endsWith(".ttf")) return "font/ttf";
    if (filePath.endsWith(".svg")) return "image/svg+xml";
    return "application/octet-stream";
  };
  return {
    name: "onboarding-sandbox-monaco-vs",
    configureServer(server) {
      server.middlewares.use("/monaco/vs", (request, response, next) => {
        const requestPath = (request.url ?? "").split("?", 1)[0];
        const filePath = path.join(monacoVsSource, requestPath);
        if (
          !filePath.startsWith(monacoVsSource) ||
          !fs.existsSync(filePath) ||
          !fs.statSync(filePath).isFile()
        ) {
          next();
          return;
        }
        response.setHeader("content-type", contentTypeFor(filePath));
        fs.createReadStream(filePath).pipe(response);
      });
    },
  };
}

export default defineConfig({
  root: sandboxRoot,
  plugins: [react(), sandboxMonacoVs(), sandboxYouTubeProxy()],
  resolve: {
    alias: {
      "@": repoRoot,
    },
    dedupe: ["react", "react-dom"],
  },
  server: {
    // Bind IPv4 explicitly: the default ("localhost") resolves to ::1 only on
    // this machine, which makes https://127.0.0.1:5199/ (and curl) fail.
    host: "127.0.0.1",
    /*
     * TLS turns the dev server into an HTTP/2 server (vite uses
     * http2.createSecureServer when `server.https` is set and no `server.proxy`
     * exists — this config proxies through middleware, not `server.proxy`).
     * HTTP/2 is required for YouTube's media requests, which Chrome only sends
     * with a streaming body over h2+. See dev-cert.ts and README.md.
     */
    https: sandboxDevCertificate(sandboxRoot) ?? undefined,
    port: 5199,
    fs: {
      allow: [repoRoot],
    },
  },
});
