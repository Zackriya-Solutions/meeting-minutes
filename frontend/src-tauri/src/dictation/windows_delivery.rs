use super::{ClipboardPort, PastePort};
use std::fmt;
use std::ptr;
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character,
    UIA_TextPatternId,
};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, GlobalFree, HANDLE, RECT};
use windows_sys::Win32::Graphics::Gdi::{DeleteEnhMetaFile, DeleteMetaFile, DeleteObject};
use windows_sys::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
    SetClipboardData, METAFILEPICT,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Ole::{
    OleDuplicateData, CF_BITMAP, CF_DSPBITMAP, CF_DSPENHMETAFILE, CF_DSPMETAFILEPICT,
    CF_ENHMETAFILE, CF_METAFILEPICT, CF_PALETTE, CF_UNICODETEXT,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowRect, GetWindowThreadProcessId,
    IsWindow, SendMessageTimeoutW, GUITHREADINFO, SMTO_ABORTIFHUNG,
};

const EM_GETSEL: u32 = 0x00B0;
const EM_SETSEL: u32 = 0x00B1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsDeliveryError {
    operation: &'static str,
    code: u32,
}

impl WindowsDeliveryError {
    fn last(operation: &'static str) -> Self {
        Self {
            operation,
            // SAFETY: GetLastError has no preconditions.
            code: unsafe { GetLastError() },
        }
    }

    fn target(operation: &'static str) -> Self {
        Self { operation, code: 0 }
    }

    pub fn operation(&self) -> &'static str {
        self.operation
    }
}

struct OwnedClipboardFormat {
    format: u32,
    handle: HANDLE,
}

impl Drop for OwnedClipboardFormat {
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }

        // These are the non-HGLOBAL formats documented for SetClipboardData.
        // All other handles returned by OleDuplicateData use global memory.
        unsafe {
            match self.format as u16 {
                CF_BITMAP | CF_DSPBITMAP | CF_PALETTE => {
                    DeleteObject(self.handle);
                }
                CF_ENHMETAFILE | CF_DSPENHMETAFILE => {
                    DeleteEnhMetaFile(self.handle);
                }
                CF_METAFILEPICT | CF_DSPMETAFILEPICT => {
                    let picture = GlobalLock(self.handle) as *const METAFILEPICT;
                    if !picture.is_null() {
                        DeleteMetaFile((*picture).hMF);
                        GlobalUnlock(self.handle);
                    }
                    GlobalFree(self.handle);
                }
                _ => {
                    GlobalFree(self.handle);
                }
            }
        }
        self.handle = ptr::null_mut();
    }
}

/// Owns independent copies of every advertised clipboard format, including
/// rich text, HTML, images, and application-specific formats.
pub struct WindowsClipboardSnapshot {
    formats: Vec<OwnedClipboardFormat>,
}

impl WindowsClipboardSnapshot {
    fn capture() -> Result<Self, WindowsDeliveryError> {
        let _guard = ClipboardGuard::open()?;
        let mut formats = Vec::new();
        let mut format = 0;
        loop {
            // SAFETY: Clipboard is open and the prior format came from this API.
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                break;
            }
            // Materialize delayed data while its source is still available,
            // then make a snapshot owned by PulseTalq.
            let source = unsafe { GetClipboardData(format) };
            if source.is_null() {
                return Err(WindowsDeliveryError::last("GetClipboardData"));
            }
            let duplicate = unsafe { OleDuplicateData(source, format as u16, GMEM_MOVEABLE) };
            if duplicate.is_null() {
                return Err(WindowsDeliveryError::last("OleDuplicateData"));
            }
            formats.push(OwnedClipboardFormat {
                format,
                handle: duplicate,
            });
        }
        Ok(Self { formats })
    }

    fn restore(mut self) -> Result<(), WindowsDeliveryError> {
        let _guard = ClipboardGuard::open()?;
        if unsafe { EmptyClipboard() } == 0 {
            return Err(WindowsDeliveryError::last("EmptyClipboard"));
        }
        for entry in &mut self.formats {
            if unsafe { SetClipboardData(entry.format, entry.handle) }.is_null() {
                return Err(WindowsDeliveryError::last("SetClipboardData"));
            }
            // SetClipboardData transferred ownership to Windows.
            entry.handle = ptr::null_mut();
        }
        Ok(())
    }
}

impl fmt::Display for WindowsDeliveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} failed with Windows error {}",
            self.operation, self.code
        )
    }
}

impl std::error::Error for WindowsDeliveryError {}

#[derive(Debug, Clone)]
pub struct WindowsTarget {
    hwnd: usize,
    process_id: u32,
    thread_id: u32,
    focused_hwnd: Option<usize>,
    text_selection: Option<TextSelectionBookmark>,
}

