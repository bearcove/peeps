import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const backendTarget = process.env.PEEPS_HTTP_PROXY_TARGET ?? "http://127.0.0.1:9130";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 9131,
    proxy: {
      "/api": {
        target: backendTarget,
        ws: false,
      },
      "/health": {
        target: backendTarget,
        ws: false,
      },
      "/favicon.svg": {
        target: backendTarget,
        ws: false,
      },
      "/favicon.ico": {
        target: backendTarget,
        ws: false,
      },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
