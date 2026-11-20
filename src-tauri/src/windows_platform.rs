mod uia;
use std::{
    cell::Cell,
    sync::{mpsc, Mutex},
    thread::JoinHandle,
    time::Duration,
};
use tauri::Manager;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{GetCurrentProcessId, GetCurrentThreadId},
    },
    UI::{
        HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        WindowsAndMessaging::*,
    },
};

#[derive(Clone, Copy)]
pub struct Source {
    window: isize,
    pid: u32,
    x: i32,
    y: i32,
}
impl Source {
    fn hwnd(self) -> HWND {
        HWND(self.window as *mut _)
    }
    fn point(self) -> POINT {
        POINT {
            x: self.x,
            y: self.y,
        }
    }
    fn validate(self) -> Result<(), String> {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(self.hwnd(), Some(&mut pid));
            if self.window == 0 || pid != self.pid || GetForegroundWindow() != self.hwnd() {
                return Err("來源視窗已切換或關閉，請重新反白並按快捷鍵。".into());
            }
        }
        Ok(())
    }
}

pub fn capture_source() -> Result<Source, String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if hwnd.0.is_null() || pid == 0 || pid == GetCurrentProcessId() {
            return Err("請先在其他 App 反白文字，再按快捷鍵。".into());
        }
        let point = cursor_position()?;
        Ok(Source {
            window: hwnd.0 as isize,
            pid,
            x: point.x,
            y: point.y,
        })
    }
}

pub fn cursor_position() -> Result<POINT, String> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.map_err(error)?;
    Ok(point)
}

fn error(e: windows::core::Error) -> String {
    if e.code().0 as u32 == 0x80070005 {
        "Windows 拒絕存取來源 App；以管理員身分啟動的 App 可能無法讀取。".into()
    } else {
        format!(
            "Windows 取字或視窗操作失敗（0x{:08X}）。",
            e.code().0 as u32
        )
    }
}

pub fn worker_main() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--read-selection") {
        return false;
    }
    let result = (|| {
        let window = args
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or("無效的來源視窗")?;
        let pid = args
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or("無效的來源程序")?;
        let x = args
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or("無效的滑鼠位置")?;
        let y = args
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or("無效的滑鼠位置")?;
        uia::read(Source { window, pid, x, y })
    })();
    // stdout is an anonymous pipe to our parent, not a log file.
    if let Ok(json) = serde_json::to_string(&result) {
        println!("{json}");
    }
    true
}

pub async fn selected_text(source: Source) -> Result<String, String> {
    use std::process::Stdio;
    let executable = std::env::current_exe().map_err(|_| "無法啟動原生取字程序。")?;
    let mut command = tokio::process::Command::new(executable);
    command
        .args([
            "--read-selection".to_string(),
            source.window.to_string(),
            source.pid.to_string(),
            source.x.to_string(),
            source.y.to_string(),
        ])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| "無法啟動原生取字程序。")?;
    let mut stdout = child.stdout.take().ok_or("取字程序沒有輸出管道。")?;
    let read = async {
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        // JSON can expand each byte into a six-byte escape.
        (&mut stdout)
            .take(130_001)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "取字回應讀取失敗。")?;
        if bytes.len() > 130_000 {
            return Err("取字回應超過上限。".into());
        }
        let status = child.wait().await.map_err(|_| "取字程序等待失敗。")?;
        if !status.success() {
            return Err("原生取字程序中止，請再試一次。".into());
        }
        let result: Result<String, String> =
            serde_json::from_slice(&bytes).map_err(|_| "取字回應格式錯誤。")?;
        let text = result?;
        if text.trim().is_empty() || text.len() > 20_000 {
            return Err("選取文字為空或超過 20 KB。".into());
        }
        source.validate()?;
        Ok(text)
    };
    match tokio::time::timeout(Duration::from_secs(4), read).await {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill().await;
            Err("讀取選取文字逾時；來源 App 沒有及時回應。".into())
        }
    }
}

pub fn show(app: &tauri::AppHandle, point: POINT) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("找不到浮窗。")?;
    let raw = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    app.run_on_main_thread(move || {
        if let Err(e) = show_native(HWND(raw as *mut _), point) {
            eprintln!("{e}");
        }
    })
    .map_err(|e| e.to_string())
}