impl WindowsTarget {
    pub fn capture() -> Result<Self, WindowsDeliveryError> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return Err(WindowsDeliveryError::target("SecureTargetUnavailable"));
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) };
        if thread_id == 0 || process_id == 0 {
            return Err(WindowsDeliveryError::last("GetWindowThreadProcessId"));
        }
        ensure_target_integrity(process_id)?;
        let focused_hwnd = focused_control(thread_id);
        let text_selection = match TextSelectionBookmark::capture(process_id, focused_hwnd) {
            Ok(selection) => selection,
            Err(error) => {
                log::warn!(
                    "dictation_selection_capture_unavailable code=selection_capture_unavailable error={error}"
                );
                None
            }
        };
        if let Some(selection) = text_selection.as_ref() {
            log::info!(
                "dictation_selection_captured selected=true method={}",
                selection.method()
            );
        }
        Ok(Self {
            hwnd: hwnd as usize,
            process_id,
            thread_id,
            focused_hwnd,
            text_selection,
        })
    }

    pub fn process_label(&self) -> String {
        format!("pid:{}", self.process_id)
    }

    pub fn focused_control_anchor(&self) -> Option<(f64, f64)> {
        let hwnd = self.focused_hwnd? as *mut std::ffi::c_void;
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return None;
        }
        Some(rectangle_center(rect))
    }

    fn validate_foreground(&self) -> Result<(), WindowsDeliveryError> {
        let hwnd = self.hwnd as *mut std::ffi::c_void;
        validate_window_identity(
            unsafe { IsWindow(hwnd) } != 0,
            unsafe { GetForegroundWindow() } as usize,
            self.hwnd,
        )?;
        validate_focused_control(self.focused_hwnd, focused_control(self.thread_id))
    }

    fn restore_text_selection(&self) -> Result<(), WindowsDeliveryError> {
        let Some(selection) = self.text_selection.as_ref() else {
            return Ok(());
        };
        selection.restore().map_err(|error| {
            log::warn!(
                "dictation_selection_restore_failed code=selection_restore_failed error={error}"
            );
            WindowsDeliveryError::target("SelectionRestoreFailed")
        })?;
        log::info!(
            "dictation_selection_restored selected=true method={}",
            selection.method()
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextElementIdentity {
    process_id: i32,
    control_type: i32,
    automation_id: String,
    class_name: String,
    bounds: (i32, i32, i32, i32),
}

impl TextElementIdentity {
    fn capture(element: &IUIAutomationElement) -> Result<Self, String> {
        let bounds = unsafe { element.CurrentBoundingRectangle() }
            .map_err(|error| format!("CurrentBoundingRectangle: {error}"))?;
        Ok(Self {
            process_id: unsafe { element.CurrentProcessId() }
                .map_err(|error| format!("CurrentProcessId: {error}"))?,
            control_type: unsafe { element.CurrentControlType() }
                .map_err(|error| format!("CurrentControlType: {error}"))?
                .0,
            automation_id: unsafe { element.CurrentAutomationId() }
                .map_err(|error| format!("CurrentAutomationId: {error}"))?
                .to_string(),
            class_name: unsafe { element.CurrentClassName() }
                .map_err(|error| format!("CurrentClassName: {error}"))?
                .to_string(),
            bounds: (bounds.left, bounds.top, bounds.right, bounds.bottom),
        })
    }

    fn matches(&self, current: &Self) -> bool {
        self.process_id == current.process_id
            && self.control_type == current.control_type
            && self.automation_id == current.automation_id
            && self.class_name == current.class_name
            && self.bounds == current.bounds
    }
}

#[derive(Debug, Clone)]
enum TextSelectionBookmark {
    NativeEdit { hwnd: usize, start: u32, end: u32 },
    UiAutomation(UiAutomationSelectionBookmark),
}

impl TextSelectionBookmark {
    fn capture(
        target_process_id: u32,
        focused_hwnd: Option<usize>,
    ) -> Result<Option<Self>, String> {
        if let Some(selection) = focused_hwnd.and_then(Self::capture_native_edit) {
            return Ok(Some(selection));
        }
        UiAutomationSelectionBookmark::capture(target_process_id)
            .map(|selection| selection.map(Self::UiAutomation))
    }

    fn capture_native_edit(hwnd: usize) -> Option<Self> {
        let hwnd_pointer = hwnd as *mut std::ffi::c_void;
        let mut class_name = [0_u16; 128];
        let copied = unsafe {
            GetClassNameW(
                hwnd_pointer,
                class_name.as_mut_ptr(),
                class_name.len() as i32,
            )
        };
        if copied <= 0 {
            return None;
        }
        let class_name = String::from_utf16_lossy(&class_name[..copied as usize]).to_lowercase();
        if !class_name.contains("edit") {
            return None;
        }

        let mut start = 0_u32;
        let mut end = 0_u32;
        let mut message_result = 0_usize;
        let delivered = unsafe {
            SendMessageTimeoutW(
                hwnd_pointer,
                EM_GETSEL,
                (&mut start as *mut u32) as usize,
                (&mut end as *mut u32) as isize,
                SMTO_ABORTIFHUNG,
                100,
                &mut message_result,
            )
        };
        if delivered == 0 || end <= start {
            None
        } else {
            Some(Self::NativeEdit { hwnd, start, end })
        }
    }

    fn restore(&self) -> Result<(), String> {
        match self {
            Self::NativeEdit { hwnd, start, end } => {
                let mut message_result = 0_usize;
                let delivered = unsafe {
                    SendMessageTimeoutW(
                        *hwnd as *mut std::ffi::c_void,
                        EM_SETSEL,
                        *start as usize,
                        *end as isize,
                        SMTO_ABORTIFHUNG,
                        100,
                        &mut message_result,
                    )
                };
                if delivered == 0 {
                    Err("NativeSelectionRestoreTimedOut".to_string())
                } else {
                    Ok(())
                }
            }
            Self::UiAutomation(selection) => selection.restore(),
        }
    }

    fn method(&self) -> &'static str {
        match self {
            Self::NativeEdit { .. } => "native_edit",
            Self::UiAutomation(_) => "ui_automation",
        }
    }

    #[cfg(test)]
    fn start_and_length(&self) -> (i32, i32) {
        match self {
            Self::NativeEdit { start, end, .. } => (*start as i32, (*end - *start) as i32),
            Self::UiAutomation(selection) => (selection.start, selection.length),
        }
    }
}

#[derive(Debug, Clone)]
struct UiAutomationSelectionBookmark {
    element: TextElementIdentity,
    start: i32,
    length: i32,
}

impl UiAutomationSelectionBookmark {
    fn capture(target_process_id: u32) -> Result<Option<Self>, String> {
        with_text_pattern(target_process_id, |element, pattern| {
            let ranges = unsafe { pattern.GetSelection() }
                .map_err(|error| format!("GetSelection: {error}"))?;
            if unsafe { ranges.Length() }.map_err(|error| format!("SelectionLength: {error}"))? < 1
            {
                return Ok(None);
            }
            let selected = unsafe { ranges.GetElement(0) }
                .map_err(|error| format!("GetSelectionRange: {error}"))?;
            // Read only BSTR lengths, then immediately drop the buffers. The
            // selected text itself is never copied into a Rust string, logged,
            // or persisted.
            let selected_text = unsafe { selected.GetText(-1) }
                .map_err(|error| format!("GetSelectedTextLength: {error}"))?;
            let length = i32::try_from(selected_text.len())
                .map_err(|_| "SelectedTextTooLong".to_string())?;
            if length == 0 {
                return Ok(None);
            }
            let prefix = unsafe { pattern.DocumentRange() }
                .map_err(|error| format!("SelectionDocumentRange: {error}"))?;
            unsafe {
                prefix
                    .MoveEndpointByRange(
                        TextPatternRangeEndpoint_End,
                        &selected,
                        TextPatternRangeEndpoint_Start,
                    )
                    .map_err(|error| format!("MeasureSelectionStart: {error}"))?;
            }
            let prefix_text = unsafe { prefix.GetText(-1) }
                .map_err(|error| format!("GetSelectionPrefixLength: {error}"))?;
            let start = i32::try_from(prefix_text.len())
                .map_err(|_| "SelectionStartOutOfRange".to_string())?;
            Ok(Some(Self {
                element: TextElementIdentity::capture(element)?,
                start,
                length,
            }))
        })
    }

    fn restore(&self) -> Result<(), String> {
        with_text_pattern(self.element.process_id as u32, |element, pattern| {
            let current = TextElementIdentity::capture(element)?;
            if !self.element.matches(&current) {
                return Err("FocusedTextElementChanged".to_string());
            }
            let range = unsafe { pattern.DocumentRange() }
                .map_err(|error| format!("DocumentRange: {error}"))?;
            unsafe {
                range
                    .MoveEndpointByRange(
                        TextPatternRangeEndpoint_End,
                        &range,
                        TextPatternRangeEndpoint_Start,
                    )
                    .map_err(|error| format!("CollapseSelectionRange: {error}"))?;
            }
            let moved = unsafe { range.Move(TextUnit_Character, self.start) }
                .map_err(|error| format!("MoveSelectionStart: {error}"))?;
            if moved != self.start {
                return Err("SelectionStartOutOfRange".to_string());
            }
            let expanded = unsafe {
                range.MoveEndpointByUnit(
                    TextPatternRangeEndpoint_End,
                    TextUnit_Character,
                    self.length,
                )
            }
            .map_err(|error| format!("MoveSelectionEnd: {error}"))?;
            if expanded != self.length {
                return Err("SelectionEndOutOfRange".to_string());
            }
            unsafe { range.Select() }.map_err(|error| format!("Select: {error}"))?;
            Ok(Some(()))
        })?
        .ok_or_else(|| "FocusedTextPatternUnavailable".to_string())
    }
}

fn with_text_pattern<T>(
    target_process_id: u32,
    operation: impl FnOnce(&IUIAutomationElement, &IUIAutomationTextPattern) -> Result<T, String>,
) -> Result<T, String> {
    COM_INITIALIZED.with(|initialized| initialized.ensure())?;
    let automation: IUIAutomation = unsafe {
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("CoCreateInstance: {error}"))?
    };
    let element = unsafe { automation.GetFocusedElement() }
        .map_err(|error| format!("GetFocusedElement: {error}"))?;
    let process_id = unsafe { element.CurrentProcessId() }
        .map_err(|error| format!("CurrentProcessId: {error}"))?;
    if process_id <= 0 || process_id as u32 != target_process_id {
        return Err("FocusedTextProcessChanged".to_string());
    }
    let pattern =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }
            .map_err(|error| format!("GetTextPattern: {error}"))?;
    operation(&element, &pattern)
}

