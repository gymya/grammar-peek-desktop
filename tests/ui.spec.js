import { test, expect } from "@playwright/test";
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const callbacks = new Map(); const events = new Map(); let id = 0;
    window.mock = { matches: [], calls: [], delay: 0, error: "", emit: (event, payload) => callbacks.get(events.get(event))?.({ payload }) };
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      transformCallback: fn => { callbacks.set(++id, fn); return id; },
      invoke: async (cmd, args) => {
        window.mock.calls.push({ cmd, args });
        if (cmd === "plugin:event|listen") { events.set(args.event, args.handler); return args.handler; }
        if (cmd === "setup_status") return { shortcut: "⌘ + Shift + G", notice: "" };
        if (cmd === "check_text") {
          const matches = window.mock.matches;
          await new Promise(resolve => setTimeout(resolve, window.mock.delay));
          if (window.mock.error) throw window.mock.error;
          return matches;
        }
      },
    };
  });
  await page.goto("/");
  await expect(page.locator("#shortcut")).toHaveText("⌘ + Shift + G");
  await page.locator("#editor summary").click();
});
test("shortcut checks selection, highlights changes, and copies only plain result", async ({ page }) => {
  await page.evaluate(() => {
    window.mock.matches = [{ offset: 5, length: 3, replacements: [{ value: "is" }] }];
    window.mock.emit("selected-text", "This are a test.");
  });
  await expect(page.locator("#result")).toHaveText("This is a test.");
  await expect(page.locator("mark")).toHaveAttribute("title", "原文：are");
  await expect(page.locator("#copy")).toBeInViewport();
  await expect(page.locator("#status")).toBeHidden();
  await page.getByRole("button", { name: "複製結果" }).click();
  expect(await page.evaluate(() => window.mock.calls.find(c => c.cmd === "plugin:clipboard-manager|write_text").args.text)).toBe("This is a test.");
  await page.screenshot({ path: "test-results/desktop-preview.png", fullPage: true });
});
test("editing while checking discards stale results", async ({ page }) => {
  await page.evaluate(() => { window.mock.delay = 350; });
  await page.locator("#input").fill("Original.");
  await page.locator("#check").click();
  await page.locator("#input").fill("New text.");
  await expect(page.locator("#status")).toHaveText("原文已變更，請重新檢查。");
  await expect(page.locator("#result-card")).toBeHidden();
});
test("renders HTML-like content as text and surfaces API errors", async ({ page }) => {
  await page.locator("#input").fill('<img src=x onerror="window.injected=true">');
  await page.locator("#check").click();
  await expect(page.locator("#result")).toContainText("<img");
  await expect(page.locator("#result img")).toHaveCount(0);
  await page.evaluate(() => { window.mock.error = "LanguageTool 額度已達上限"; });
  await page.locator("#check").click();
  await expect(page.locator("#status")).toHaveText("LanguageTool 額度已達上限");
  await expect(page.locator("#check")).toBeEnabled();
  await expect(page.locator("#result-card")).toBeHidden();
});
test("empty input never sends a request; capture failure preserves existing input", async ({ page }) => {
  await page.locator("#check").click();
  expect(await page.evaluate(() => window.mock.calls.filter(c => c.cmd === "check_text"))).toHaveLength(0);
  await page.locator("#input").fill("Keep this.");
  await page.evaluate(() => window.mock.emit("selection-error", "無法讀取選取文字"));
  await expect(page.locator("#input")).toHaveValue("Keep this.");
  await expect(page.locator("#status")).toHaveText("無法讀取選取文字");
});

test("selection failure is visible at the top of a compact popup", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 240 });
  await page.evaluate(() => window.mock.emit("selection-error", "需要輔助使用權限"));
  await expect(page.locator("#status")).toBeInViewport();
  await expect(page.locator("#status")).toHaveClass("error");
});

test("close and Escape hide the native popup through the backend", async ({ page }) => {
  await page.getByRole("button", { name: "關閉浮窗" }).click();
  await page.keyboard.press("Escape");
  expect(await page.evaluate(() => window.mock.calls.filter(c => c.cmd === "hide_popup"))).toHaveLength(2);
});
