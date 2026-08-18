//! Chooses the macOS Outlook connector that the current machine can actually use.
//!
//! Apple Events (Automation) is the default because consent is recorded in the
//! per-user TCC database and therefore needs no administrator password. The
//! Accessibility connector stays available as a fallback for New Outlook, whose
//! AppleScript support Microsoft has not shipped, and for machines where an
//! administrator has already approved Accessibility.

use super::{
    local_outlook::{
        LocalOutlookCalendarStatus, LocalOutlookMeeting, PERMISSION_DENIED, PERMISSION_GRANTED,
        PERMISSION_UNDETERMINED, PERMISSION_UNKNOWN,
    },
    macos_outlook,
    macos_outlook_events::{self, AutomationPermission},
};

pub const PROVIDER_AUTOMATION: &str = "macos-outlook-automation";
pub const PROVIDER_ACCESSIBILITY: &str = "macos-outlook-accessibility";

pub const PERMISSION_AUTOMATION: &str = "automation";
pub const PERMISSION_ACCESSIBILITY: &str = "accessibility";

/// True when Accessibility is the only connector that can work here: New
/// Outlook for Mac still has no AppleScript support.
fn accessibility_only() -> bool {
    macos_outlook_events::is_new_outlook_mode()
}

pub fn status() -> LocalOutlookCalendarStatus {
    let installed = macos_outlook::outlook_installed();
    let running = macos_outlook::outlook_pid().is_some();

    if !installed {
        return LocalOutlookCalendarStatus {
            supported: true,
            installed: false,
            running: false,
            permission: PERMISSION_AUTOMATION,
            permission_state: PERMISSION_UNKNOWN,
            requires_admin: false,
            provider: PROVIDER_AUTOMATION,
            detail: "Microsoft Outlook for Mac is not installed.".to_string(),
        };
    }

    if accessibility_only() {
        let mut status = macos_outlook::status();
        status.detail = if status.permission_state == PERMISSION_GRANTED {
            "New Outlook is active. Memento reads the visible Calendar through Accessibility."
                .to_string()
        } else {
            "New Outlook is active, and it does not support local calendar automation. Switch off New Outlook to read the calendar without an administrator, or ask an administrator to allow Memento under Accessibility."
                .to_string()
        };
        return status;
    }

    let (permission_state, detail) = match macos_outlook_events::automation_permission(false) {
        AutomationPermission::Granted => (
            PERMISSION_GRANTED,
            "Outlook automation is allowed. Memento reads the calendar locally, without bringing Outlook to the front."
                .to_string(),
        ),
        AutomationPermission::Denied => (
            PERMISSION_DENIED,
            "Outlook automation is turned off for Memento. Turn on Microsoft Outlook for Memento in System Settings → Privacy & Security → Automation. No administrator password is required."
                .to_string(),
        ),
        AutomationPermission::NotDetermined => (
            PERMISSION_UNDETERMINED,
            "Memento needs your permission to read the Outlook calendar. Approving it takes one click and no administrator password."
                .to_string(),
        ),
        // macOS cannot resolve the target until Outlook runs. The calendar read
        // starts Outlook in the background first, so this is not a failure.
        AutomationPermission::TargetNotRunning => (
            PERMISSION_UNKNOWN,
            "Outlook is installed. Memento starts it in the background and asks for permission once."
                .to_string(),
        ),
        AutomationPermission::Unknown(code) => (
            PERMISSION_UNKNOWN,
            format!("macOS could not report the Outlook automation permission ({code})."),
        ),
    };

    LocalOutlookCalendarStatus {
        supported: true,
        installed,
        running,
        permission: PERMISSION_AUTOMATION,
        permission_state,
        requires_admin: false,
        provider: PROVIDER_AUTOMATION,
        detail,
    }
}

pub fn request_permission() -> Result<LocalOutlookCalendarStatus, String> {
    if accessibility_only() {
        macos_outlook::request_accessibility_permission()?;
    } else {
        macos_outlook_events::request_permission()?;
    }
    Ok(status())
}

pub fn upcoming_meetings(
    days: u32,
    include_attendees: bool,
) -> Result<Vec<LocalOutlookMeeting>, String> {
    if accessibility_only() {
        return macos_outlook::upcoming_meetings(days);
    }

    match macos_outlook_events::upcoming_meetings(days, include_attendees) {
        Ok(meetings) => Ok(meetings),
        Err(error) => {
            // Accessibility is a strictly worse connector — it moves Outlook's
            // UI around and only sees the rendered week — so it is used only
            // when an administrator has already approved it anyway.
            if macos_outlook::accessibility_trusted(false) {
                macos_outlook::upcoming_meetings(days).map_err(|fallback| {
                    format!("{error} Accessibility fallback also failed: {fallback}")
                })
            } else {
                Err(error)
            }
        }
    }
}
