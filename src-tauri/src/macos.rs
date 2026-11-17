use accessibility_sys::{
    AXIsProcessTrusted, AXIsProcessTrustedWithOptions, AXUIElementCopyAttributeValue,
    AXUIElementCreateSystemWide, AXUIElementCreateApplication, AXUIElementGetTypeID, AXUIElementRef,
    AXUIElementCopyParameterizedAttributeValue,
    AXUIElementSetMessagingTimeout, AXUIElementSetAttributeValue,
};
use core_foundation::{
    array::CFArray,
    base::{CFType, TCFType},
    boolean::CFBoolean,
    dictionary::CFDictionary,
    string::CFString,
};
use std::time::{Duration, Instant};

pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub fn request_permission() -> bool {
    let options = CFDictionary::from_CFType_pairs(&[(
        CFString::new("AXTrustedCheckOptionPrompt"),
        CFBoolean::true_value(),
    )]);
    // Show the system's permission guidance; only the user can grant access.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

fn attribute(element: &CFType, name: &str) -> Result<CFType, i32> {
    if element.type_of() != unsafe { AXUIElementGetTypeID() } {
        return Err(-25202);
    }
    let key = CFString::new(name);
    let mut value = std::ptr::null();
    let code = unsafe {
        AXUIElementCopyAttributeValue(
            element.as_CFTypeRef() as AXUIElementRef,
            key.as_concrete_TypeRef(),
            &mut value,
        )
    };
    if code != 0 || value.is_null() {
        return Err(if code == 0 { -25212 } else { code });
    }
    // Copy APIs transfer ownership; CFType releases the object on drop.
    Ok(unsafe { CFType::wrap_under_create_rule(value) })
}
fn string_attribute(element: &CFType, name: &str) -> Option<String> {
    attribute(element, name)
        .ok()?
        .downcast::<CFString>()
        .map(|s| s.to_string())
}
fn secure(element: &CFType) -> bool {
    string_attribute(element, "AXSubrole").as_deref() == Some("AXSecureTextField")
}
fn selected(element: &CFType) -> Option<String> {
    if secure(element) {
        return None;
    }
    if let Some(text) = string_attribute(element, "AXSelectedText").filter(|s| !s.trim().is_empty()) {
        return Some(text);
    }
    // Some text controls expose only the selected range, not AXSelectedText.
    let range = attribute(element, "AXSelectedTextRange").ok()?;
    let key = CFString::new("AXStringForRange");
    let mut value = std::ptr::null();
    let code = unsafe {
        AXUIElementCopyParameterizedAttributeValue(
            element.as_CFTypeRef() as AXUIElementRef,
            key.as_concrete_TypeRef(), range.as_CFTypeRef(), &mut value,
        )
    };
    if code == 0 && !value.is_null() {
        let value = unsafe { CFType::wrap_under_create_rule(value) };
        if let Some(text) = value.downcast::<CFString>().map(|s| s.to_string()).filter(|s| !s.trim().is_empty()) {
            return Some(text);
        }
    }
    // Standard text fields can expose AXValue + AXSelectedTextRange without AXStringForRange.
    let (start, length) = selected_range(element)?;
    if length == 0 { return None; }
    let value = string_attribute(element, "AXValue")?;
    selected_substring(&value, start, length)
}

fn selected_range(element: &CFType) -> Option<(usize, usize)> {
    use accessibility_sys::{AXValueGetTypeID, AXValueGetValue, AXValueRef, kAXValueTypeCFRange};
    let value = attribute(element, "AXSelectedTextRange").ok()?;
    if value.type_of() != unsafe { AXValueGetTypeID() } { return None; }
    let mut range = core_foundation::base::CFRange { location: 0, length: 0 };
    let valid = unsafe { AXValueGetValue(value.as_CFTypeRef() as AXValueRef, kAXValueTypeCFRange, (&mut range as *mut core_foundation::base::CFRange).cast()) };
    if !valid { return None; }
    Some((range.location.try_into().ok()?, range.length.try_into().ok()?))
}

fn selected_substring(value: &str, start: usize, length: usize) -> Option<String> {
    if length == 0 { return None; }
    let units: Vec<u16> = value.encode_utf16().collect();
    let selected = units.get(start..start.checked_add(length)?)?;
    String::from_utf16(selected).ok().filter(|s| !s.trim().is_empty())
}

fn ax_hint(code: i32) -> &'static str {
    match code {
        -25204 => "系統無法與來源 App 通訊，請重新啟動來源 App 後再試。",
        -25211 => "系統拒絕輔助使用存取，請重新授予目前安裝版本的權限。",
        -25202 => "來源的輔助使用元素已失效，請重新選字。",
        _ => "請回報此錯誤碼以確認原因。",
    }
}