thread_local! {
    static COM_INITIALIZED: ComInitialization = ComInitialization::initialize();
}

struct ComInitialization(windows::core::HRESULT);

impl ComInitialization {
    fn initialize() -> Self {
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) })
    }

    fn ensure(&self) -> Result<(), String> {
        if self.0.is_ok() || self.0 == RPC_E_CHANGED_MODE {
            Ok(())
        } else {
            Err(format!("CoInitializeEx: {}", self.0))
        }
    }
}

impl Drop for ComInitialization {
    fn drop(&mut self) {
        if self.0.is_ok() {
            unsafe { CoUninitialize() };
        }
    }
}

fn focused_control(thread_id: u32) -> Option<usize> {
    // SAFETY: A zeroed GUITHREADINFO is valid once cbSize is populated, and
    // GetGUIThreadInfo writes the remaining fields before they are inspected.
    let mut info = unsafe { std::mem::zeroed::<GUITHREADINFO>() };
    info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    if unsafe { GetGUIThreadInfo(thread_id, &mut info) } == 0 || info.hwndFocus.is_null() {
        None
    } else {
        Some(info.hwndFocus as usize)
    }
}

fn rectangle_center(rect: RECT) -> (f64, f64) {
    (
        (rect.left as f64 + rect.right as f64) / 2.0,
        (rect.top as f64 + rect.bottom as f64) / 2.0,
    )
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
            self.0 = ptr::null_mut();
        }
    }
}

