use super::{ClipboardPort, PastePort};
use std::fmt;
use std::ptr;
use std::thread;
use std::time::Duration;
use windows_sys::Win32::Foundation::{GetLastError, GlobalFree};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
};

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
    fn read_text() -> Result<Option<String>, WindowsDeliveryError> {
        let _guard = ClipboardGuard::open()?;
        // SAFETY: Clipboard is open on this thread.
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT as u32) } == 0 {
            return Ok(None);
        }
        // SAFETY: CF_UNICODETEXT remains owned by the clipboard; we only read.
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT as u32) };
        if handle.is_null() {
            return Err(WindowsDeliveryError::last("GetClipboardData"));
        }
        // SAFETY: GlobalLock accepts the movable global-memory clipboard handle.
        let pointer = unsafe { GlobalLock(handle) } as *const u16;
        if pointer.is_null() {
            return Err(WindowsDeliveryError::last("GlobalLock"));
        }
        let units = unsafe { GlobalSize(handle) } / std::mem::size_of::<u16>();
        // SAFETY: GlobalSize bounds this allocation; CF_UNICODETEXT is UTF-16.
        let slice = unsafe { std::slice::from_raw_parts(pointer, units) };
        let end = slice
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(slice.len());
        let text = String::from_utf16_lossy(&slice[..end]);
        // SAFETY: Balances the successful GlobalLock above.
        unsafe { GlobalUnlock(handle) };
        Ok(Some(text))
    }

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
    type Snapshot = Option<String>;
    type Error = WindowsDeliveryError;

    fn snapshot(&mut self) -> Result<Self::Snapshot, Self::Error> {
        Self::read_text()
    }

    fn set_text(&mut self, text: &str) -> Result<(), Self::Error> {
        Self::write_text(Some(text))
    }

    fn restore(&mut self, snapshot: Self::Snapshot) -> Result<(), Self::Error> {
        Self::write_text(snapshot.as_deref())
    }
}

pub struct WindowsPaste {
    settle_delay: Duration,
}

impl Default for WindowsPaste {
    fn default() -> Self {
        Self {
            // SendInput only queues keystrokes. Give the target time to read
            // staged clipboard content before the transaction restores it.
            settle_delay: Duration::from_millis(80),
        }
    }
}

impl PastePort for WindowsPaste {
    type Error = WindowsDeliveryError;

    fn paste_at_caret(&mut self) -> Result<(), Self::Error> {
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
