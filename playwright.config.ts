import { defineConfig, devices } from "@playwright/test";

/**
 * End-to-end configuration.
 *
 * Playwright cannot drive the Tauri window itself — the app renders in
 * WKWebView on macOS and there is no WebDriver for it. So the suite loads the
 * real front end in Chromium and points its IPC at `examples/devserver.rs`,
 * which calls the same `AppState` methods the Tauri commands do, against a
 * real migrated SQLite database. React, the command layer, the repository,
 * the schema and the domain rules are all genuinely under test; Tauri's IPC
 * transport and the three platform webviews are not.
 */

const DEVSERVER_PORT = 4599;
const WEB_PORT = 5199;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false, // one database, one dev server
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : [["list"]],

  use: {
    baseURL: `http://localhost:${WEB_PORT}`,
    trace: "on-first-retry",
  },

  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],

  webServer: [
    {
      // A GET on any command works too, so this doubles as the readiness probe.
      command: "cargo run --quiet --example devserver",
      cwd: "src-tauri",
      url: `http://127.0.0.1:${DEVSERVER_PORT}/ipc/portfolio_stats`,
      reuseExistingServer: !process.env.CI,
      timeout: 300_000, // a cold cargo build is minutes
      stdout: "pipe",
    },
    {
      command: `npm run dev -- --port ${WEB_PORT} --strictPort`,
      url: `http://localhost:${WEB_PORT}`,
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
    },
  ],
});
