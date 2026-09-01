use super::{ClipboardPort, PastePort};
use std::fmt;
use std::ptr;
use std::thread;
use std::time::Duration;
use windows_sys::Win32::Foundation::{GetLastError, GlobalFree, HANDLE};
use windows_sys::Win32::Graphics::Gdi::{DeleteEnhMetaFile, DeleteMetaFile, DeleteObject};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
    SetClipboardData, METAFILEPICT,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Ole::{
    OleDuplicateData, CF_BITMAP, CF_DSPBITMAP, CF_DSPENHMETAFILE, CF_DSPMETAFILEPICT,
    CF_ENHMETAFILE, CF_METAFILEPICT, CF_PALETTE, CF_UNICODETEXT,
};
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
            // then make a snapshot owned by PulseTalk.
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

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::core::w;
    use windows_sys::Win32::System::DataExchange::{
        GetClipboardData, IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };
    use windows_sys::Win32::System::Memory::GlobalSize;

    struct RestoreClipboard(Option<WindowsClipboardSnapshot>);

    impl Drop for RestoreClipboard {
        fn drop(&mut self) {
            if let Some(snapshot) = self.0.take() {
                let _ = snapshot.restore();
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
        let format = unsafe { RegisterClipboardFormatW(w!("PulseTalk.RichClipboardTest")) };
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
}
