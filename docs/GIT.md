# Git 與首次推送

## 推送前

```sh
git status --short --untracked-files=all
git diff --check
```

確認只包含原始碼、測試、設定、文件、圖示與 lockfile。`node_modules/`、`dist/`、`src-tauri/target/`、`releases/`、測試輸出、環境變數檔與簽章私鑰不應提交。

新增檔案在暫存前不會出現在一般 `git diff` 中。暫存後再檢查一次：

```sh
git add .
git diff --cached --stat
git diff --cached --check
git diff --cached
```

`.gitignore` 只控制未追蹤檔案。可用 `git check-ignore -v 路徑` 確認規則；不要以 `git add -f` 強制加入建置產物或機密。

## 第一次推送

在 GitHub／GitLab 建立空白 repository，避免先產生另一份 README 或初始 commit。將下方 URL 換成自己的遠端；若已有 origin，先用 `git remote -v` 核對，不要直接覆蓋。

```sh
git commit -m "Initial desktop application"
git branch -m main
git remote add origin https://github.com/YOUR_ACCOUNT/grammar-peek-desktop.git
git push -u origin main
```

Git 若提示缺少姓名與信箱，可設定此 repository 的 `user.name` 與 `user.email`，不必修改全域設定。

## 後續變更

```sh
git switch -c codex/short-description
# 修改、驗證、暫存並檢查差異後：
git commit -m "Describe the change"
git push -u origin codex/short-description
```

使用 PR 範本描述問題、變更與實際驗證範圍。避免強制推送共用分支。

## CI 與發布

現有 `.github/workflows/build.yml` 會在 push／PR 執行三平台測試與打包，產物保存在 Actions artifacts；不會自動發布 Release，也不代表各平台實機驗收通過。安裝檔應放在 Release／artifacts，不要提交到 Git。

正式簽章的憑證與密碼放在 CI 的秘密設定，不放進 repository。本專案尚未選定開源授權；若要公開提供他人使用／修改／散布，請先由專案擁有者選定適當的 LICENSE。
