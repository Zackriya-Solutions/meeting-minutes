//! Read-only Outlook for Mac calendar access through macOS Accessibility.
//!
//! The connector never talks to Exchange and never reads Outlook databases.
//! It activates the locally installed Outlook application, attempts to expose
//! the current week in Calendar, and reads VoiceOver labels from the native
//! Accessibility tree. Only labels that parse as calendar events are returned.

use std::{
    collections::HashSet,
    ffi::{c_void, CStr, CString},
    path::Path,
    process::Command,
    ptr, thread,
    time::Duration as StdDuration,
};

use chrono::{DateTime, Duration, Local, LocalResult, TimeZone};
use core_foundation_sys::{
    array::{CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFArrayRef},
    base::{
        kCFAllocatorDefault, Boolean, CFGetTypeID, CFHash, CFIndex, CFRelease, CFTypeID, CFTypeRef,
    },
    dictionary::{
        kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreate,
        CFDictionaryRef,
    },
    number::kCFBooleanTrue,
    string::{
        kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringGetCString, CFStringGetLength,
        CFStringGetMaximumSizeForEncoding, CFStringGetTypeID, CFStringRef,
    },
};
use once_cell::sync::Lazy;
use regex::Regex;
use sha2::{Digest, Sha256};

use super::local_outlook::{LocalOutlookCalendarStatus, LocalOutlookMeeting};

type AXUIElementRef = CFTypeRef;
type AXError = i32;

const AX_ERROR_SUCCESS: AXError = 0;
const MAX_AX_NODES: usize = 8_000;
const MAX_AX_DEPTH: usize = 32;
const AX_CHILD_COLLECTIONS: [&str; 8] = [
    "AXChildren",
    "AXWindows",
    "AXSections",
    "AXContents",
    "AXRows",
    "AXColumns",
    "AXVisibleChildren",
    "AXChildrenInNavigationOrder",
];

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> Boolean;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_in_seconds: f32) -> AXError;
}

static RUSSIAN_DATE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?P<day>\d{1,2})\s+(?P<month>января|февраля|марта|апреля|мая|июня|июля|августа|сентября|октября|ноября|декабря)\s+(?P<year>\d{4})",
    )
    .expect("valid Russian Outlook date regex")
});

static ENGLISH_DATE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(?P<month>January|February|March|April|May|June|July|August|September|October|November|December)\s+(?P<day>\d{1,2})(?:st|nd|rd|th)?[,]?\s+(?P<year>\d{4})",
    )
    .expect("valid English Outlook date regex")
});

static EVENT_TIME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?P<hour>\d{1,2})[:.](?P<minute>\d{2})(?:\s*(?P<ampm>a\.?m\.?|p\.?m\.?))?")
        .expect("valid Outlook time regex")
});

static LOCATION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:местоположение|расположение|location)\s*:\s*(?P<location>[^,;]+)")
        .expect("valid Outlook location regex")
});

#[derive(Default)]
struct AxSnapshot {
    labels: Vec<String>,
    visited: HashSet<usize>,
    node_count: usize,
}

struct ParsedDate {
    day: u32,
    month: u32,
    year: i32,
    start: usize,
}

pub fn status() -> LocalOutlookCalendarStatus {
    let installed = outlook_installed();
    let running = outlook_pid().is_some();
    let accessibility_granted = accessibility_trusted(false);

    let detail = if !installed {
        "Microsoft Outlook for Mac is not installed.".to_string()
    } else if !accessibility_granted {
        "Allow Memento in System Settings → Privacy & Security → Accessibility, then return and refresh."
            .to_string()
    } else if !running {
        "Outlook is installed. Memento will open it and read the visible Calendar accessibility labels."
            .to_string()
    } else {
        "Outlook is running and Memento has Accessibility permission. Calendar labels can be read locally."
            .to_string()
    };

    LocalOutlookCalendarStatus {
        supported: true,
        installed,
        running,
        permission: super::macos_provider::PERMISSION_ACCESSIBILITY,
        permission_state: if accessibility_granted {
            super::local_outlook::PERMISSION_GRANTED
        } else {
            super::local_outlook::PERMISSION_UNDETERMINED
        },
        requires_admin: !accessibility_granted,
        provider: super::macos_provider::PROVIDER_ACCESSIBILITY,
        detail,
    }
}