fn enable_accessibility(element: &CFType, name: &str) -> bool {
    if attribute(element, name).ok().and_then(|value| value.downcast::<CFBoolean>())
        .is_some_and(|value| value == CFBoolean::true_value()) {
        return false;
    }
    let name = CFString::new(name);
    unsafe {
        AXUIElementSetAttributeValue(element.as_CFTypeRef() as AXUIElementRef,
            name.as_concrete_TypeRef(), CFBoolean::true_value().as_CFTypeRef()) == 0
    }
}

pub fn source_pid() -> Option<i32> {
    objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication()
        .map(|app| app.processIdentifier())
        .filter(|pid| *pid > 0 && *pid as u32 != std::process::id())
}

pub fn selected_text(source_pid: Option<i32>) -> Result<String, String> {
    if !trusted() {
        return Err(
            "需要輔助使用權限。請按「啟用取字權限」，允許 Grammar Peek 後重新選字並按快捷鍵。"
                .into(),
        );
    }
    let system = unsafe { CFType::wrap_under_create_rule(AXUIElementCreateSystemWide().cast()) };
    unsafe {
        AXUIElementSetMessagingTimeout(system.as_CFTypeRef() as AXUIElementRef, 1.0);
    }
    // Capture the source before asynchronous work; AXFocusedApplication may be unavailable.
    let app = if let Some(pid) = source_pid {
        unsafe { CFType::wrap_under_create_rule(AXUIElementCreateApplication(pid).cast()) }
    } else {
        attribute(&system, "AXFocusedApplication")
            .map_err(|code| format!("無法辨識來源 App（AX {code}），請回到來源 App 重新選字。"))?
    };
    unsafe {
        AXUIElementSetMessagingTimeout(app.as_CFTypeRef() as AXUIElementRef, 1.0);
    }
    // Electron and Chromium document these signals for exposing their accessibility trees.
    // Unsupported attributes are ignored; never disable accessibility used by other clients.
    let mut enabled = enable_accessibility(&app, "AXManualAccessibility");
    enabled |= enable_accessibility(&app, "AXEnhancedUserInterface");
    if let Ok(window) = attribute(&app, "AXFocusedWindow") {
        enabled |= enable_accessibility(&window, "AXEnhancedUserInterface");
    }
    if enabled {
        std::thread::sleep(Duration::from_millis(200));
    }
    let focused_result = attribute(&app, "AXFocusedUIElement");
    let focus_error = focused_result.as_ref().err().copied();
    let focused = focused_result.ok();
    if focused.as_ref().is_some_and(secure) {
        return Err("不讀取密碼欄位。".into());
    }
    let start = Instant::now();
    let window_result = attribute(&app, "AXFocusedWindow")
        .or_else(|_| attribute(&app, "AXMainWindow"));
    let window_error = window_result.as_ref().err().copied();
    if let (Some(focus), Some(window)) = (focus_error, window_error) {
        return Err(format!("無法讀取來源 App：焦點 AX {focus}、視窗 AX {window}。{}", ax_hint(focus)));
    }
    let source_name = string_attribute(&app, "AXTitle").unwrap_or_else(|| format!("PID {}", source_pid.unwrap_or(0)));
    let focus_role = focused.as_ref().and_then(|element| string_attribute(element, "AXRole")).unwrap_or_else(|| "無聚焦元素".into());
    let range_length = focused.as_ref().and_then(selected_range).map(|(_, len)| len.to_string()).unwrap_or_else(|| "未知".into());
    let selection_error = focused.as_ref()
        .and_then(|element| attribute(element, "AXSelectedText").err());
    let root = window_result.unwrap_or(app);
    let mut stack = vec![root];
    if let Some(focused) = focused {
        // Selection may belong to the enclosing web area rather than its focused child.
        let mut ancestor = focused.clone();
        for _ in 0..8 {
            if start.elapsed() > Duration::from_secs(4) { break; }
            if secure(&ancestor) { return Err("不讀取密碼欄位。".into()); }
            if let Some(text) = selected(&ancestor) { return Ok(text); }
            let Ok(parent) = attribute(&ancestor, "AXParent") else { break };
            ancestor = parent;
        }
        // Search the focused subtree before scanning the rest of a long conversation.
        stack.push(focused);
    }
    let mut visited = 0;
    while let Some(element) = stack.pop() {
        if visited >= 256 || start.elapsed() > Duration::from_secs(4) {
            break;
        }
        visited += 1;
        if secure(&element) {
            continue;
        }
        if let Some(text) = selected(&element) {
            return Ok(text);
        }
        if let Some(children) = attribute(&element, "AXChildren")
            .ok()
            .and_then(|v| v.downcast::<CFArray>())
        {
            for child in children
                .get_all_values()
                .into_iter()
                .take(256 - visited)
                .rev()
            {
                stack.push(unsafe { CFType::wrap_under_get_rule(child) });
            }
        }
    }
    if start.elapsed() >= Duration::from_secs(4) {
        return Err(format!("讀取選取文字逾時（已檢查 {visited} 個元素），請縮小選取範圍後再試。"));
    }
    Err(format!("未讀到反白文字：{source_name}／{focus_role}／選取長度 {range_length}／AX {}（{visited} 個元素）。",
        selection_error.map(|code| code.to_string()).unwrap_or_else(|| "無錯誤".into())))
}

