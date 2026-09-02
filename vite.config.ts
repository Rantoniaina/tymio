import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    // In a packaged app the front end talks to Rust over Tauri's IPC. The
    // Playwright suite has no Tauri, so it points `invoke` at /ipc and this
    // proxy forwards to `cargo run --example devserver`, which calls the same
    // command layer. Dev-server only — `vite build` never sees it.
    proxy: {
      "/ipc": {
        target: process.env.TYMIO_DEVSERVER_URL || "http://127.0.0.1:4599",
        changeOrigin: false,
      },
    },
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