fn show_native(hwnd: HWND, point: POINT) -> Result<(), String> {
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return Err(error(windows::core::Error::from_thread()));
        }
        let (mut dpi_x, mut dpi_y) = (96, 96);
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).map_err(error)?;
        let (x, y, width, height) = popup_geometry(point, info.rcWork, dpi_x, dpi_y);
        // Do not permanently set WS_EX_NOACTIVATE: clicking the manual input must allow typing.
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
        .map_err(error)
    }
}

fn popup_geometry(
    point: POINT,
    area: windows::Win32::Foundation::RECT,
    dpi_x: u32,
    dpi_y: u32,
) -> (i32, i32, i32, i32) {
    let width = (360 * dpi_x / 96) as i32;
    let height = (240 * dpi_y / 96) as i32;
    let x =
        (point.x + (12 * dpi_x / 96) as i32).clamp(area.left, (area.right - width).max(area.left));
    let y =
        (point.y + (18 * dpi_y / 96) as i32).clamp(area.top, (area.bottom - height).max(area.top));
    (x, y, width, height)
}

thread_local! { static POPUP: Cell<isize> = const { Cell::new(0) }; }
unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0
        && matches!(
            wparam.0 as u32,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
        )
    {
        let hwnd = HWND(POPUP.with(Cell::get) as *mut _);
        if unsafe { IsWindowVisible(hwnd) }.as_bool() {
            let event = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            let target = unsafe { GetAncestor(WindowFromPoint(event.pt), GA_ROOT) };
            if target != hwnd {
                // Queue Tauri's existing close-to-hide path; never block inside the hook.
                let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub struct ClickMonitor {
    thread: Mutex<Option<(u32, JoinHandle<()>)>>,
}
impl ClickMonitor {
    pub fn start(window: &tauri::WebviewWindow) -> Result<Self, String> {
        let raw = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
        let (send, receive) = mpsc::sync_channel(1);
        let handle = std::thread::Builder::new()
            .name("outside-click".into())
            .spawn(move || {
                POPUP.with(|slot| slot.set(raw));
                unsafe {
                    let mut message = MSG::default();
                    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                    let module = GetModuleHandleW(None).map(|module| module.into());
                    let hook = module.and_then(|module| {
                        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(module), 0)
                    });
                    let hook = match hook {
                        Ok(hook) => hook,
                        Err(e) => {
                            let _ = send.send(Err(error(e)));
                            return;
                        }
                    };
                    if send.send(Ok(GetCurrentThreadId())).is_ok() {
                        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                    let _ = UnhookWindowsHookEx(hook);
                }
            })
            .map_err(|_| "無法建立浮窗點擊監聽。")?;
        match receive.recv() {
            Ok(Ok(id)) => Ok(Self {
                thread: Mutex::new(Some((id, handle))),
            }),
            result => {
                let _ = handle.join();
                Err(result
                    .ok()
                    .and_then(Result::err)
                    .unwrap_or_else(|| "浮窗點擊監聽啟動失敗。".into()))
            }
        }
    }
    pub fn stop(&self) {
        if let Some((id, handle)) = self.thread.lock().unwrap().take() {
            if unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) }.is_ok() {
                let _ = handle.join();
            }
        }
    }
}
impl Drop for ClickMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::RECT;
    #[test]
    fn popup_respects_scaling_taskbar_and_negative_monitor_origin() {
        let work = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        assert_eq!(
            popup_geometry(POINT { x: -10, y: 1000 }, work, 144, 144),
            (-540, 680, 540, 360)
        );
        assert_eq!(
            popup_geometry(POINT { x: -1800, y: 20 }, work, 96, 96),
            (-1788, 38, 360, 240)
        );
        let tiny = RECT {
            left: 0,
            top: 0,
            right: 200,
            bottom: 100,
        };
        assert_eq!(
            popup_geometry(POINT { x: 190, y: 90 }, tiny, 96, 96),
            (0, 0, 360, 240)
        );
    }
}
