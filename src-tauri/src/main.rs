#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(target_os = "macos")]
mod macos;
mod selection;
#[cfg(target_os = "windows")]
mod windows_platform;

use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const SHORTCUT: &str = "CommandOrControl+Shift+G";
const API: &str = "https://api.languagetool.org/v2/check";

#[derive(Default)]
struct RateLimit {
    requests: VecDeque<(Instant, usize)>,
}
impl RateLimit {
    fn reserve(&mut self, bytes: usize, now: Instant) -> Result<(), String> {
        while self
            .requests
            .front()
            .is_some_and(|(time, _)| now.duration_since(*time) >= Duration::from_secs(60))
        {
            self.requests.pop_front();
        }
        if self.requests.len() >= 15
            || self.requests.iter().map(|(_, size)| size).sum::<usize>() + bytes > 70_000
        {
            return Err("已接近免費 API 使用上限，請等一分鐘再試。".into());
        }
        self.requests.push_back((now, bytes));
        Ok(())
    }
}
struct AppState {
    client: reqwest::Client,
    rate: Mutex<RateLimit>,
    request: tokio::sync::Mutex<()>,
    capturing: AtomicBool,
    notice: Mutex<String>,
}
#[derive(Deserialize, Serialize)]
struct Replacement {
    value: String,
}
#[derive(Deserialize, Serialize)]
struct Match {
    offset: usize,
    length: usize,
    replacements: Vec<Replacement>,
}
#[derive(Deserialize)]
struct CheckResponse {
    matches: Vec<Match>,
}

fn validate_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("請先選取或貼上文字。".into());
    }
    if text.len() > 20_000 {
        return Err("文字超過 20 KB，請縮短後再試。".into());
    }
    Ok(())
}
#[tauri::command]
async fn check_text(text: String, state: State<'_, AppState>) -> Result<Vec<Match>, String> {
    validate_text(&text)?;
    let _guard = state
        .request
        .try_lock()
        .map_err(|_| "已有一筆檢查進行中，請稍候。")?;
    state
        .rate
        .lock()
        .map_err(|_| "無法讀取使用額度。")?
        .reserve(text.len(), Instant::now())?;
    let mut response = state
        .client
        .post(API)
        .form(&[
            ("text", text.as_str()),
            ("language", "auto"),
            ("preferredVariants", "en-US,de-DE,pt-PT"),
        ])
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                "檢查逾時，請稍後再試。"
            } else {
                "無法連線到 LanguageTool，請確認網路。"
            }
        })?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("LanguageTool 額度已達上限，請稍後再試。".into());
    }
    if !response.status().is_success() {
        return Err("LanguageTool 暫時無法處理文字，請稍後再試。".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "讀取回應失敗，請稍後再試。")?
    {
        if body.len() + chunk.len() > 4_000_000 {
            return Err("LanguageTool 回應過大，請縮短文字再試。".into());
        }
        body.extend_from_slice(&chunk);
    }
    let data: CheckResponse =
        serde_json::from_slice(&body).map_err(|_| "LanguageTool 回應格式不正確。")?;
    Ok(data.matches)
}
#[derive(Serialize)]
struct SetupStatus {
    shortcut: &'static str,
    notice: String,
    needs_permission: bool,
}
#[tauri::command]
fn setup_status(state: State<'_, AppState>) -> SetupStatus {
    SetupStatus {
        shortcut: if cfg!(target_os = "macos") {
            "⌘ + Shift + G"
        } else {
            "Ctrl + Shift + G"
        },
        notice: state.notice.lock().unwrap().clone(),
        needs_permission: {
            #[cfg(target_os = "macos")]
            {
                !macos::trusted()
            }
            #[cfg(not(target_os = "macos"))]
            {
                false
            }
        },
    }
}
#[tauri::command]
fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::request_permission()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
fn hide_popup(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::hide_panel(&app).map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        app.get_webview_window("main")
            .ok_or("Window unavailable")?
            .hide()
            .map_err(|error| error.to_string())
    }
}

