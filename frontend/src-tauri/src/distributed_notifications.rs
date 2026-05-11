// distributed_notifications.rs
//
// macOS Distributed Notifications for external integration.
// Posts events to NSDistributedNotificationCenter so external apps
// can react to Meetily recording lifecycle events.
//
// This module is separate from the user-facing `notifications/` module,
// which handles OS toast notifications with consent management.
// Distributed notifications are developer-facing IPC events.

use std::collections::HashMap;

/// Post a distributed notification that recording has started.
pub fn post_recording_started(meeting_name: &str) {
    let mut info = HashMap::new();
    info.insert("meeting_name".to_string(), meeting_name.to_string());
    post_distributed_notification("com.meetily.ai.recording.started", &info);
}

/// Post a distributed notification that recording has stopped.
pub fn post_recording_stopped(meeting_name: &str, folder_path: &str) {
    let mut info = HashMap::new();
    info.insert("meeting_name".to_string(), meeting_name.to_string());
    info.insert("folder_path".to_string(), folder_path.to_string());
    post_distributed_notification("com.meetily.ai.recording.stopped", &info);
}

/// Post a distributed notification that a recording error occurred.
pub fn post_recording_error(error: &str) {
    let mut info = HashMap::new();
    info.insert("error".to_string(), error.to_string());
    post_distributed_notification("com.meetily.ai.recording.error", &info);
}

/// Post a distributed notification that transcription has completed.
pub fn post_transcription_completed(meeting_name: &str, folder_path: &str) {
    let mut info = HashMap::new();
    info.insert("meeting_name".to_string(), meeting_name.to_string());
    info.insert("folder_path".to_string(), folder_path.to_string());
    post_distributed_notification("com.meetily.ai.transcription.completed", &info);
}

#[cfg(target_os = "macos")]
fn post_distributed_notification(name: &str, user_info: &HashMap<String, String>) {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    log::info!("📡 Posting distributed notification: {}", name);

    unsafe {
        // SAFETY: NSDistributedNotificationCenter is a well-known Foundation class
        // that is always available on macOS. defaultCenter returns a singleton.
        let center_class = match Class::get("NSDistributedNotificationCenter") {
            Some(cls) => cls,
            None => {
                log::warn!("NSDistributedNotificationCenter class not found");
                return;
            }
        };
        let center: *mut Object = msg_send![center_class, defaultCenter];
        if center.is_null() {
            log::warn!("NSDistributedNotificationCenter defaultCenter returned null");
            return;
        }

        // SAFETY: NSString is a well-known Foundation class. initWithBytes:length:encoding:
        // creates a new NSString from a UTF-8 byte buffer. NSUTF8StringEncoding = 4.
        let nsstring_class = match Class::get("NSString") {
            Some(cls) => cls,
            None => {
                log::warn!("NSString class not found");
                return;
            }
        };

        let name_nsstring = create_nsstring(nsstring_class, name);
        if name_nsstring.is_null() {
            log::warn!("Failed to create NSString for notification name");
            return;
        }

        // SAFETY: NSMutableDictionary is a well-known Foundation class.
        // We create a mutable dictionary and populate it with the user_info entries.
        let dict_class = match Class::get("NSMutableDictionary") {
            Some(cls) => cls,
            None => {
                log::warn!("NSMutableDictionary class not found");
                return;
            }
        };
        let dict: *mut Object = msg_send![dict_class, new];
        if dict.is_null() {
            log::warn!("Failed to create NSMutableDictionary");
            return;
        }

        for (key, value) in user_info {
            let key_ns = create_nsstring(nsstring_class, key);
            let val_ns = create_nsstring(nsstring_class, value);
            if !key_ns.is_null() && !val_ns.is_null() {
                // SAFETY: setObject:forKey: is a standard NSMutableDictionary method.
                let _: () = msg_send![dict, setObject: val_ns forKey: key_ns];
            }
        }

        // SAFETY: postNotificationName:object:userInfo:deliverImmediately: is the standard
        // method for posting distributed notifications. deliverImmediately:YES ensures
        // the notification is delivered even if the app is in the background.
        let null_ptr: *mut Object = std::ptr::null_mut();
        let yes: bool = true;
        let _: () = msg_send![
            center,
            postNotificationName: name_nsstring
            object: null_ptr
            userInfo: dict
            deliverImmediately: yes
        ];

        log::info!("📡 Distributed notification posted: {}", name);
    }
}

#[cfg(target_os = "macos")]
unsafe fn create_nsstring(nsstring_class: &objc::runtime::Class, s: &str) -> *mut objc::runtime::Object {
    use objc::{msg_send, sel, sel_impl};

    let bytes = s.as_bytes();
    // SAFETY: alloc + initWithBytes:length:encoding: is the standard way to create an
    // NSString from a Rust &str. NSUTF8StringEncoding = 4.
    let alloc: *mut objc::runtime::Object = msg_send![nsstring_class, alloc];
    if alloc.is_null() {
        return std::ptr::null_mut();
    }
    let nsstring: *mut objc::runtime::Object = msg_send![
        alloc,
        initWithBytes: bytes.as_ptr()
        length: bytes.len()
        encoding: 4usize // NSUTF8StringEncoding
    ];
    nsstring
}

#[cfg(not(target_os = "macos"))]
fn post_distributed_notification(_name: &str, _user_info: &HashMap<String, String>) {
    // Distributed notifications are macOS-only. No-op on other platforms.
}
