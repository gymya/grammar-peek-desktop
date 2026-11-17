# Grammar Peek Desktop

獨立的 Rust / Tauri 2 桌面專案，介面使用原生 HTML、CSS、JavaScript。沿用 Grammar Peek Chrome 擴充功能的修正合併規則；原擴充功能不受影響。

## 使用（0.1.12）

這是一個在目前 App 上方顯示的 **360 × 240 小浮窗**，不再使用大型主視窗。反白文字後按 **⌘⇧G（macOS）／Ctrl+Shift+G（Windows、Linux X11）**，浮窗出現在按快捷鍵時的滑鼠附近，並保持在螢幕範圍內。macOS 使用 `orderFrontRegardless`，不會主動切換來源 App 的焦點；不顯示 Dock 圖示。

綠底顯示新增與替換，滑鼠停留可查看原文，刪除文字另外列出。按「複製結果」後自行貼回來源 App。macOS 點擊浮窗外也會隱藏浮窗；× 會隱藏浮窗並保留快捷鍵；浮窗取得鍵盤焦點時也可按 Esc。展開「貼上文字」可手動檢查，支援 ⌘／Ctrl+Enter。程式常駐選單列／系統匣；關閉視窗只會隱藏，快捷鍵繼續有效。從選單列／系統匣選「顯示浮窗」可重新開啟，選「結束程式」才會退出。

### 系統支援與取字權限

- **macOS**：使用 Rust 直接呼叫原生 Accessibility API，不再依賴 AppleScript 或 System Events。首次請按浮窗中的「啟用取字權限」，在「系統設定 → 隱私權與安全性 → 輔助使用」允許 **Grammar Peek**。建議先把新版 App 放入 Applications 再授權，避免路徑／版本更換使系統要求重新授權。權限只能由使用者授予。
- 優先讀取聚焦元素的 AXSelectedText；網頁型 App 可再搜尋目前視窗的選取元素。搜尋限制為 256 個元素／約 4 秒，避免大型頁面卡住。先向來源 App 發送 Chromium/Electron 文件指定的輔助使用啟用訊號；支援的文字欄位也可透過 AXValue 與選取範圍在本機擷取反白片段，僅傳送片段。不讀取密碼欄位，不模擬複製，不覆寫原剪貼簿。如果 GPT 或其他 App 沒有公開可讀的選取內容，浮窗會提示手動複製後貼上，不會改讀整頁或舊剪貼簿。
- **Windows**：使用內建 Windows PowerShell 與 UI Automation TextPattern。未實作此介面的 App、密碼欄位及較高權限視窗可能無法讀取，請改用貼上。
- **Linux X11**：需 `xclip`（Debian/Ubuntu：`sudo apt install xclip`），讀取 PRIMARY selection，取決於來源 App 是否更新選取緩衝區。
- **Linux Wayland**：可使用浮窗內貼上、檢查和複製；本版不支援全域快捷鍵取字。

沒有自動開機啟動、背景掃描、直接改寫其他 App 或檢查歷史。

## 開發與打包

需要 Node.js 22.12+、最新版 stable Rust（本次驗證為 1.100.0），及各系統的 [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)。macOS 需 Xcode Command Line Tools；Windows 需 MSVC C++ build tools 與 WebView2；Linux 需 WebKitGTK 4.1 等套件（參考 CI）。

```sh
npm ci
npm run desktop
```

```sh
npm test
npx playwright install chromium
npm run test:ui
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run tauri -- build
```

安裝檔位於 `src-tauri/target/release/bundle/`。GitHub Actions 在三個原生作業系統上測試、打包並保存 artifacts，不會發布 release。macOS 建置架構取決於 runner／本機；若要 Intel 可用對應 Intel runner 或安裝 Rust x86_64 target 後指定建置 target。正式散布需另設 Apple 簽章／公證及 Windows 簽章；目前沒有簽章憑證。

## 資料與 API

只在手動快捷鍵或檢查按鈕觸發時將文字 POST 到 `https://api.languagetool.org/v2/check`。不傳送來源 App 名稱、網址或整份文件；App 不儲存文字、歷史或請求日誌。額度計數只在記憶體中，退出即清空。HTTPS 由 Rust 發送，不允許重新導向；20 秒逾時，不自動重試。

每次最多 20,000 UTF-8 bytes；本 App 限制每分鐘 15 次／70,000 bytes。公共服務仍依 IP 計算，與其他程式共用額度；不保證服務可用性或檢查完整度。語言自動偵測，英文採美式拼字。

[LanguageTool](https://languagetool.org) · [公開 API 規則](https://dev.languagetool.org/public-http-api) · [服務隱私政策](https://languagetool.org/legal/privacy)

## 驗收

自動測試覆蓋修正偏移、UTF-16、插入刪除、重疊建議、UTF-8 上限、限流與回應結構。各平台實機還需確認：一般文字編輯器選取、拒絕權限、沒有選取、空文字、中文與 emoji、斷網、隱藏後快捷鍵、快捷鍵衝突、檢查途中改字，以及複製結果。自動測試不呼叫公開 API。

## 現行架構與安裝

- HTML/CSS/JS：360 × 240 介面、修正標記、複製與錯誤提示。
- Rust：快捷鍵、取字、LanguageTool HTTPS 請求、輸入驗證與限流。
- `src-tauri/src/macos.rs`：公開 AppKit API 建立 nonactivating NSPanel；直接用 AppKit 滑鼠位置、螢幕可用範圍與面板大小定位，不再同步隱藏視窗座標。顯示／隱藏一律操作同一個 Panel。
- Tauri：WebView 與 IPC 的生命週期。macOS 將內容視圖移入 NSPanel；原 Tauri 視窗僅保留作為 WebView/IPC 宿主，不參與浮窗定位或顯示。這是客製整合，升級 Tauri 時需驗證焦點與 WebView 行為。
- macOS 13+ 使用 CanJoinAllApplications；舊版保留 FullScreenAuxiliary 相容處理，全螢幕效果需實機確認。Windows/Linux 使用各自平台的 Tauri 視窗。

唯一日常執行位置：`/Applications/Grammar Peek.app`。不要同時執行 target 或其他舊副本。
目前為完整 ad-hoc 簽章，未做 Developer ID 簽章／公證；更新可能需要使用者重新授予輔助使用權限。

本輪僅執行原生定位的小型測試、正式編譯與簽章檢查。全螢幕選字快捷鍵、多螢幕、鍵盤焦點與 Windows/Linux 實機驗收交由使用者進行，不宣稱已通過。

## Repository 文件

- [開發與貢獻指南](CONTRIBUTING.md)
- [Git 與首次推送](docs/GIT.md)
- [Pull request 範本](.github/pull_request_template.md)

原始碼與兩份 lockfile 應提交；依賴目錄、建置產物、安裝檔、測試輸出及機密由 `.gitignore` 排除。跨平台文字換行由 `.gitattributes` 統一。尚未選定開源授權。
