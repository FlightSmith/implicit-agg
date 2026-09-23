import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  // WebGL canvases under parallel headless workers contend for the GPU and
  // make timing-sensitive specs flaky; one worker keeps them deterministic.
  workers: 1,
  use: {
    ...devices["Desktop Chrome"],
    baseURL: "http://localhost:4173",
    viewport: { width: 1440, height: 900 },
  },
  webServer: {
    command: "npm run preview",
    port: 4173,
    reuseExistingServer: true,
  },
});