use objc2::{define_class, msg_send, rc::Retained, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSEvent, NSEventMask, NSScreen, NSFloatingWindowLevel, NSPanel, NSWindow,
    NSWindowCollectionBehavior as Behavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSProcessInfo, NSRect};
use std::cell::RefCell;
use tauri::Manager;

define_class!(
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    struct GrammarPanel;
    impl GrammarPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { true }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { false }
    }
);
thread_local! {
    // AppKit objects stay on the main thread; Tauri continues owning the webview.
    static PANEL: RefCell<Option<Retained<GrammarPanel>>> = const { RefCell::new(None) };
    // Keep the single event monitor alive for the application lifetime.
    static OUTSIDE_CLICK_MONITOR: RefCell<Option<Retained<objc2::runtime::AnyObject>>> = const { RefCell::new(None) };
}
fn panel_behavior(major_version: isize) -> Behavior {
    Behavior::CanJoinAllSpaces
        | if major_version >= 13 {
            Behavior::CanJoinAllApplications
        } else {
            Behavior::FullScreenAuxiliary
        }
}

pub fn initialize_panel(window: &tauri::WebviewWindow) -> Result<(), Box<dyn std::error::Error>> {
    let mtm = MainThreadMarker::new().ok_or("Panel must be created on the main thread")?;
    let raw = window.ns_window()?;
    let original = unsafe { &*raw.cast::<NSWindow>() };
    let content = original.contentView().ok_or("Missing webview content")?;
    let frame: NSRect = original.frame();
    let allocated = GrammarPanel::alloc(mtm).set_ivars(());
    // Initialize a real NSPanel with its nonactivation behavior from birth.
    let panel: Retained<GrammarPanel> = unsafe {
        msg_send![super(allocated),
            initWithContentRect: frame,
            styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    unsafe {
        panel.setReleasedWhenClosed(false);
    }
    panel.setTitle(&original.title());
    panel.setLevel(NSFloatingWindowLevel);
    panel.setFloatingPanel(true);
    panel.setBecomesKeyOnlyIfNeeded(true);
    panel.setHidesOnDeactivate(false);
    panel.setHasShadow(true);
    let version = NSProcessInfo::processInfo().operatingSystemVersion();
    panel.setCollectionBehavior(panel_behavior(version.majorVersion));
    original.setContentView(None);
    panel.setContentView(Some(&content));
    original.orderOut(None);
    PANEL.with(|slot| *slot.borrow_mut() = Some(panel));
    // Global monitors see clicks delivered to other apps, never clicks inside this panel.
    let handler = block2::RcBlock::new(|_event: std::ptr::NonNull<NSEvent>| {
        PANEL.with(|slot| {
            if let Some(panel) = slot.borrow().as_ref() {
                if panel.isVisible() { panel.orderOut(None); }
            }
        });
    });
    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
        NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown,
        &handler,
    ).ok_or("Cannot monitor outside clicks")?;
    OUTSIDE_CLICK_MONITOR.with(|slot| *slot.borrow_mut() = Some(monitor));
    show_panel(window.app_handle(), cursor_position())?;
    Ok(())
}

// AppKit uses logical points with a bottom-left origin, including secondary screens.
pub fn cursor_position() -> NSPoint {
    NSEvent::mouseLocation()
}

fn popup_origin(cursor: NSPoint, screen: NSRect, size: objc2_foundation::NSSize) -> NSPoint {
    NSPoint::new(
        (cursor.x + 12.0).clamp(screen.origin.x, screen.origin.x + (screen.size.width - size.width).max(0.0)),
        (cursor.y - 18.0 - size.height).clamp(screen.origin.y, screen.origin.y + (screen.size.height - size.height).max(0.0)),
    )
}

pub fn show_panel(app: &tauri::AppHandle, cursor: NSPoint) -> tauri::Result<()> {
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("AppKit main thread");
        PANEL.with(|slot| {
            if let Some(panel) = slot.borrow().as_ref() {
                let screens = NSScreen::screens(mtm);
                let screen = screens.iter().find(|screen| {
                    let frame = screen.frame();
                    cursor.x >= frame.origin.x && cursor.x < frame.origin.x + frame.size.width
                        && cursor.y >= frame.origin.y && cursor.y < frame.origin.y + frame.size.height
                }).or_else(|| screens.firstObject());
                if let Some(screen) = screen {
                    panel.setFrameOrigin(popup_origin(cursor, screen.visibleFrame(), panel.frame().size));
                }
                panel.orderFrontRegardless();
            }
        });
    })
}