pub fn request_accessibility_permission() -> Result<LocalOutlookCalendarStatus, String> {
    let _ = accessibility_trusted(true);
    Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map_err(|error| format!("cannot open Accessibility settings: {error}"))?;
    Ok(status())
}

pub fn upcoming_meetings(days: u32) -> Result<Vec<LocalOutlookMeeting>, String> {
    if !outlook_installed() {
        return Err("Microsoft Outlook for Mac is not installed.".to_string());
    }
    if !accessibility_trusted(false) {
        let _ = accessibility_trusted(true);
        return Err(
            "Accessibility permission is required. Allow Memento in System Settings → Privacy & Security → Accessibility."
                .to_string(),
        );
    }

    let pid = ensure_outlook_running()?;
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return Err("macOS did not return an Accessibility element for Outlook.".to_string());
    }

    unsafe {
        let _ = AXUIElementSetMessagingTimeout(app, 5.0);
    }

    // Best-effort, read-only navigation. Exact button labels vary across the
    // Russian/English and classic/new Outlook UIs, so failure here is harmless:
    // we still inspect the currently visible calendar view.
    let _ = press_first_matching(app, &["calendar", "календарь"]);
    thread::sleep(StdDuration::from_millis(450));
    let _ = press_first_matching(app, &["today", "сегодня"]);
    thread::sleep(StdDuration::from_millis(350));
    let _ = press_first_matching(app, &["week", "неделя"]);
    thread::sleep(StdDuration::from_millis(500));

    let mut snapshot = AxSnapshot::default();
    unsafe {
        collect_labels(app, 0, &mut snapshot);
        CFRelease(app);
    }

    if snapshot.node_count == 0 {
        return Err(
            "Outlook did not expose its window through Accessibility. Open Outlook Calendar and try again."
                .to_string(),
        );
    }

    let now = Local::now();
    let range_end = now + Duration::days(i64::from(days));
    let mut meetings = snapshot
        .labels
        .iter()
        .filter_map(|label| parse_event_label(label, now, range_end))
        .collect::<Vec<_>>();

    meetings.sort_by(|left, right| {
        left.start_at
            .cmp(&right.start_at)
            .then_with(|| left.subject.cmp(&right.subject))
    });
    meetings.dedup_by(|left, right| left.id == right.id);
    Ok(meetings)
}

pub(super) fn outlook_installed() -> bool {
    let system_path = Path::new("/Applications/Microsoft Outlook.app");
    let user_path = dirs::home_dir()
        .map(|home| home.join("Applications/Microsoft Outlook.app"))
        .is_some_and(|path| path.exists());
    system_path.exists() || user_path
}