#[cfg(any(target_os = "linux", test))]
fn popup_position(
    cursor: (f64, f64),
    origin: (i32, i32),
    screen: (u32, u32),
    window: (u32, u32),
) -> (i32, i32) {
    let x = (cursor.0 + 12.0) as i32;
    let y = (cursor.1 + 18.0) as i32;
    let max_x = origin.0 + screen.0.saturating_sub(window.0) as i32;
    let max_y = origin.1 + screen.1.saturating_sub(window.1) as i32;
    (x.clamp(origin.0, max_x), y.clamp(origin.1, max_y))
}
#[cfg(target_os = "linux")]
fn show(app: &tauri::AppHandle, cursor: Option<tauri::PhysicalPosition<f64>>) {
    if let Some(window) = app.get_webview_window("main") {
        if let Some(cursor) = cursor {
            if let (Ok(monitors), Ok(size)) = (window.available_monitors(), window.outer_size()) {
                if let Some(monitor) = monitors.iter().find(|monitor| {
                    let p = monitor.position();
                    let s = monitor.size();
                    cursor.x >= p.x as f64
                        && cursor.y >= p.y as f64
                        && cursor.x < p.x as f64 + s.width as f64
                        && cursor.y < p.y as f64 + s.height as f64
                }) {
                    let p = monitor.position();
                    let s = monitor.size();
                    let (x, y) = popup_position(
                        (cursor.x, cursor.y),
                        (p.x, p.y),
                        (s.width, s.height),
                        (size.width, size.height),
                    );
                    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                }
            }
        }
        {
            let _ = window.show();
        }
    }
}
fn main() {
    #[cfg(target_os = "windows")]
    if windows_platform::worker_main() { return; }
    tauri::Builder::default()
        .manage(AppState {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Cannot create HTTPS client"),
            rate: Mutex::new(RateLimit::default()),
            request: tokio::sync::Mutex::new(()),
            capturing: AtomicBool::new(false),
            notice: Mutex::new(String::new()),
        })
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let show_item = tauri::menu::MenuItem::with_id(app, "show", "顯示浮窗", true, None::<&str>)?;
            let quit_item = tauri::menu::MenuItem::with_id(app, "quit", "結束程式", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show_item, &quit_item])?;
            let mut tray = tauri::tray::TrayIconBuilder::with_id("grammar-peek")
                .tooltip("Grammar Peek")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        #[cfg(target_os = "macos")]
                        if let Err(error) = macos::show_panel(app, macos::cursor_position()) {
                            eprintln!("Cannot show panel: {error}");
                        }
                        #[cfg(target_os = "windows")]
                        if let Err(error) = windows_platform::cursor_position().and_then(|point| windows_platform::show(app, point)) {
                            eprintln!("{error}");
                        }
                        #[cfg(target_os = "linux")]
                        show(app, None);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            let window = app
                .get_webview_window("main")
                .ok_or("Missing main webview")?;
            #[cfg(target_os = "macos")]
            macos::initialize_panel(&window)?;
            #[cfg(target_os = "windows")]
            {
                app.manage(windows_platform::ClickMonitor::start(&window)?);
                windows_platform::show(app.handle(), windows_platform::cursor_position()?)?;
            }
            #[cfg(target_os = "linux")]
            window.show()?;
            let wayland =
                cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
            let shortcut_available = if wayland {
                false
            } else {
                app.handle()
                    .plugin(
                        tauri_plugin_global_shortcut::Builder::new()
                            .with_handler(|app, _, event| {
                                if event.state() != ShortcutState::Pressed {
                                    return;
                                }
                                if app
                                    .state::<AppState>()
                                    .capturing
                                    .swap(true, Ordering::SeqCst)
                                {
                                    return;
                                }
                                #[cfg(target_os = "macos")]
                                let cursor = macos::cursor_position();
                                #[cfg(target_os = "macos")]
                                let source_pid = macos::source_pid();
                                #[cfg(target_os = "windows")]
                                let source = windows_platform::capture_source();
                                #[cfg(target_os = "windows")]
                                let cursor = windows_platform::cursor_position();
                                #[cfg(target_os = "linux")]
                                let cursor = app
                                    .get_webview_window("main")
                                    .and_then(|window| window.cursor_position().ok());
                                let app = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    // Read before focusing our window, while the source app still owns the selection.
                                    #[cfg(target_os = "macos")]
                                    let result = selection::selected_text(source_pid).await;
                                    #[cfg(target_os = "windows")]
                                    let result = match source {
                                        Ok(source) => windows_platform::selected_text(source).await,
                                        Err(error) => Err(error),
                                    };
                                    #[cfg(target_os = "linux")]
                                    let result = selection::selected_text().await;
                                    #[cfg(target_os = "macos")]
                                    if let Err(error) = macos::show_panel(&app, cursor) {
                                        eprintln!("Cannot show panel: {error}");
                                    }
                                    #[cfg(target_os = "windows")]
                                    if let Err(error) = cursor.and_then(|point| windows_platform::show(&app, point)) {
                                        eprintln!("{error}");
                                    }
                                    #[cfg(target_os = "linux")]
                                    show(&app, cursor);
                                    match result {
                                        Ok(text) => {
                                            let _ = app.emit_to("main", "selected-text", text);
                                        }
                                        Err(message) => {
                                            let _ = app.emit_to("main", "selection-error", message);
                                        }
                                    }
                                    app.state::<AppState>()
                                        .capturing
                                        .store(false, Ordering::SeqCst);
                                });
                            })
                            .build(),
                    )
                    .is_ok()
            };
            let notice =
                if cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some() {
                    "Wayland：請直接貼上文字檢查。全域快捷鍵與跨 App 取字需要 X11 工作階段。"
                        .to_string()
                } else if !shortcut_available || app.global_shortcut().register(SHORTCUT).is_err() {
                    "無法註冊快捷鍵（可能已被其他 App 使用）。請直接貼上文字檢查。".to_string()
                } else if cfg!(target_os = "macos") {
                    String::new()
                } else if cfg!(target_os = "linux") {
                    "X11 取字需要 xclip，讀取目前的 PRIMARY 選取內容；不支援的 App 請改用貼上。"
                        .to_string()
                } else {
                    String::new()
                };
            *app.state::<AppState>().notice.lock().unwrap() = notice;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(error) = hide_popup(window.app_handle().clone()) {
                    eprintln!("Cannot hide popup: {error}");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            check_text,
            setup_status,
            request_accessibility,
            hide_popup
        ])
        .build(tauri::generate_context!())
        .expect("Unable to build Grammar Peek")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                #[cfg(target_os = "windows")]
                if let Some(monitor) = _app.try_state::<windows_platform::ClickMonitor>() { monitor.stop(); }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_stays_inside_current_monitor() {
        assert_eq!(
            popup_position((100.0, 200.0), (0, 0), (1920, 1080), (440, 380)),
            (112, 218)
        );
        assert_eq!(
            popup_position((1910.0, 1070.0), (0, 0), (1920, 1080), (440, 380)),
            (1480, 700)
        );
        assert_eq!(
            popup_position((-20.0, 100.0), (-1920, 0), (1920, 1080), (440, 380)),
            (-440, 118)
        );
        assert_eq!(
            popup_position((20.0, 20.0), (0, 0), (320, 240), (440, 380)),
            (0, 0)
        );
    }
    #[test]
    fn text_limits_use_utf8_bytes() {
        assert!(validate_text(" \n").is_err());
        assert!(validate_text(&"a".repeat(20_000)).is_ok());
        assert!(validate_text(&"中".repeat(6_667)).is_err());
    }
    #[test]
    fn limits_requests_and_recovers_at_window_boundary() {
        let now = Instant::now();
        let mut rate = RateLimit::default();
        for _ in 0..15 {
            assert!(rate.reserve(1, now).is_ok());
        }
        assert!(rate.reserve(1, now).is_err());
        assert!(rate.reserve(1, now + Duration::from_secs(60)).is_ok());
    }
    #[test]
    fn limits_total_bytes_without_charging_rejected_requests() {
        let now = Instant::now();
        let mut rate = RateLimit::default();
        for _ in 0..3 {
            assert!(rate.reserve(20_000, now).is_ok());
        }
        assert!(rate.reserve(10_001, now).is_err());
        assert!(rate.reserve(10_000, now).is_ok());
    }
    #[test]
    fn rejects_malformed_response() {
        assert!(serde_json::from_str::<CheckResponse>(r#"{"matches":[{"offset":-1}]}"#).is_err());
        let parsed = serde_json::from_str::<CheckResponse>(
            r#"{"matches":[{"offset":3,"length":1,"replacements":[{"value":"the"}]}]}"#,
        )
        .unwrap();
        assert_eq!(parsed.matches[0].offset, 3);
    }
}
