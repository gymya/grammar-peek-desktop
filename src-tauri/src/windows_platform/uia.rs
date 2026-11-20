use super::{error, Source};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use windows::{
    core::Interface,
    Win32::{
        Foundation::E_NOINTERFACE,
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_MULTITHREADED,
        },
        UI::Accessibility::{
            CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement,
            IUIAutomationTextPattern, IUIAutomationTreeWalker,
            UIA_IsTextPatternAvailablePropertyId, UIA_TextPatternId, UIA_E_NOTSUPPORTED,
        },
    },
};

struct Com;
impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

// Runs in a disposable process: a stalled third-party provider cannot strand the UI thread.
pub(super) fn read(source: Source) -> Result<String, String> {
    source.validate()?;
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }.map_err(error)?;
    let _com = Com;
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }.map_err(error)?;
    let settings = automation.cast::<IUIAutomation2>().map_err(error)?;
    unsafe {
        settings.SetConnectionTimeout(800).map_err(error)?;
        settings.SetTransactionTimeout(1000).map_err(error)?;
    }
    let root = unsafe { automation.ElementFromHandle(source.hwnd()) }.map_err(error)?;
    let walker = unsafe { automation.RawViewWalker() }.map_err(error)?;
    let deadline = Instant::now() + Duration::from_millis(2500);
    let focused = unsafe { automation.GetFocusedElement() }.ok();
    if let Some(element) = &focused {
        if unsafe { element.CurrentIsPassword() }
            .map_err(error)?
            .as_bool()
        {
            return Err("不讀取密碼欄位。".into());
        }
    }
    let pointed = unsafe { automation.ElementFromPoint(source.point()) }.ok();
    for candidate in [focused, pointed].into_iter().flatten() {
        if let Some(path) = path_to_root(&automation, &walker, candidate, &root, deadline)? {
            for element in path {
                if Instant::now() >= deadline {
                    return Err("讀取選取文字逾時。".into());
                }
                if let Some(text) = selected(&element)? {
                    return finish(source, text);
                }
            }
        }
    }
    let mut queue = VecDeque::from([root]);
    let mut visited = 0;
    while let Some(element) = queue.pop_front() {
        if Instant::now() >= deadline {
            return Err("讀取選取文字逾時。".into());
        }
        if visited >= 512 {
            return Err("來源頁面過大，未在搜尋範圍內找到反白文字，請點選文字區域後再試。".into());
        }
        visited += 1;
        // Hidden tabs/panes can retain old selections; only search visible fallback elements.
        if unsafe { element.CurrentIsOffscreen() }
            .map_err(error)?
            .as_bool()
        {
            continue;
        }
        // Fail closed when a provider cannot tell us whether this is a password field.
        if unsafe { element.CurrentIsPassword() }
            .map_err(error)?
            .as_bool()
        {
            continue;
        }
        if let Some(text) = selected(&element)? {
            return finish(source, text);
        }
        let mut child = unsafe { walker.GetFirstChildElement(&element) }.ok();
        while let Some(current) = child {
            if queue.len() + visited >= 512 || Instant::now() >= deadline {
                break;
            }
            child = unsafe { walker.GetNextSiblingElement(&current) }.ok();
            queue.push_back(current);
        }
    }
    source.validate()?;
    Err("未讀到反白文字。來源 App 必須支援 UI Automation 的文字選取；VS Code 可先開啟輔助使用支援。".into())
}

fn path_to_root(
    automation: &IUIAutomation,
    walker: &IUIAutomationTreeWalker,
    element: IUIAutomationElement,
    root: &IUIAutomationElement,
    deadline: Instant,
) -> Result<Option<Vec<IUIAutomationElement>>, String> {
    let mut path = Vec::new();
    let mut current = element;
    for _ in 0..32 {
        if Instant::now() >= deadline {
            break;
        }
        let at_root = unsafe { automation.CompareElements(&current, root) }
            .map_err(error)?
            .as_bool();
        path.push(current.clone());
        if at_root {
            return Ok(Some(path));
        }
        let Ok(parent) = (unsafe { walker.GetParentElement(&current) }) else {
            break;
        };
        current = parent;
    }
    Ok(None)
}

fn selected(element: &IUIAutomationElement) -> Result<Option<String>, String> {
    if unsafe { element.CurrentIsPassword() }
        .map_err(error)?
        .as_bool()
    {
        return Ok(None);
    }
    let available =
        unsafe { element.GetCurrentPropertyValue(UIA_IsTextPatternAvailablePropertyId) }
            .map_err(error)?;
    if !bool::try_from(&available).map_err(error)? {
        return Ok(None);
    }
    let pattern =
        match unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }
        {
            Ok(pattern) => pattern,
            Err(e) if e.code() == E_NOINTERFACE || e.code().0 as u32 == UIA_E_NOTSUPPORTED => {
                return Ok(None)
            }
            Err(e) => return Err(error(e)),
        };
    let ranges = unsafe { pattern.GetSelection() }.map_err(error)?;
    let count = unsafe { ranges.Length() }.map_err(error)?;
    if count == 0 {
        return Ok(None);
    }
    if count != 1 {
        return Err("請只反白一段連續文字，不支援多重選取。".into());
    }
    let range = unsafe { ranges.GetElement(0) }.map_err(error)?;
    // The extra UTF-16 unit detects oversized selections without reading an entire document.
    let text = unsafe { range.GetText(20_001) }.map_err(error)?.to_string();
    if text.trim().is_empty() {
        return Ok(None);
    }
    if text.len() > 20_000 {
        return Err("選取文字超過 20 KB，請縮短後再試。".into());
    }
    Ok(Some(text))
}

fn finish(source: Source, text: String) -> Result<String, String> {
    source.validate()?;
    Ok(text)
}
