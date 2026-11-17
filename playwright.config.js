import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests", testMatch: "**/*.spec.js", workers: 1,
  use: { baseURL: "http://127.0.0.1:1420", viewport: { width: 360, height: 240 },
    launchOptions: process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {} },
  webServer: { command: "npm run dev", url: "http://127.0.0.1:1420", reuseExistingServer: false },
});
