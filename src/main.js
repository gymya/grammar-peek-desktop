import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { correctText } from "./corrections.js";
const $ = id => document.getElementById(id);
let revision = 0;
let busy = false;
let corrected = "";
function status(message, error = false) { $("status").hidden = !message; $("status").textContent = message; $("status").classList.toggle("error", error); if (message) $("status").scrollIntoView({ block: "nearest" }); }
function changed() {
  revision++;
  corrected = "";
  $("result-card").hidden = true;
  $("count").textContent = `${new TextEncoder().encode($("input").value).length.toLocaleString()} / 20,000 bytes`;
}
$("input").addEventListener("input", changed);
async function check() {
  if (busy) { status("正在檢查上一段文字，請完成後再試。"); return; }
  const text = $("input").value;
  if (!text.trim()) { status("請先選取或貼上文字。", true); return; }
  if (new TextEncoder().encode(text).length > 20000) { status("文字超過 20 KB，請縮短後再試。", true); return; }
  const version = revision;
  busy = true;
  $("check").disabled = true;
  $("result-card").hidden = true;
  status("正在透過 LanguageTool 檢查…");
  try {
    const matches = await invoke("check_text", { text });
    if (version !== revision) { status("原文已變更，請重新檢查。"); return; }
    const result = correctText(text, matches);
    corrected = result.text;
    $("result").replaceChildren();
    $("deletions").replaceChildren();
    let cursor = 0;
    for (const change of result.changes) {
      $("result").append(document.createTextNode(result.text.slice(cursor, change.start)));
      cursor = change.start + change.length;
      if (change.length) {
        const mark = document.createElement("mark");
        mark.textContent = result.text.slice(change.start, cursor);
        mark.title = change.original ? `原文：${change.original}` : "新增文字";
        $("result").append(mark);
      } else {
        const deletion = document.createElement("del");
        deletion.textContent = change.original;
        deletion.title = "建議刪除";
        $("deletions").append(deletion);
      }
    }
    $("result").append(document.createTextNode(result.text.slice(cursor)));
    $("summary").textContent = `${result.applied} 處修正${result.unresolved ? ` · ${result.unresolved} 處無法自動修正，保留原文` : ""}`;
    $("result-card").hidden = false;
    status(result.applied ? "" : "未發現需修正的地方。");
  } catch (error) { if (version === revision) status(String(error), true); }
  finally { busy = false; $("check").disabled = false; }
}
$("check").addEventListener("click", check);
$("input").addEventListener("keydown", event => { if ((event.metaKey || event.ctrlKey) && event.key === "Enter") { event.preventDefault(); check(); } });
$("copy").addEventListener("click", async () => {
  try { await writeText(corrected); status("已複製"); }
  catch { status("無法寫入剪貼簿，請反白結果手動複製。", true); }
});
$("hide").addEventListener("click", () => invoke("hide_popup"));
$("hide-editor").addEventListener("click", () => invoke("hide_popup"));
document.addEventListener("keydown", event => { if (event.key === "Escape") invoke("hide_popup"); });
await listen("selected-text", ({ payload }) => {
  if (busy) { status("正在檢查上一段文字，請完成後再按快捷鍵。", true); return; }
  $("setup").hidden = true;
  $("editor").open = false;
  $("permission").hidden = true;
  $("input").value = payload;
  changed();
  check();
});
await listen("selection-error", async ({ payload }) => {
  $("result-card").hidden = true;
  status(payload, true);
  const setup = await invoke("setup_status");
  $("permission").hidden = !setup.needs_permission;
});
$("enable-permission").addEventListener("click", async () => {
  const allowed = await invoke("request_accessibility");
  status(allowed ? "權限已啟用，請回原 App 重新選字並按快捷鍵。" : "請在系統設定的「輔助使用」允許 Grammar Peek，再回原 App 重新選字。", !allowed);
  $("permission").hidden = allowed;
});
const setup = await invoke("setup_status");
$("shortcut").textContent = setup.shortcut;
$("permission").hidden = !setup.needs_permission;
if (setup.notice && !setup.needs_permission) { $("setup").hidden = false; $("setup").textContent = setup.notice; }
