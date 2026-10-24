//! Read-only CoreAudio process activity. No audio samples or window contents
//! are captured by the detector. Browser URLs are checked locally via Apple Events.
use super::signals;
use std::{
    collections::BTreeSet,
    ffi::c_void,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}
fn address(selector: &[u8; 4]) -> Address {
    Address {
        selector: u32::from_be_bytes(*selector),
        scope: u32::from_be_bytes(*b"glob"),
        element: 0,
    }
}
#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectHasProperty(object: u32, address: *const Address) -> u8;
    fn AudioObjectGetPropertyDataSize(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> i32;
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringGetLength(string: *const c_void) -> isize;
    fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    fn CFStringGetCString(string: *const c_void, buffer: *mut u8, size: isize, encoding: u32)
        -> u8;
    fn CFRelease(object: *const c_void);
}

pub fn supported() -> bool {
    // The selector is absent before macOS 14.2. Checking it avoids calling
    // APIs unavailable on older systems without changing Meetily's minimum OS.
    unsafe { AudioObjectHasProperty(1, &address(b"prs#")) != 0 }
}

fn flag(object: u32, selector: &[u8; 4]) -> Result<bool, String> {
    let mut value = 0u32;
    let mut size = 4;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            &address(selector),
            0,
            std::ptr::null(),
            &mut size,
            (&mut value as *mut u32).cast(),
        )
    };
    if status != 0 {
        return Err(format!("Audio process disappeared ({status})"));
    }
    Ok(value != 0)
}

fn bundle_id(object: u32) -> Result<String, String> {
    let mut value: *const c_void = std::ptr::null();
    let mut size = std::mem::size_of_val(&value) as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            &address(b"pbid"),
            0,
            std::ptr::null(),
            &mut size,
            (&mut value as *mut *const c_void).cast(),
        )
    };
    if status != 0 || value.is_null() {
        return Err("Cannot identify audio process".into());
    }
    // The CoreAudio header transfers ownership of this CFString to the caller.
    let text = unsafe {
        let capacity = CFStringGetMaximumSizeForEncoding(CFStringGetLength(value), 0x08000100) + 1;
        let mut bytes = vec![0u8; capacity as usize];
        let ok = CFStringGetCString(value, bytes.as_mut_ptr(), capacity, 0x08000100);
        CFRelease(value);
        if ok == 0 {
            return Err("Cannot decode audio process identity".into());
        }
        let length = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        bytes.truncate(length);
        String::from_utf8(bytes).map_err(|_| "Invalid audio process identity")?
    };
    Ok(text)
}

fn browser_has_call(id: &str, all_tabs: bool) -> Result<bool, String> {
    // IDs come exclusively from the fixed allowlist, never from a URL or process.
    let selection = if all_tabs {
        "repeat with w in windows\nrepeat with t in tabs of w\nset output to output & (URL of t) & linefeed\nend repeat\nend repeat"
    } else if id == "com.apple.Safari" {
        "if (count windows) > 0 then set output to URL of current tab of front window"
    } else {
        "if (count windows) > 0 then set output to URL of active tab of front window"
    };
    let script = format!("with timeout of 2 seconds\ntell application id \"{id}\"\nset output to \"\"\n{selection}\nreturn output\nend tell\nend timeout");
    let mut child = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                let output = child.wait_with_output().map_err(|e| e.to_string())?;
                if !status.success() {
                    return Err("Allow browser automation in System Settings > Privacy & Security > Automation".into());
                }
                return Ok(String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(signals::conference_url));
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Browser detection timed out".into());
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}

/// Once recording, output-only activity keeps a muted call alive. At initial
/// detection a supported app must be capturing input, not merely playing sound.
pub fn probe(recording_ours: bool, browsers_enabled: bool) -> Result<Option<String>, String> {
    let mut size = 0;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(1, &address(b"prs#"), 0, std::ptr::null(), &mut size)
    };
    if status != 0 {
        return Err(format!("Cannot read audio processes ({status})"));
    }
    let mut objects = vec![0u32; size as usize / 4];
    let status = unsafe {
        AudioObjectGetPropertyData(
            1,
            &address(b"prs#"),
            0,
            std::ptr::null(),
            &mut size,
            objects.as_mut_ptr().cast(),
        )
    };
    if status != 0 {
        return Err("Audio process list changed; retrying".into());
    }
    objects.truncate(size as usize / 4);
    let mut browsers = BTreeSet::new();
    let mut uncertain = false;
    for object in objects {
        let active = match flag(object, b"piri") {
            Ok(true) => true,
            Ok(false) if recording_ours => flag(object, b"piro").unwrap_or(false),
            Ok(false) => false,
            Err(_) => {
                uncertain = true;
                continue;
            }
        };
        if !active {
            continue;
        }
        let Ok(bundle) = bundle_id(object) else {
            uncertain = true;
            continue;
        };
        if let Some(label) = signals::desktop_app(&bundle) {
            return Ok(Some(label.into()));
        }
        if browsers_enabled {
            if let Some(id) = signals::browser(&bundle) {
                browsers.insert(id);
            }
        }
    }
    for id in browsers {
        match browser_has_call(id, recording_ours) {
            Ok(true) => return Ok(Some("Browser call".into())),
            Ok(false) => {}
            Err(_) => uncertain = true,
        }
    }
    if uncertain {
        Err("Call detection unavailable; retaining the current recording".into())
    } else {
        Ok(None)
    }
}
