# 開發與提交

## 本機開發

環境需求與啟動方式見 [README](README.md#開發與打包)。先執行 `npm ci`，再用 `npm run desktop` 啟動。

- `src/`、`index.html`：HTML/CSS/JavaScript 介面與修正結果。
- `src-tauri/src/main.rs`：快捷鍵、API、限流與選單列。
- `src-tauri/src/macos.rs`：macOS 浮窗與輔助使用取字。
- `src-tauri/src/selection.rs`：平台取字入口。
- `tests/`：修正邏輯與模擬 IPC 的介面測試。

## 變更與驗證

從 `codex/簡短描述` 分支開始，每次提交聚焦一個目的。避免格式化或改動無關檔案。

依變更範圍執行必要檢查，並在 PR 說明結果與未驗證的平台：

```sh
npm test
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

介面互動有變更時，可執行 `npx playwright install chromium` 和 `npm run test:ui`。只改文件或 Git 設定不必重新編譯 App。

模擬 IPC 測試不能證明跨 App 取字、全螢幕、權限、焦點或選單列行為。涉及原生整合時，需說明實機驗證範圍；不要將編譯通過寫成完整功能通過。

## 版本與依賴

提交 `package-lock.json` 與 `src-tauri/Cargo.lock`；修改依賴時同步更新對應 lockfile。

發版時同步更新 `package.json`、`package-lock.json` 的專案版本、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 的專案版本、`src-tauri/tauri.conf.json`、`index.html` 的版本提示及 README。不要全域取代 lockfile 中其他依賴的版本。

## 資料與機密

不要提交使用者選取文字、真實聊天截圖、API 金鑰、簽章私鑰或憑證。測試使用合成文字；不自動呼叫 LanguageTool 公共服務。程式僅送出使用者主動要求檢查的文字，不加入背景掃描或自動操作剪貼簿。

`.gitignore` 不能移除已追蹤檔案或清理 Git 歷史中的機密。若誤提交憑證，先撤銷／輪換，再處理紀錄。

推送流程見 [Git 使用說明](docs/GIT.md)。
