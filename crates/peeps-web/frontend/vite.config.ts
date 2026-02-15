import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 9131,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:9130",
      },
      "/health": {
        target: "http://127.0.0.1:9130",
      },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