pub fn hide_panel(app: &tauri::AppHandle) -> tauri::Result<()> {
    app.run_on_main_thread(|| {
        PANEL.with(|slot| {
            if let Some(panel) = slot.borrow().as_ref() {
                panel.orderOut(None);
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_range_uses_utf16_and_rejects_invalid_boundaries() {
        assert_eq!(selected_substring("a😀bc", 1, 2).as_deref(), Some("😀"));
        assert_eq!(selected_substring("abc", 1, 2).as_deref(), Some("bc"));
        assert_eq!(selected_substring("abc", 2, 2), None);
        assert_eq!(selected_substring("a😀", 1, 1), None);
        assert_eq!(selected_substring("abc", 0, 0), None);
    }
    #[test]
    fn native_position_respects_negative_screen_origin_and_edges() {
        use objc2_foundation::NSSize;
        let screen = NSRect::new(NSPoint::new(-1440.0, 0.0), NSSize::new(1440.0, 900.0));
        let size = NSSize::new(360.0, 240.0);
        assert_eq!(popup_origin(NSPoint::new(-10.0, 10.0), screen, size), NSPoint::new(-360.0, 0.0));
        assert_eq!(popup_origin(NSPoint::new(-1400.0, 800.0), screen, size), NSPoint::new(-1388.0, 542.0));
    }
    #[test]
    fn panel_joins_other_apps_without_conflicting_behaviors() {
        let modern = panel_behavior(26);
        assert!(modern.contains(Behavior::CanJoinAllSpaces | Behavior::CanJoinAllApplications));
        assert!(!modern.intersects(
            Behavior::Primary
                | Behavior::Auxiliary
                | Behavior::MoveToActiveSpace
                | Behavior::FullScreenPrimary
        ));
        assert!(panel_behavior(12).contains(Behavior::FullScreenAuxiliary));
        assert!(!panel_behavior(12).contains(Behavior::CanJoinAllApplications));
    }
    #[test]
    fn rejects_non_accessibility_objects_before_ffi() {
        let value = CFString::new("not an accessibility element").as_CFType();
        assert!(attribute(&value, "AXSelectedText").is_err());
    }
}
