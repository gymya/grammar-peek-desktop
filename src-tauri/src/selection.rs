#[cfg(target_os = "linux")]
use std::process::Stdio;
#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(target_os = "linux")]
use tokio::process::Command;

#[cfg(target_os = "linux")]
pub async fn selected_text() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    let mut command = {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return Err("Wayland 不支援此全域取字方式，請在視窗中貼上文字。".into());
        }
        let mut command = Command::new("xclip");
        command.args(["-o", "-selection", "primary"]);
        command
    };
    command.kill_on_drop(true).stdin(Stdio::null());
    let output = tokio::time::timeout(Duration::from_secs(4), command.output())
        .await
        .map_err(|_| "讀取選取文字逾時，請手動貼上。")?
        .map_err(|_| "無法讀取選取文字。Linux X11 請安裝 xclip；也可以直接貼上。")?;
    if !output.status.success() {
        return Err(
            "無法讀取 X11 選取文字；請確認 xclip 可用且來源 App 支援 PRIMARY selection。"
                .into(),
        );
    }
    let text = String::from_utf8(output.stdout).map_err(|_| "無法解讀選取文字，請手動貼上。")?;
    if text.trim().is_empty() {
        return Err("沒有讀到選取文字，請先反白文字，或直接貼上。".into());
    }
    if text.len() > 20_000 {
        return Err("選取文字超過 20 KB，請縮短後再試。".into());
    }
    Ok(text)
}

#[cfg(target_os = "macos")]
pub async fn selected_text(source_pid: Option<i32>) -> Result<String, String> {
    let text = tauri::async_runtime::spawn_blocking(move || crate::macos::selected_text(source_pid))
        .await
        .map_err(|_| "讀取選取文字失敗，請再試一次。")??;
    if text.len() > 20_000 {
        return Err("選取文字超過 20 KB，請縮短後再試。".into());
    }
    Ok(text)
}