fn ensure_target_integrity(process_id: u32) -> Result<(), WindowsDeliveryError> {
    let current = unsafe { GetCurrentProcess() };
    let current_integrity = process_integrity(current)?;
    let target_process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if target_process.is_null() {
        // Access denial is itself a reason not to attempt synthetic input.
        return Err(WindowsDeliveryError::last("ElevatedTargetAccessDenied"));
    }
    let target_process = OwnedHandle(target_process);
    let target_integrity = process_integrity(target_process.0)?;
    validate_integrity_levels(current_integrity, target_integrity)
}

fn validate_integrity_levels(
    current_integrity: u32,
    target_integrity: u32,
) -> Result<(), WindowsDeliveryError> {
    if target_integrity > current_integrity {
        return Err(WindowsDeliveryError::target("ElevatedTarget"));
    }
    Ok(())
}

fn process_integrity(process: HANDLE) -> Result<u32, WindowsDeliveryError> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(WindowsDeliveryError::last("OpenProcessToken"));
    }
    let token = OwnedHandle(token);
    let mut byte_len = 0;
    unsafe {
        GetTokenInformation(
            token.0,
            TokenIntegrityLevel,
            ptr::null_mut(),
            0,
            &mut byte_len,
        )
    };
    if byte_len == 0 {
        return Err(WindowsDeliveryError::last("GetTokenInformationSize"));
    }
    let mut buffer = vec![0_u8; byte_len as usize];
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenIntegrityLevel,
            buffer.as_mut_ptr().cast(),
            byte_len,
            &mut byte_len,
        )
    } == 0
    {
        return Err(WindowsDeliveryError::last("GetTokenInformation"));
    }
    let label = unsafe { &*(buffer.as_ptr() as *const TOKEN_MANDATORY_LABEL) };
    let count = unsafe { GetSidSubAuthorityCount(label.Label.Sid) };
    if count.is_null() || unsafe { *count } == 0 {
        return Err(WindowsDeliveryError::target("InvalidIntegritySid"));
    }
    let rid = unsafe { GetSidSubAuthority(label.Label.Sid, u32::from(*count) - 1) };
    if rid.is_null() {
        return Err(WindowsDeliveryError::target("InvalidIntegritySid"));
    }
    Ok(unsafe { *rid })
}

fn validate_window_identity(
    target_exists: bool,
    foreground: usize,
    expected: usize,
) -> Result<(), WindowsDeliveryError> {
    if !target_exists {
        return Err(WindowsDeliveryError::target("ForegroundTargetClosed"));
    }
    if foreground != expected {
        return Err(WindowsDeliveryError::target("ForegroundTargetChanged"));
    }
    Ok(())
}

fn validate_focused_control(
    expected: Option<usize>,
    current: Option<usize>,
) -> Result<(), WindowsDeliveryError> {
    if expected.is_some() && current != expected {
        return Err(WindowsDeliveryError::target("FocusedControlChanged"));
    }
    Ok(())
}

struct ClipboardGuard;

impl ClipboardGuard {
    fn open() -> Result<Self, WindowsDeliveryError> {
        // Clipboard contention is short-lived in practice. Retrying avoids a
        // large class of intermittent paste failures caused by other apps.
        for _ in 0..10 {
            // SAFETY: A null owner is valid and this thread closes the handle.
            if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
                return Ok(Self);
            }
            thread::sleep(Duration::from_millis(10));
        }
        Err(WindowsDeliveryError::last("OpenClipboard"))
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: This guard exists only after OpenClipboard succeeds.
        unsafe { CloseClipboard() };
    }
}

#[derive(Default)]
pub struct WindowsClipboard;

