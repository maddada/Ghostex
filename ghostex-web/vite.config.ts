import { transformAsync } from "@babel/core";
import react from "@vitejs/plugin-react";
import reactCompiler from "babel-plugin-react-compiler";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import { defineConfig, type Plugin } from "vite";

const webRoot = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = path.resolve(webRoot, "..");

function ghostexReactCompiler(): Plugin {
  return {
    enforce: "pre",
    name: "ghostex-web-react-compiler",
    async transform(code, id) {
      const filename = id.split("?", 1)[0];
      if (!filename.startsWith(webRoot) || !/\.[jt]sx$/.test(filename)) {
        return null;
      }
      const result = await transformAsync(code, {
        babelrc: false,
        configFile: false,
        filename,
        parserOpts: { plugins: ["jsx", "typescript"] },
        plugins: [[reactCompiler, {}]],
        sourceMaps: true,
      });
      return result?.code ? { code: result.code, map: result.map } : null;
    },
  };
}

export default defineConfig({
  root: webRoot,
  plugins: [
    tanstackRouter({
      autoCodeSplitting: true,
      generatedRouteTree: "./src/routeTree.gen.ts",
      routesDirectory: "./src/routes",
      target: "react",
    }),
    ghostexReactCompiler(),
    react(),
  ],
  resolve: {
    alias: {
      "@": repoRoot,
    },
  },
  build: {
    emptyOutDir: true,
    outDir: path.resolve(webRoot, "dist"),
  },
  server: {
    proxy: {
      "/api": {
        changeOrigin: true,
        configure(proxy) {
          const stripBootstrapOrigin = (
            proxyRequest: { removeHeader(name: string): void },
            request: { url?: string },
          ) => {
            if (request.url?.startsWith("/api/webBootstrap")) {
              proxyRequest.removeHeader("origin");
            }
          };
          proxy.on("proxyReq", stripBootstrapOrigin);
          proxy.on("proxyReqWs", stripBootstrapOrigin);
        },
        target: "http://127.0.0.1:58744",
        ws: true,
      },
    },
  },
});