pub(super) fn outlook_pid() -> Option<i32> {
    let output = Command::new("/usr/bin/pgrep")
        .args(["-x", "Microsoft Outlook"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.trim().parse::<i32>().ok())
}

fn ensure_outlook_running() -> Result<i32, String> {
    if let Some(pid) = outlook_pid() {
        return Ok(pid);
    }

    let status = Command::new("/usr/bin/open")
        .args(["-a", "Microsoft Outlook"])
        .status()
        .map_err(|error| format!("cannot start Microsoft Outlook: {error}"))?;
    if !status.success() {
        return Err("macOS could not start Microsoft Outlook.".to_string());
    }

    for _ in 0..24 {
        if let Some(pid) = outlook_pid() {
            thread::sleep(StdDuration::from_millis(750));
            return Ok(pid);
        }
        thread::sleep(StdDuration::from_millis(250));
    }
    Err("Microsoft Outlook did not finish starting.".to_string())
}

pub(super) fn accessibility_trusted(prompt: bool) -> bool {
    unsafe {
        if !prompt {
            return AXIsProcessTrusted() != 0;
        }

        let keys = [kAXTrustedCheckOptionPrompt as *const c_void];
        let values = [kCFBooleanTrue as *const c_void];
        let options = CFDictionaryCreate(
            kCFAllocatorDefault,
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        if options.is_null() {
            return AXIsProcessTrusted() != 0;
        }
        let trusted = AXIsProcessTrustedWithOptions(options) != 0;
        CFRelease(options as CFTypeRef);
        trusted
    }
}

fn press_first_matching(app: AXUIElementRef, labels: &[&str]) -> bool {
    let mut visited = HashSet::new();
    let mut node_count = 0;
    unsafe { find_and_press(app, 0, labels, &mut visited, &mut node_count) }
}

unsafe fn find_and_press(
    element: AXUIElementRef,
    depth: usize,
    labels: &[&str],
    visited: &mut HashSet<usize>,
    node_count: &mut usize,
) -> bool {
    if element.is_null() {
        return false;
    }
    let identity = CFHash(element);
    if depth > MAX_AX_DEPTH || *node_count >= MAX_AX_NODES || !visited.insert(identity) {
        return false;
    }
    *node_count += 1;

    let role = copy_string_attribute(element, "AXRole").unwrap_or_default();
    if matches!(role.as_str(), "AXButton" | "AXRadioButton" | "AXMenuButton") {
        for attribute in ["AXTitle", "AXDescription", "AXValue"] {
            if let Some(value) = copy_string_attribute(element, attribute) {
                let normalized = value.trim().to_lowercase();
                if labels.iter().any(|label| normalized == *label)
                    && perform_action(element, "AXPress")
                {
                    return true;
                }
            }
        }
    }

    for collection in AX_CHILD_COLLECTIONS {
        if let Some(children) = copy_attribute(element, collection) {
            let mut pressed = false;
            if CFGetTypeID(children) == CFArrayGetTypeID() {
                let array = children as CFArrayRef;
                let count = CFArrayGetCount(array);
                for index in 0..count {
                    let child = CFArrayGetValueAtIndex(array, index) as AXUIElementRef;
                    if !child.is_null()
                        && CFGetTypeID(child) == AXUIElementGetTypeID()
                        && find_and_press(child, depth + 1, labels, visited, node_count)
                    {
                        pressed = true;
                        break;
                    }
                }
            }
            CFRelease(children);
            if pressed {
                return true;
            }
        }
    }
    false
}

unsafe fn collect_labels(element: AXUIElementRef, depth: usize, snapshot: &mut AxSnapshot) {
    if element.is_null() {
        return;
    }
    let identity = CFHash(element);
    if depth > MAX_AX_DEPTH
        || snapshot.node_count >= MAX_AX_NODES
        || !snapshot.visited.insert(identity)
    {
        return;
    }
    snapshot.node_count += 1;

    for attribute in ["AXDescription", "AXTitle", "AXValue", "AXHelp"] {
        if let Some(value) = copy_string_attribute(element, attribute) {
            let trimmed = value.trim();
            if !trimmed.is_empty()
                && trimmed.len() <= 2_048
                && !snapshot.labels.iter().any(|existing| existing == trimmed)
            {
                snapshot.labels.push(trimmed.to_string());
            }
        }
    }

    for collection in AX_CHILD_COLLECTIONS {
        if let Some(children) = copy_attribute(element, collection) {
            if CFGetTypeID(children) == CFArrayGetTypeID() {
                let array = children as CFArrayRef;
                let count = CFArrayGetCount(array);
                for index in 0..count {
                    let child = CFArrayGetValueAtIndex(array, index) as AXUIElementRef;
                    if !child.is_null() && CFGetTypeID(child) == AXUIElementGetTypeID() {
                        collect_labels(child, depth + 1, snapshot);
                    }
                }
            }
            CFRelease(children);
        }
    }
}

unsafe fn perform_action(element: AXUIElementRef, action: &str) -> bool {
    let Ok(action) = CString::new(action) else {
        return false;
    };
    let action_ref =
        CFStringCreateWithCString(kCFAllocatorDefault, action.as_ptr(), kCFStringEncodingUTF8);
    if action_ref.is_null() {
        return false;
    }
    let result = AXUIElementPerformAction(element, action_ref);
    CFRelease(action_ref as CFTypeRef);
    result == AX_ERROR_SUCCESS
}

unsafe fn copy_attribute(element: AXUIElementRef, attribute: &str) -> Option<CFTypeRef> {
    let attribute = CString::new(attribute).ok()?;
    let attribute_ref = CFStringCreateWithCString(
        kCFAllocatorDefault,
        attribute.as_ptr(),
        kCFStringEncodingUTF8,
    );
    if attribute_ref.is_null() {
        return None;
    }

    let mut value = ptr::null();
    let result = AXUIElementCopyAttributeValue(element, attribute_ref, &mut value);
    CFRelease(attribute_ref as CFTypeRef);
    (result == AX_ERROR_SUCCESS && !value.is_null()).then_some(value)
}

unsafe fn copy_string_attribute(element: AXUIElementRef, attribute: &str) -> Option<String> {
    let value = copy_attribute(element, attribute)?;
    let text = if CFGetTypeID(value) == CFStringGetTypeID() {
        cf_string_to_string(value as CFStringRef)
    } else {
        None
    };
    CFRelease(value);
    text
}

unsafe fn cf_string_to_string(value: CFStringRef) -> Option<String> {
    let length = CFStringGetLength(value);
    let capacity =
        CFStringGetMaximumSizeForEncoding(length, kCFStringEncodingUTF8).checked_add(1)?;
    let mut buffer = vec![0_i8; usize::try_from(capacity).ok()?];
    if CFStringGetCString(
        value,
        buffer.as_mut_ptr(),
        capacity as CFIndex,
        kCFStringEncodingUTF8,
    ) == 0
    {
        return None;
    }
    Some(
        CStr::from_ptr(buffer.as_ptr())
            .to_string_lossy()
            .into_owned(),
    )
}

fn parse_event_label(
    label: &str,
    range_start: DateTime<Local>,
    range_end: DateTime<Local>,
) -> Option<LocalOutlookMeeting> {
    let normalized = label.replace(['\n', '\r', '\t', '\u{a0}', '\u{202f}'], " ");
    let parsed_date = parse_date(&normalized)?;
    let times = EVENT_TIME
        .captures_iter(&normalized[parsed_date.start..])
        .filter_map(parse_time)
        .take(2)
        .collect::<Vec<_>>();

    let all_day = contains_case_insensitive(&normalized, &["all day", "all-day", "весь день"]);
    if times.len() < 2 && !all_day {
        return None;
    }

    let (start_hour, start_minute, end_hour, end_minute) = if all_day {
        (0, 0, 0, 0)
    } else {
        (times[0].0, times[0].1, times[1].0, times[1].1)
    };

    let start_at = local_datetime(
        parsed_date.year,
        parsed_date.month,
        parsed_date.day,
        start_hour,
        start_minute,
    )?;
    let mut end_at = if all_day {
        start_at + Duration::days(1)
    } else {
        local_datetime(
            parsed_date.year,
            parsed_date.month,
            parsed_date.day,
            end_hour,
            end_minute,
        )?
    };
    if end_at <= start_at && !all_day {
        end_at += Duration::days(1);
    }
    if end_at <= range_start || start_at >= range_end {
        return None;
    }

    let subject = clean_subject(&normalized[..parsed_date.start]);
    if subject.is_empty() {
        return None;
    }

    let location = LOCATION
        .captures(&normalized)
        .and_then(|captures| captures.name("location"))
        .map(|capture| capture.as_str().trim().to_string())
        .filter(|value| !value.is_empty());
    let is_recurring = contains_case_insensitive(
        &normalized,
        &["recurr", "повторя", "повторяющееся", "серия"],
    );

    let mut hasher = Sha256::new();
    hasher.update(subject.as_bytes());
    hasher.update(start_at.to_rfc3339().as_bytes());
    hasher.update(end_at.to_rfc3339().as_bytes());
    let id = format!("macos-ax-{:x}", hasher.finalize());

    Some(LocalOutlookMeeting {
        id,
        calendar_id: "outlook-macos-visible-calendar".to_string(),
        calendar_name: "Outlook".to_string(),
        store_name: "Microsoft Outlook for Mac".to_string(),
        subject,
        start_at: start_at.to_rfc3339(),
        end_at: end_at.to_rfc3339(),
        is_all_day: all_day,
        is_meeting: true,
        is_recurring,
        location,
        response_status: "none".to_string(),
    })
}

fn parse_date(value: &str) -> Option<ParsedDate> {
    if let Some(captures) = RUSSIAN_DATE.captures(value) {
        let whole = captures.get(0)?;
        return Some(ParsedDate {
            day: captures.name("day")?.as_str().parse().ok()?,
            month: russian_month(captures.name("month")?.as_str())?,
            year: captures.name("year")?.as_str().parse().ok()?,
            start: whole.start(),
        });
    }

    let captures = ENGLISH_DATE.captures(value)?;
    let whole = captures.get(0)?;
    Some(ParsedDate {
        day: captures.name("day")?.as_str().parse().ok()?,
        month: english_month(captures.name("month")?.as_str())?,
        year: captures.name("year")?.as_str().parse().ok()?,
        start: whole.start(),
    })
}

fn parse_time(captures: regex::Captures<'_>) -> Option<(u32, u32)> {
    let mut hour = captures.name("hour")?.as_str().parse::<u32>().ok()?;
    let minute = captures.name("minute")?.as_str().parse::<u32>().ok()?;
    let ampm = captures
        .name("ampm")
        .map(|capture| capture.as_str().to_ascii_lowercase().replace('.', ""));

    if let Some(ampm) = ampm {
        if hour == 12 {
            hour = 0;
        }
        if ampm == "pm" {
            hour += 12;
        }
    }
    (hour <= 23 && minute <= 59).then_some((hour, minute))
}

fn local_datetime(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
) -> Option<DateTime<Local>> {
    match Local.with_ymd_and_hms(year, month, day, hour, minute, 0) {
        LocalResult::Single(value) => Some(value),
        LocalResult::Ambiguous(earlier, _) => Some(earlier),
        LocalResult::None => None,
    }
}

fn clean_subject(prefix: &str) -> String {
    let mut subject = prefix.trim_matches(|character: char| {
        character.is_whitespace() || matches!(character, ',' | ';' | ':' | '—' | '-')
    });
    if let Some((candidate, suffix)) = subject.rsplit_once(',') {
        let suffix = suffix.trim().to_lowercase();
        if matches!(
            suffix.as_str(),
            "start"
                | "starts"
                | "begin"
                | "begins"
                | "начало"
                | "monday"
                | "tuesday"
                | "wednesday"
                | "thursday"
                | "friday"
                | "saturday"
                | "sunday"
                | "понедельник"
                | "вторник"
                | "среда"
                | "четверг"
                | "пятница"
                | "суббота"
                | "воскресенье"
        ) {
            subject = candidate.trim();
        }
    }
    for generic in [
        "calendar event",
        "appointment",
        "event",
        "событие календаря",
        "событие",
    ] {
        if subject.to_lowercase().starts_with(&format!("{generic},")) {
            subject = subject[generic.len() + 1..].trim();
        }
    }
    subject
        .trim_matches(|character: char| character.is_whitespace() || character == ',')
        .to_string()
}

fn contains_case_insensitive(value: &str, needles: &[&str]) -> bool {
    let lower = value.to_lowercase();
    needles.iter().any(|needle| lower.contains(needle))
}

fn russian_month(value: &str) -> Option<u32> {
    match value.to_lowercase().as_str() {
        "января" => Some(1),
        "февраля" => Some(2),
        "марта" => Some(3),
        "апреля" => Some(4),
        "мая" => Some(5),
        "июня" => Some(6),
        "июля" => Some(7),
        "августа" => Some(8),
        "сентября" => Some(9),
        "октября" => Some(10),
        "ноября" => Some(11),
        "декабря" => Some(12),
        _ => None,
    }
}

fn english_month(value: &str) -> Option<u32> {
    match value.to_ascii_lowercase().as_str() {
        "january" => Some(1),
        "february" => Some(2),
        "march" => Some(3),
        "april" => Some(4),
        "may" => Some(5),
        "june" => Some(6),
        "july" => Some(7),
        "august" => Some(8),
        "september" => Some(9),
        "october" => Some(10),
        "november" => Some(11),
        "december" => Some(12),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_russian_accessibility_event() {
        let range_start = Local
            .with_ymd_and_hms(2026, 7, 28, 0, 0, 0)
            .single()
            .unwrap();
        let range_end = range_start + Duration::days(7);
        let event = parse_event_label(
            "Командный синк, вторник 28 июля 2026 г. в 10:30, заканчивается в 11:15, местоположение: Переговорная 1, не повторяется",
            range_start,
            range_end,
        )
        .unwrap();

        assert_eq!(event.subject, "Командный синк");
        assert_eq!(event.location.as_deref(), Some("Переговорная 1"));
        assert!(event.start_at.contains("T10:30:00"));
        assert!(event.end_at.contains("T11:15:00"));
    }

    #[test]
    fn parses_english_accessibility_event_with_meridiem() {
        let range_start = Local
            .with_ymd_and_hms(2026, 7, 28, 0, 0, 0)
            .single()
            .unwrap();
        let range_end = range_start + Duration::days(7);
        let event = parse_event_label(
            "Project review, Tuesday, July 29, 2026 at 2:00 PM, ends at 3:30 PM, location: Room 4",
            range_start,
            range_end,
        )
        .unwrap();

        assert_eq!(event.subject, "Project review");
        assert!(event.start_at.contains("T14:00:00"));
        assert!(event.end_at.contains("T15:30:00"));
    }

    #[test]
    fn removes_accessibility_start_marker_from_subject() {
        let range_start = Local
            .with_ymd_and_hms(2026, 7, 28, 0, 0, 0)
            .single()
            .unwrap();
        let range_end = range_start + Duration::days(7);
        let event = parse_event_label(
            "Демо Дататеки, Новинки. Копия, начало, 29 июля 2026 г. в 11:00, заканчивается в 11:30",
            range_start,
            range_end,
        )
        .unwrap();

        assert_eq!(event.subject, "Демо Дататеки, Новинки. Копия");
    }

    #[test]
    fn ignores_non_calendar_accessibility_text() {
        let range_start = Local::now();
        let range_end = range_start + Duration::days(7);
        assert!(parse_event_label("Today, 10:30", range_start, range_end).is_none());
    }

    #[test]
    #[ignore = "requires a running, configured Outlook and Accessibility permission"]
    fn reads_visible_outlook_calendar_without_exposing_event_content() {
        let meetings = upcoming_meetings(7).expect("Outlook Accessibility query should succeed");
        eprintln!(
            "Outlook Accessibility returned {} meeting(s)",
            meetings.len()
        );
    }
}