impl WindowsClipboard {
    fn write_text(text: Option<&str>) -> Result<(), WindowsDeliveryError> {
        let _guard = ClipboardGuard::open()?;
        // SAFETY: Clipboard is open on this thread.
        if unsafe { EmptyClipboard() } == 0 {
            return Err(WindowsDeliveryError::last("EmptyClipboard"));
        }
        let Some(text) = text else {
            return Ok(());
        };

        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_len = utf16.len() * std::mem::size_of::<u16>();
        // SAFETY: Allocates a movable block required by SetClipboardData.
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_len) };
        if handle.is_null() {
            return Err(WindowsDeliveryError::last("GlobalAlloc"));
        }
        // SAFETY: The allocation is valid and large enough for utf16.
        let destination = unsafe { GlobalLock(handle) } as *mut u8;
        if destination.is_null() {
            unsafe { GlobalFree(handle) };
            return Err(WindowsDeliveryError::last("GlobalLock"));
        }
        // SAFETY: Both buffers are valid for byte_len bytes and do not overlap.
        unsafe {
            ptr::copy_nonoverlapping(utf16.as_ptr() as *const u8, destination, byte_len);
            GlobalUnlock(handle);
        }
        // SAFETY: Ownership transfers to Windows only on success.
        if unsafe { SetClipboardData(CF_UNICODETEXT as u32, handle) }.is_null() {
            unsafe { GlobalFree(handle) };
            return Err(WindowsDeliveryError::last("SetClipboardData"));
        }
        Ok(())
    }
}

impl ClipboardPort for WindowsClipboard {
    type Snapshot = WindowsClipboardSnapshot;
    type Error = WindowsDeliveryError;

    fn snapshot(&mut self) -> Result<Self::Snapshot, Self::Error> {
        WindowsClipboardSnapshot::capture()
    }

    fn set_text(&mut self, text: &str) -> Result<(), Self::Error> {
        Self::write_text(Some(text))
    }

    fn restore(&mut self, snapshot: Self::Snapshot) -> Result<(), Self::Error> {
        snapshot.restore()
    }
}

pub struct WindowsPaste {
    settle_delay: Duration,
    target: Option<WindowsTarget>,
}

impl Default for WindowsPaste {
    fn default() -> Self {
        Self {
            // SendInput only queues keystrokes. Give the target time to read
            // staged clipboard content before the transaction restores it.
            settle_delay: Duration::from_millis(80),
            target: None,
        }
    }
}

impl WindowsPaste {
    pub fn for_target(target: WindowsTarget) -> Self {
        Self {
            target: Some(target),
            ..Self::default()
        }
    }
}

impl PastePort for WindowsPaste {
    type Error = WindowsDeliveryError;

