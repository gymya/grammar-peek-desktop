# Windows 版 (0.2.0)

目標為 Windows 10/11 x64。介面與 LanguageTool 流程共用現有專案，不使用 macOS NSPanel。

## 安裝與使用

執行 `Grammar Peek_0.2.0_x64-setup.exe`。安裝於目前使用者帳號；若缺少 WebView2 Runtime，安裝程式會嘗試連網下載安裝。

- 在來源 App 反白一段連續文字，按 **Ctrl+Shift+G**。
- 浮窗顯示時不搶焦點；點擊手動輸入區後仍能打字。
- 點擊外部或關閉按鈕只隱藏浮窗，程式留在右下角系統匣；圖示可能位於隱藏圖示區。
- 系統匣選單可重新顯示或真正結束程式。
- 只有「複製結果」會寫入剪貼簿；取字不模擬 Ctrl+C，也不讀取舊剪貼簿。

## 實作

`src-tauri/src/windows_platform.rs` 負責來源 HWND/PID、滑鼠位置、DPI 與螢幕工作區定位、外部點擊監聽及結束時的清理。

`src-tauri/src/windows_platform/uia.rs` 使用 UI Automation TextPattern，在不持有視窗的 COM MTA 程序內讀取選取範圍。優先查詢來源視窗內的聚焦與滑鼠元素，再有限度搜尋子元素。

取字使用同一執行檔的內部 `--read-selection` 模式，不啟動 WebView 或系統匣。結果僅經匿名 pipe 回傳、不寫檔；主程式在 4 秒逾時後終止並回收子程序，避免來源 App 卡住整個程式。

最多讀取 20 KB；拒絕密碼欄位、多重選取、空文字及讀取期間切換來源視窗的結果。不會改讀整份文件或自動提權。

## 限制

來源 App 必須公開 UI Automation 選取資訊。Brave、ChatGPT、VS Code 各區域仍需實機驗證；VS Code 可先開啟輔助使用支援。一般權限程式不保證能讀取管理員 App。

遊戲獨佔全螢幕、安全桌面和鎖定畫面不在浮窗支援範圍。快捷鍵被占用時保留手動貼上功能並顯示提示。

本版沒有 Authenticode 簽章，可能顯示未知發行者或 SmartScreen 提示。正式發布前仍需 Windows 原生建置、實機驗收和簽章。

## Windows 原生建置

需要 Node.js 22.12+、Rust stable MSVC、Visual Studio C++ Build Tools (含 Windows SDK) 及 WebView2。

```powershell
npm ci
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis
```

產物位於 `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`。Tauri CLI 自動合併 `src-tauri/tauri.windows.conf.json`。

現有 GitHub Actions 包含 Windows 原生建置工作；本輪沒有推送或啟動遠端 CI。

## 本輪驗證

完成前端編譯、共用程式 macOS 編譯檢查、Windows x64 交叉編譯與連結、NSIS 打包，以及 Windows 單元測試的編譯。沒有執行 Windows 測試程式、操作 Windows App 或測試安裝器。
