import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

const host = process.env.TAURI_DEV_HOST;

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "chrome120",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    include: ["src/**/*.test.{ts,tsx}", "e2e/**/*.test.{ts,tsx}"],
    // A stuck hook or teardown fails instead of hanging the run (a single test already has 5 s).
    hookTimeout: 30_000,
    teardownTimeout: 15_000,
    // In CI, name each test as it finishes (a hung file is where the log stops) and, at the end,
    // whatever keeps a worker alive.
    reporters: process.env.CI ? ["verbose", "hanging-process"] : ["default"],
  },
});