    fn paste_at_caret(&mut self) -> Result<(), Self::Error> {
        if let Some(target) = self.target.as_ref() {
            target.validate_foreground()?;
            target.restore_text_selection()?;
        }
        let mut inputs = [
            keyboard_input(VK_CONTROL, 0),
            keyboard_input(VK_V, 0),
            keyboard_input(VK_V, KEYEVENTF_KEYUP),
            keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        // SAFETY: inputs points to initialized INPUT values for this call.
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if sent != inputs.len() as u32 {
            return Err(WindowsDeliveryError::last("SendInput"));
        }
        thread::sleep(self.settle_delay);
        Ok(())
    }
}

fn keyboard_input(key: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::deliver_text;
    use std::sync::mpsc;
    use windows_sys::core::w;
    use windows_sys::Win32::Graphics::Gdi::UpdateWindow;
    use windows_sys::Win32::System::DataExchange::{
        GetClipboardData, IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };
    use windows_sys::Win32::System::Memory::GlobalSize;
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetActiveWindow, SetFocus};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, CreateWindowExW, DispatchMessageW, GetMessageW, GetWindowTextLengthW,
        GetWindowTextW, PostMessageW, PostThreadMessageW, SendMessageW, SetForegroundWindow,
        SetWindowTextW, ShowWindow, SwitchToThisWindow, TranslateMessage, MSG, SW_SHOW, WM_CLOSE,
        WM_QUIT, WS_BORDER, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE,
    };

    const EM_SETSEL: u32 = 0x00B1;
    const EM_GETSEL: u32 = 0x00B0;
    const ES_MULTILINE: u32 = 0x0004;
    const WM_APP_ACTIVATE_EDIT: u32 = 0x8001;

    struct RestoreClipboard(Option<WindowsClipboardSnapshot>);

    impl Drop for RestoreClipboard {
        fn drop(&mut self) {
            if let Some(snapshot) = self.0.take() {
                let _ = snapshot.restore();
            }
        }
    }

    struct NativeEditWindow {
        window_hwnd: usize,
        edit_hwnd: usize,
        alternate_edit_hwnd: usize,
        thread_id: u32,
        join: Option<thread::JoinHandle<()>>,
    }

    impl NativeEditWindow {
        fn open() -> Self {
            let (sender, receiver) = mpsc::sync_channel(1);
            let join = thread::spawn(move || {
                let window_hwnd = unsafe {
                    CreateWindowExW(
                        0,
                        w!("STATIC"),
                        w!("PulseTalq delivery acceptance"),
                        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                        120,
                        120,
                        720,
                        280,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null(),
                    )
                };
                assert!(!window_hwnd.is_null(), "create native test window");
                let edit_hwnd = unsafe {
                    CreateWindowExW(
                        0,
                        w!("EDIT"),
                        w!(""),
                        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | ES_MULTILINE,
                        24,
                        24,
                        648,
                        84,
                        window_hwnd,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null(),
                    )
                };
                assert!(!edit_hwnd.is_null(), "create native child edit control");
                let alternate_edit_hwnd = unsafe {
                    CreateWindowExW(
                        0,
                        w!("EDIT"),
                        w!("alternate field"),
                        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | ES_MULTILINE,
                        24,
                        124,
                        648,
                        84,
                        window_hwnd,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null(),
                    )
                };
                assert!(
                    !alternate_edit_hwnd.is_null(),
                    "create alternate native child edit control"
                );
                unsafe {
                    ShowWindow(window_hwnd, SW_SHOW);
                    ShowWindow(edit_hwnd, SW_SHOW);
                    ShowWindow(alternate_edit_hwnd, SW_SHOW);
                    UpdateWindow(window_hwnd);
                }
                sender
                    .send((
                        window_hwnd as usize,
                        edit_hwnd as usize,
                        alternate_edit_hwnd as usize,
                        unsafe { GetCurrentThreadId() },
                    ))
                    .expect("publish native edit window");

                let mut message: MSG = unsafe { std::mem::zeroed() };
                while unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } > 0 {
                    if message.message == WM_APP_ACTIVATE_EDIT {
                        let control_hwnd = if message.wParam == 0 {
                            edit_hwnd
                        } else {
                            message.wParam as *mut std::ffi::c_void
                        };
                        unsafe {
                            ShowWindow(window_hwnd, SW_SHOW);
                            BringWindowToTop(window_hwnd);
                            SetForegroundWindow(window_hwnd);
                            SetActiveWindow(window_hwnd);
                            SetFocus(control_hwnd);
                        }
                        continue;
                    }
                    unsafe {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            });
            let (window_hwnd, edit_hwnd, alternate_edit_hwnd, thread_id) =
                receiver.recv().expect("receive native edit window");
            Self {
                window_hwnd,
                edit_hwnd,
                alternate_edit_hwnd,
                thread_id,
                join: Some(join),
            }
        }

        fn activate(&self) {
            self.activate_control(self.edit_hwnd);
        }

        fn activate_alternate(&self) {
            self.activate_control(self.alternate_edit_hwnd);
        }

        fn activate_control(&self, control_hwnd: usize) {
            let window_hwnd = self.window_hwnd as *mut std::ffi::c_void;
            let control_hwnd_pointer = control_hwnd as *mut std::ffi::c_void;
            let foreground = unsafe { GetForegroundWindow() };
            // Windows normally blocks background processes from stealing focus.
            // A synthetic Alt tap is the documented foreground hand-off used by
            // UI automation before SetForegroundWindow.
            if foreground != window_hwnd {
                let mut foreground_handoff = [
                    keyboard_input(0x12, 0),
                    keyboard_input(0x12, KEYEVENTF_KEYUP),
                ];
                unsafe {
                    SendInput(
                        foreground_handoff.len() as u32,
                        foreground_handoff.as_mut_ptr(),
                        std::mem::size_of::<INPUT>() as i32,
                    );
                }
            }
            let current_thread = unsafe { GetCurrentThreadId() };
            let mut foreground_process = 0;
            let foreground_thread = if foreground.is_null() {
                0
            } else {
                unsafe { GetWindowThreadProcessId(foreground, &mut foreground_process) }
            };
            unsafe {
                if foreground_thread != 0 && foreground_thread != current_thread {
                    AttachThreadInput(current_thread, foreground_thread, 1);
                }
                if self.thread_id != current_thread {
                    AttachThreadInput(current_thread, self.thread_id, 1);
                }
                ShowWindow(window_hwnd, SW_SHOW);
                BringWindowToTop(window_hwnd);
                SetActiveWindow(window_hwnd);
                SetForegroundWindow(window_hwnd);
                SwitchToThisWindow(window_hwnd, 1);
                SetFocus(control_hwnd_pointer);
                if self.thread_id != current_thread {
                    AttachThreadInput(current_thread, self.thread_id, 0);
                }
                if foreground_thread != 0 && foreground_thread != current_thread {
                    AttachThreadInput(current_thread, foreground_thread, 0);
                }
                PostThreadMessageW(self.thread_id, WM_APP_ACTIVATE_EDIT, control_hwnd, 0);
            }
            for _ in 0..50 {
                if unsafe { GetForegroundWindow() } as usize == self.window_hwnd
                    && focused_control(self.thread_id) == Some(control_hwnd)
                {
                    return;
                }
                thread::sleep(Duration::from_millis(20));
            }
            panic!(
                "native edit window did not become foreground and focused: expected={} foreground={} focus={}",
                control_hwnd,
                unsafe { GetForegroundWindow() } as usize,
                focused_control(self.thread_id).unwrap_or_default()
            );
        }

        fn set_text_and_selection(&self, text: &str, start: usize, end: usize) {
            let hwnd = self.edit_hwnd as *mut std::ffi::c_void;
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            assert_ne!(unsafe { SetWindowTextW(hwnd, wide.as_ptr()) }, 0);
            self.activate();
            self.set_selection(start, end);
        }

        fn set_selection(&self, start: usize, end: usize) {
            let hwnd = self.edit_hwnd as *mut std::ffi::c_void;
            unsafe { SendMessageW(hwnd, EM_SETSEL, start, end as isize) };
            let mut actual_start = 0_u32;
            let mut actual_end = 0_u32;
            unsafe {
                SendMessageW(
                    hwnd,
                    EM_GETSEL,
                    (&mut actual_start as *mut u32) as usize,
                    (&mut actual_end as *mut u32) as isize,
                )
            };
            assert_eq!((actual_start as usize, actual_end as usize), (start, end));
        }

        fn text(&self) -> String {
            let hwnd = self.edit_hwnd as *mut std::ffi::c_void;
            let length = unsafe { GetWindowTextLengthW(hwnd) };
            assert!(length >= 0);
            let mut buffer = vec![0_u16; length as usize + 1];
            let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
            String::from_utf16_lossy(&buffer[..copied as usize])
        }
    }

    impl Drop for NativeEditWindow {
        fn drop(&mut self) {
            let hwnd = self.window_hwnd as *mut std::ffi::c_void;
            unsafe {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
            }
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    fn write_custom_format(format: u32, bytes: &[u8]) {
        let _guard = ClipboardGuard::open().expect("open clipboard");
        assert_ne!(unsafe { EmptyClipboard() }, 0);
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
        assert!(!handle.is_null());
        let destination = unsafe { GlobalLock(handle) } as *mut u8;
        assert!(!destination.is_null());
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len());
            GlobalUnlock(handle);
        }
        if unsafe { SetClipboardData(format, handle) }.is_null() {
            unsafe { GlobalFree(handle) };
            panic!("set custom clipboard format");
        }
    }

    fn read_custom_format(format: u32) -> Vec<u8> {
        let _guard = ClipboardGuard::open().expect("open clipboard");
        assert_ne!(unsafe { IsClipboardFormatAvailable(format) }, 0);
        let handle = unsafe { GetClipboardData(format) };
        assert!(!handle.is_null());
        let size = unsafe { GlobalSize(handle) };
        let source = unsafe { GlobalLock(handle) } as *const u8;
        assert!(!source.is_null());
        let bytes = unsafe { std::slice::from_raw_parts(source, size) }.to_vec();
        unsafe { GlobalUnlock(handle) };
        bytes
    }

    #[test]
    #[ignore = "temporarily owns the real Windows clipboard"]
    fn restores_non_text_clipboard_formats() {
        let original = WindowsClipboardSnapshot::capture().expect("snapshot original clipboard");
        let mut original_guard = RestoreClipboard(Some(original));
        let format = unsafe { RegisterClipboardFormatW(w!("PulseTalq.RichClipboardTest")) };
        assert_ne!(format, 0);
        let expected = b"application-specific clipboard data";
        write_custom_format(format, expected);

        let snapshot = WindowsClipboardSnapshot::capture().expect("snapshot custom clipboard");
        WindowsClipboard::write_text(Some("temporary dictation text")).expect("stage text");
        snapshot.restore().expect("restore custom clipboard");

        assert_eq!(read_custom_format(format), expected);

        if let Some(original) = original_guard.0.take() {
            original.restore().expect("restore original clipboard");
        }
    }

    #[test]
    #[ignore = "opens a real Windows edit control and temporarily owns foreground and clipboard"]
    fn delivers_to_real_edit_caret_and_selection_without_losing_rich_clipboard() {
        let original = WindowsClipboardSnapshot::capture().expect("snapshot original clipboard");
        let mut original_guard = RestoreClipboard(Some(original));
        let format = unsafe { RegisterClipboardFormatW(w!("PulseTalq.LiveDeliveryTest")) };
        assert_ne!(format, 0);
        let expected_clipboard = b"rich application clipboard payload";
        write_custom_format(format, expected_clipboard);

        let window = NativeEditWindow::open();
        window.set_text_and_selection("alpha omega", 6, 6);
        let target = WindowsTarget::capture().expect("capture native edit target");
        let receipt = deliver_text(
            &mut WindowsClipboard,
            &mut WindowsPaste::for_target(target),
            "dictated ",
        )
        .expect("insert at native edit caret");
        assert!(receipt.pasted && receipt.clipboard_restored);
        assert_eq!(window.text(), "alpha dictated omega");
        assert_eq!(read_custom_format(format), expected_clipboard);

        window.set_text_and_selection("keep replace tail", 5, 12);
        let target = WindowsTarget::capture().expect("recapture native edit target");
        let selection = target
            .text_selection
            .as_ref()
            .expect("capture selected native edit text");
        assert_eq!(selection.start_and_length(), (5, 7));
        // Simulate a browser or overlay collapsing the visual selection while
        // speech is being transcribed. Delivery must restore the captured
        // range before it sends Ctrl+V.
        window.set_selection(12, 12);
        let receipt = deliver_text(
            &mut WindowsClipboard,
            &mut WindowsPaste::for_target(target),
            "dictated",
        )
        .expect("replace native edit selection");
        assert!(receipt.pasted && receipt.clipboard_restored);
        assert_eq!(window.text(), "keep dictated tail");
        assert_eq!(read_custom_format(format), expected_clipboard);

        if let Some(original) = original_guard.0.take() {
            original.restore().expect("restore original clipboard");
        }
    }

    #[test]
    #[ignore = "opens a real Windows window with two edit controls and temporarily owns foreground and clipboard"]
    fn refuses_native_delivery_after_focus_moves_to_another_control() {
        let original = WindowsClipboardSnapshot::capture().expect("snapshot original clipboard");
        let mut original_guard = RestoreClipboard(Some(original));
        let format = unsafe { RegisterClipboardFormatW(w!("PulseTalq.FocusSafetyTest")) };
        assert_ne!(format, 0);
        let expected_clipboard = b"focus safety clipboard payload";
        write_custom_format(format, expected_clipboard);

        let window = NativeEditWindow::open();
        window.set_text_and_selection("original field", 8, 8);
        let target = WindowsTarget::capture().expect("capture first native edit target");
        window.activate_alternate();

        let error = deliver_text(
            &mut WindowsClipboard,
            &mut WindowsPaste::for_target(target),
            "must not paste",
        )
        .expect_err("delivery must fail after focus moves within the window");

        assert!(error.to_string().contains("FocusedControlChanged"));
        assert_eq!(window.text(), "original field");
        assert_eq!(read_custom_format(format), expected_clipboard);

        if let Some(original) = original_guard.0.take() {
            original.restore().expect("restore original clipboard");
        }
    }

    #[test]
    #[ignore = "pastes into the current foreground app and temporarily owns the Windows clipboard"]
    fn delivers_test_text_to_current_foreground_app() {
        let delay_ms = std::env::var("PULSETALK_DELIVERY_TEST_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5_000);
        let text = std::env::var("PULSETALK_DELIVERY_TEST_TEXT")
            .unwrap_or_else(|_| "PulseTalq acceptance text".to_string());
        thread::sleep(Duration::from_millis(delay_ms));

        let original = WindowsClipboardSnapshot::capture().expect("snapshot original clipboard");
        let mut original_guard = RestoreClipboard(Some(original));
        let format = unsafe { RegisterClipboardFormatW(w!("PulseTalq.ForegroundDeliveryTest")) };
        assert_ne!(format, 0);
        let expected_clipboard = b"foreground delivery clipboard payload";
        write_custom_format(format, expected_clipboard);

        let target = WindowsTarget::capture().expect("capture foreground app target");
        let target_process = target.process_label();
        let receipt = deliver_text(
            &mut WindowsClipboard,
            &mut WindowsPaste::for_target(target),
            &text,
        )
        .expect("deliver into foreground app");
        assert!(receipt.pasted && receipt.clipboard_restored);
        assert_eq!(read_custom_format(format), expected_clipboard);
        println!("delivered_to={target_process}");

        if let Some(original) = original_guard.0.take() {
            original.restore().expect("restore original clipboard");
        }
    }

    #[test]
    fn refuses_to_paste_after_the_target_closes() {
        let error = validate_window_identity(false, 41, 41).unwrap_err();
        assert_eq!(error.operation(), "ForegroundTargetClosed");
    }

    #[test]
    fn refuses_to_paste_after_focus_moves_to_another_window() {
        let error = validate_window_identity(true, 42, 41).unwrap_err();
        assert_eq!(error.operation(), "ForegroundTargetChanged");
    }

    #[test]
    fn accepts_the_original_foreground_window() {
        assert!(validate_window_identity(true, 41, 41).is_ok());
    }

    #[test]
    fn refuses_to_paste_after_focus_moves_within_the_original_window() {
        let error = validate_focused_control(Some(51), Some(52)).unwrap_err();
        assert_eq!(error.operation(), "FocusedControlChanged");
    }

    #[test]
    fn refuses_to_paste_when_captured_focus_can_no_longer_be_read() {
        let error = validate_focused_control(Some(51), None).unwrap_err();
        assert_eq!(error.operation(), "FocusedControlChanged");
    }

    #[test]
    fn accepts_the_original_focused_control_or_an_untracked_focus() {
        assert!(validate_focused_control(Some(51), Some(51)).is_ok());
        assert!(validate_focused_control(None, Some(52)).is_ok());
        assert!(validate_focused_control(None, None).is_ok());
    }

    #[test]
    fn text_element_identity_requires_the_same_control_bounds() {
        let captured = TextElementIdentity {
            process_id: 4,
            control_type: 50004,
            automation_id: String::new(),
            class_name: "Chrome_RenderWidgetHostHWND".into(),
            bounds: (100, 200, 700, 250),
        };
        let mut other_field = captured.clone();
        other_field.bounds = (100, 300, 700, 350);
        assert!(!captured.matches(&other_field));

        let mut stable_id = captured.clone();
        stable_id.automation_id = "searchbox".into();
        let mut moved_stable_id = stable_id.clone();
        moved_stable_id.bounds = (120, 220, 720, 270);
        assert!(!stable_id.matches(&moved_stable_id));
    }

    #[test]
    fn focused_control_anchor_uses_the_center_of_its_screen_rectangle() {
        assert_eq!(
            rectangle_center(RECT {
                left: -1200,
                top: 80,
                right: -400,
                bottom: 680,
            }),
            (-800.0, 380.0)
        );
    }

    #[test]
    fn refuses_a_higher_integrity_target() {
        let error = validate_integrity_levels(0x2000, 0x3000).unwrap_err();
        assert_eq!(error.operation(), "ElevatedTarget");
    }

    #[test]
    fn accepts_same_or_lower_integrity_targets() {
        assert!(validate_integrity_levels(0x2000, 0x2000).is_ok());
        assert!(validate_integrity_levels(0x3000, 0x2000).is_ok());
    }
}
