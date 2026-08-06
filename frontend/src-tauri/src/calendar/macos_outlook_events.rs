//! Read-only Outlook for Mac calendar access through Apple Events (Automation).
//!
//! This is the default macOS connector because it needs no administrator
//! password. macOS records Automation consent in the *user* TCC database
//! (`~/Library/Application Support/com.apple.TCC/TCC.db`), so a standard
//! account approves it from the consent alert alone. Accessibility consent, by
//! contrast, lives in the root-owned system TCC database and can only be
//! granted by unlocking System Settings with an administrator credential.
//!
//! The connector never talks to Exchange, never opens Outlook databases, and
//! never brings Outlook to the foreground. It asks the locally installed
//! classic Outlook for the fields listed in `docs/LOCAL_OUTLOOK_CALENDAR.md`
//! and nothing else; appointment bodies are never requested.

use std::{
    ffi::c_void,
    io::{Read, Write},
    process::{Command, Stdio},
    ptr, thread,
    time::{Duration as StdDuration, Instant},
};

use chrono::{DateTime, Duration, Local, LocalResult, NaiveDateTime, TimeZone};
use sha2::{Digest, Sha256};

use super::{
    local_outlook::{normalize_attendees, LocalOutlookMeeting, MAX_ATTENDEES},
    macos_outlook::{outlook_installed, outlook_pid},
};

const OUTLOOK_BUNDLE_ID: &str = "com.microsoft.Outlook";
const RECORD_SEPARATOR: char = '\u{1e}';
const UNIT_SEPARATOR: char = '\u{1f}';
/// Separates the invitee names inside a record's attendee field.
const GROUP_SEPARATOR: char = '\u{1d}';
const SCRIPT_TIMEOUT: StdDuration = StdDuration::from_secs(150);
/// Invitee names one calendar read may fetch in total.
///
/// Every name is two Apple Event round trips, so a full week of large invitations could
/// otherwise turn a background refresh into a minute of automation traffic. A calendar
/// busy enough to exhaust this budget still returns every meeting, with the later
/// entries simply carrying no invitee list.
const ATTENDEE_NAME_BUDGET: usize = 400;
/// Meetings that already started are still worth offering as a recording target.
const LOOKBEHIND_HOURS: i64 = 2;

type OSStatus = i32;
type DescType = u32;

const TYPE_APPLICATION_BUNDLE_ID: DescType = 0x6275_6e64; // 'bund'
const TYPE_WILDCARD: DescType = 0x2a2a_2a2a; // '****'

const NO_ERR: OSStatus = 0;
const PROC_NOT_FOUND: OSStatus = -600;
const ERR_AE_EVENT_NOT_PERMITTED: OSStatus = -1743;
const ERR_AE_EVENT_WOULD_REQUIRE_USER_CONSENT: OSStatus = -1744;

#[repr(C)]
struct AEDesc {
    descriptor_type: DescType,
    data_handle: *mut c_void,
}

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn AECreateDesc(
        type_code: DescType,
        data: *const c_void,
        data_size: isize,
        result: *mut AEDesc,
    ) -> OSStatus;
    fn AEDisposeDesc(desc: *mut AEDesc) -> OSStatus;
    fn AEDeterminePermissionToAutomateTarget(
        target: *const AEDesc,
        event_class: u32,
        event_id: u32,
        ask_user_if_needed: u8,
    ) -> OSStatus;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationPermission {
    Granted,
    Denied,
    /// The user has not answered the consent alert yet.
    NotDetermined,
    /// Outlook is not running, so macOS cannot resolve the target yet.
    TargetNotRunning,
    Unknown(OSStatus),
}

/// Query Automation consent for Outlook.
///
/// With `ask_user = false` this never shows UI, which keeps background
/// refreshes silent. With `ask_user = true` macOS shows the consent alert; that
/// alert is a plain OK / Don't Allow choice and never asks for an
/// administrator password.
pub fn automation_permission(ask_user: bool) -> AutomationPermission {
    let bundle = OUTLOOK_BUNDLE_ID.as_bytes();
    let mut target = AEDesc {
        descriptor_type: 0,
        data_handle: ptr::null_mut(),
    };

    let created = unsafe {
        AECreateDesc(
            TYPE_APPLICATION_BUNDLE_ID,
            bundle.as_ptr().cast(),
            bundle.len() as isize,
            &mut target,
        )
    };
    if created != NO_ERR {
        return AutomationPermission::Unknown(created);
    }

    let result = unsafe {
        AEDeterminePermissionToAutomateTarget(
            &target,
            TYPE_WILDCARD,
            TYPE_WILDCARD,
            u8::from(ask_user),
        )
    };
    unsafe {
        AEDisposeDesc(&mut target);
    }

    match result {
        NO_ERR => AutomationPermission::Granted,
        ERR_AE_EVENT_NOT_PERMITTED => AutomationPermission::Denied,
        ERR_AE_EVENT_WOULD_REQUIRE_USER_CONSENT => AutomationPermission::NotDetermined,
        PROC_NOT_FOUND => AutomationPermission::TargetNotRunning,
        other => AutomationPermission::Unknown(other),
    }
}

/// True when Outlook runs in "New Outlook" mode, which drops AppleScript.
pub fn is_new_outlook_mode() -> bool {
    let output = Command::new("/usr/bin/defaults")
        .args(["read", OUTLOOK_BUNDLE_ID, "IsRunningNewOutlook"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim() == "1"
        }
        // A missing key means the preference was never written; classic Outlook
        // is the safe assumption because the Apple Events read fails loudly.
        _ => false,
    }
}

/// Start Outlook without stealing focus and without unhiding its windows.
pub fn ensure_outlook_running_in_background() -> Result<i32, String> {
    if let Some(pid) = outlook_pid() {
        return Ok(pid);
    }

    let status = Command::new("/usr/bin/open")
        .args(["-g", "-j", "-a", "Microsoft Outlook"])
        .status()
        .map_err(|error| format!("cannot start Microsoft Outlook: {error}"))?;
    if !status.success() {
        return Err("macOS could not start Microsoft Outlook.".to_string());
    }

    for _ in 0..40 {
        if let Some(pid) = outlook_pid() {
            return Ok(pid);
        }
        thread::sleep(StdDuration::from_millis(250));
    }
    Err("Microsoft Outlook did not finish starting.".to_string())
}

pub fn request_permission() -> Result<AutomationPermission, String> {
    if !outlook_installed() {
        return Err("Microsoft Outlook for Mac is not installed.".to_string());
    }
    ensure_outlook_running_in_background()?;

    let permission = automation_permission(true);
    if matches!(permission, AutomationPermission::Denied) {
        // A previous "Don't Allow" is only reversible in Settings, but the
        // Automation list is per-user and needs no administrator unlock.
        let _ = Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation")
            .spawn();
    }
    Ok(permission)
}

pub fn upcoming_meetings(days: u32) -> Result<Vec<LocalOutlookMeeting>, String> {
    if !outlook_installed() {
        return Err("Microsoft Outlook for Mac is not installed.".to_string());
    }
    if is_new_outlook_mode() {
        return Err(
            "New Outlook for Mac does not support local calendar automation. Switch off New Outlook in Outlook, or use the Accessibility connector."
                .to_string(),
        );
    }

    ensure_outlook_running_in_background()?;

    match automation_permission(false) {
        AutomationPermission::Granted => {}
        AutomationPermission::NotDetermined => {
            return Err(
                "Memento needs permission to read the Outlook calendar. Choose Allow Outlook access and confirm the macOS alert."
                    .to_string(),
            );
        }
        AutomationPermission::Denied => {
            return Err(
                "Outlook automation is turned off for Memento. Enable Microsoft Outlook for Memento in System Settings → Privacy & Security → Automation. No administrator password is required."
                    .to_string(),
            );
        }
        AutomationPermission::TargetNotRunning => {
            return Err("Microsoft Outlook is not responding.".to_string());
        }
        AutomationPermission::Unknown(code) => {
            return Err(format!(
                "macOS refused the Outlook automation check ({code})."
            ));
        }
    }

    let script = CALENDAR_SCRIPT
        .replace("__DAYS__", &days.to_string())
        .replace("__MAX_ATTENDEES__", &MAX_ATTENDEES.to_string())
        .replace("__ATTENDEE_BUDGET__", &ATTENDEE_NAME_BUDGET.to_string());
    let output = run_osascript(&script, SCRIPT_TIMEOUT)?;

    let now = Local::now();
    let range_start = now - Duration::hours(LOOKBEHIND_HOURS);
    let range_end = now + Duration::days(i64::from(days));

    let mut meetings = parse_records(&output, range_start, range_end)?;
    meetings.sort_by(|left, right| {
        left.start_at
            .cmp(&right.start_at)
            .then_with(|| left.subject.cmp(&right.subject))
    });
    meetings.dedup_by(|left, right| left.id == right.id);
    Ok(meetings)
}

fn parse_records(
    output: &str,
    range_start: DateTime<Local>,
    range_end: DateTime<Local>,
) -> Result<Vec<LocalOutlookMeeting>, String> {
    let mut meetings = Vec::new();
    for record in output.split(RECORD_SEPARATOR) {
        let record = record.trim_matches(|character: char| character == '\n' || character == '\r');
        if record.is_empty() {
            continue;
        }
        if let Some(message) = record.strip_prefix("ERROR\u{1f}") {
            return Err(format!("Outlook rejected the calendar query: {message}"));
        }
        if let Some(meeting) = parse_record(record, range_start, range_end) {
            meetings.push(meeting);
        }
    }
    Ok(meetings)
}

fn parse_record(
    record: &str,
    range_start: DateTime<Local>,
    range_end: DateTime<Local>,
) -> Option<LocalOutlookMeeting> {
    let fields = record.split(UNIT_SEPARATOR).collect::<Vec<_>>();
    if fields.len() < 10 {
        return None;
    }

    let calendar_id = fields[0].trim();
    let calendar_name = fields[1].trim();
    let event_key = fields[2].trim();
    let subject = fields[3].trim();
    let start_at = parse_local_stamp(fields[4].trim())?;
    let end_at = parse_local_stamp(fields[5].trim())?;
    let is_all_day = fields[6].trim() == "true";
    let is_recurring = fields[7].trim() == "true";
    let location = fields[8].trim();
    let attendee_count = fields[9].trim().parse::<u32>().unwrap_or(0);
    // Older records carried no invitee field; a meeting without one is still usable.
    let attendees = normalize_attendees(
        fields
            .get(10)
            .copied()
            .unwrap_or_default()
            .split(GROUP_SEPARATOR),
    );

    if subject.is_empty() {
        return None;
    }
    let end_at = if end_at <= start_at {
        start_at + Duration::minutes(30)
    } else {
        end_at
    };
    if end_at <= range_start || start_at >= range_end {
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(calendar_id.as_bytes());
    hasher.update(event_key.as_bytes());
    hasher.update(start_at.to_rfc3339().as_bytes());
    let id = format!("macos-ae-{:x}", hasher.finalize());

    Some(LocalOutlookMeeting {
        id,
        calendar_id: if calendar_id.is_empty() {
            "outlook-macos-calendar".to_string()
        } else {
            format!("outlook-macos-{calendar_id}")
        },
        calendar_name: if calendar_name.is_empty() {
            "Outlook".to_string()
        } else {
            calendar_name.to_string()
        },
        store_name: "Microsoft Outlook for Mac".to_string(),
        subject: subject.to_string(),
        start_at: start_at.to_rfc3339(),
        end_at: end_at.to_rfc3339(),
        is_all_day,
        is_meeting: attendee_count > 0 || !attendees.is_empty(),
        is_recurring,
        location: (!location.is_empty()).then(|| location.to_string()),
        response_status: "none".to_string(),
        attendees,
    })
}

fn parse_local_stamp(value: &str) -> Option<DateTime<Local>> {
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").ok()?;
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(value) => Some(value),
        LocalResult::Ambiguous(earlier, _) => Some(earlier),
        LocalResult::None => None,
    }
}

fn run_osascript(script: &str, timeout: StdDuration) -> Result<String, String> {
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run osascript: {error}"))?;

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "osascript did not accept the script".to_string())?;
        stdin
            .write_all(script.as_bytes())
            .map_err(|error| format!("cannot send the calendar script: {error}"))?;
    }

    // Drain both pipes on their own threads so a large calendar cannot fill a
    // pipe buffer and deadlock the watchdog below.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let stdout_reader = thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(stream) = stdout.as_mut() {
            let _ = stream.read_to_string(&mut buffer);
        }
        buffer
    });
    let stderr_reader = thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(stream) = stderr.as_mut() {
            let _ = stream.read_to_string(&mut buffer);
        }
        buffer
    });

    let started = Instant::now();
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(error) => return Err(format!("osascript failed: {error}")),
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(StdDuration::from_millis(50));
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();

    match exit_status {
        None => Err("Outlook did not answer the calendar query in time.".to_string()),
        Some(status) if status.success() => Ok(stdout),
        Some(_) => {
            let message = stderr.trim();
            if message.contains("-1743") || message.contains("not allowed") {
                Err(
                    "Outlook automation is turned off for Memento. Enable Microsoft Outlook for Memento in System Settings → Privacy & Security → Automation. No administrator password is required."
                        .to_string(),
                )
            } else if message.is_empty() {
                Err("Outlook rejected the calendar query.".to_string())
            } else {
                Err(format!("Outlook rejected the calendar query: {message}"))
            }
        }
    }
}

/// Reads only the documented calendar fields, now including the invitee names an
/// invitation carries. `content` and `plain text content` (the appointment body) are
/// deliberately never requested.
const CALENDAR_SCRIPT: &str = r#"
on padNumber(value, width)
	set rendered to (value as integer) as text
	repeat while (length of rendered) < width
		set rendered to "0" & rendered
	end repeat
	return rendered
end padNumber

on isoStamp(theDate)
	return my padNumber(year of theDate, 4) & "-" & my padNumber((month of theDate) as integer, 2) & "-" & my padNumber(day of theDate, 2) & "T" & my padNumber(hours of theDate, 2) & ":" & my padNumber(minutes of theDate, 2) & ":" & my padNumber(seconds of theDate, 2)
end isoStamp

on textOrEmpty(value)
	if value is missing value then return ""
	return my flattenText(value)
end textOrEmpty

on flattenText(value)
	set rendered to value as text
	set saved to AppleScript's text item delimiters
	set AppleScript's text item delimiters to {return, linefeed, tab, (character id 29), (character id 30), (character id 31)}
	set parts to text items of rendered
	set AppleScript's text item delimiters to " "
	set rendered to parts as text
	set AppleScript's text item delimiters to saved
	return rendered
end flattenText

set unitSeparator to (character id 31)
set recordSeparator to (character id 30)
set groupSeparator to (character id 29)
set rightNow to (current date)
set rangeStart to rightNow - (2 * hours)
set rangeEnd to rightNow + (__DAYS__ * days)
set collected to {}
set failureText to ""
set attendeeBudget to __ATTENDEE_BUDGET__

tell application "Microsoft Outlook"
	launch
	with timeout of 120 seconds
		set calendarList to every calendar
		repeat with currentCalendar in calendarList
			set calendarName to ""
			try
				set calendarName to my textOrEmpty(name of currentCalendar)
			end try
			set calendarIdentifier to ""
			try
				set calendarIdentifier to (id of currentCalendar) as text
			end try

			set eventList to {}
			try
				set eventList to (every calendar event of currentCalendar whose start time >= rangeStart and start time <= rangeEnd)
			on error errorText
				if failureText is "" then set failureText to errorText
				set eventList to {}
			end try

			repeat with currentEvent in eventList
				try
					set startsAt to start time of currentEvent
					set endsAt to startsAt
					try
						set endsAt to end time of currentEvent
					end try

					set eventSubject to ""
					try
						set eventSubject to my textOrEmpty(subject of currentEvent)
					end try

					set eventLocation to ""
					try
						set eventLocation to my textOrEmpty(location of currentEvent)
					end try

					set eventAllDay to false
					try
						set eventAllDay to (all day flag of currentEvent) as boolean
					end try

					set eventRecurring to false
					try
						set eventRecurring to (is recurring of currentEvent) as boolean
					end try

					set eventKey to ""
					try
						set eventKey to my textOrEmpty(exchange id of currentEvent)
					end try
					if eventKey is "" then
						try
							set eventKey to (id of currentEvent) as text
						end try
					end if

					-- Invitee names, so a recording started from this entry knows who was
					-- invited. Only the name and address of each invitee are read; the
					-- appointment body is still never requested. The per-event cap keeps a
					-- distribution list from spending the whole Apple Events budget.
					set attendeeTotal to 0
					set attendeeNames to {}
					try
						set eventAttendees to attendees of currentEvent
						set attendeeTotal to (count of eventAttendees)
						set attendeeLimit to attendeeTotal
						if attendeeLimit > __MAX_ATTENDEES__ then set attendeeLimit to __MAX_ATTENDEES__
						if attendeeLimit > attendeeBudget then set attendeeLimit to attendeeBudget
						set attendeeBudget to attendeeBudget - attendeeLimit
						repeat with attendeeIndex from 1 to attendeeLimit
							set attendeeName to ""
							try
								set attendeeAddress to email address of item attendeeIndex of eventAttendees
								try
									set attendeeName to my textOrEmpty(name of attendeeAddress)
								end try
								if attendeeName is "" then
									try
										set attendeeName to my textOrEmpty(address of attendeeAddress)
									end try
								end if
							end try
							if attendeeName is not "" then set end of attendeeNames to attendeeName
						end repeat
					end try

					-- `organizer` is plain display text on a calendar event, unlike an
					-- attendee's `email address` record.
					set organizerName to ""
					try
						set organizerName to my textOrEmpty(organizer of currentEvent)
					end try
					if organizerName is not "" then set beginning of attendeeNames to organizerName

					set attendeeText to ""
					if (count of attendeeNames) > 0 then
						set savedDelimiters to AppleScript's text item delimiters
						set AppleScript's text item delimiters to groupSeparator
						set attendeeText to attendeeNames as text
						set AppleScript's text item delimiters to savedDelimiters
					end if

					set end of collected to (calendarIdentifier & unitSeparator & calendarName & unitSeparator & eventKey & unitSeparator & eventSubject & unitSeparator & my isoStamp(startsAt) & unitSeparator & my isoStamp(endsAt) & unitSeparator & (eventAllDay as text) & unitSeparator & (eventRecurring as text) & unitSeparator & eventLocation & unitSeparator & (attendeeTotal as text) & unitSeparator & attendeeText)
				end try
			end repeat
		end repeat
	end timeout
end tell

if (count of collected) is 0 and failureText is not "" then
	return "ERROR" & unitSeparator & failureText
end if

set AppleScript's text item delimiters to recordSeparator
set rendered to collected as text
set AppleScript's text item delimiters to ""
return rendered
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fields: &[&str]) -> String {
        fields.join(&UNIT_SEPARATOR.to_string())
    }

    #[test]
    fn parses_a_calendar_record() {
        let range_start = Local
            .with_ymd_and_hms(2026, 7, 28, 0, 0, 0)
            .single()
            .unwrap();
        let range_end = range_start + Duration::days(7);
        let meetings = parse_records(
            &record(&[
                "42",
                "Календарь",
                "AAMkAD==",
                "Командный синк",
                "2026-07-29T10:30:00",
                "2026-07-29T11:15:00",
                "false",
                "true",
                "Переговорная 1",
                "4",
            ]),
            range_start,
            range_end,
        )
        .unwrap();

        assert_eq!(meetings.len(), 1);
        let meeting = &meetings[0];
        assert_eq!(meeting.subject, "Командный синк");
        // A record from an older read carries no invitee field at all.
        assert!(meeting.attendees.is_empty());
        assert_eq!(meeting.calendar_name, "Календарь");
        assert_eq!(meeting.location.as_deref(), Some("Переговорная 1"));
        assert!(meeting.is_recurring);
        assert!(meeting.is_meeting);
        assert!(!meeting.is_all_day);
        assert!(meeting.start_at.contains("T10:30:00"));
        assert!(meeting.end_at.contains("T11:15:00"));
    }

    #[test]
    fn reads_the_invitee_names_organizer_first() {
        let range_start = Local
            .with_ymd_and_hms(2026, 7, 28, 0, 0, 0)
            .single()
            .unwrap();
        let range_end = range_start + Duration::days(7);
        let invitees = [
            "Андрей Евлампиев",
            "Мария Петрова",
            // A duplicate spelling, and an invitee Outlook could only name by address.
            "андрей евлампиев",
            "guest@example.com",
        ]
        .join(&GROUP_SEPARATOR.to_string());
        let meetings = parse_records(
            &record(&[
                "42",
                "Календарь",
                "AAMkAD==",
                "Планирование спринта",
                "2026-07-29T10:30:00",
                "2026-07-29T11:15:00",
                "false",
                "false",
                "",
                "3",
                &invitees,
            ]),
            range_start,
            range_end,
        )
        .unwrap();

        assert_eq!(
            meetings[0].attendees,
            vec![
                "Андрей Евлампиев".to_string(),
                "Мария Петрова".to_string(),
                "guest@example.com".to_string(),
            ]
        );
    }

    #[test]
    fn counts_an_event_with_invitees_as_a_meeting() {
        let range_start = Local
            .with_ymd_and_hms(2026, 7, 28, 0, 0, 0)
            .single()
            .unwrap();
        let range_end = range_start + Duration::days(7);
        // Outlook can refuse the attendee count and still list the invitees.
        let meetings = parse_records(
            &record(&[
                "42",
                "Календарь",
                "AAMkAD==",
                "Синк",
                "2026-07-29T10:30:00",
                "2026-07-29T11:00:00",
                "false",
                "false",
                "",
                "0",
                "Мария Петрова",
            ]),
            range_start,
            range_end,
        )
        .unwrap();
        assert!(meetings[0].is_meeting);
    }

    #[test]
    fn skips_events_outside_the_window() {
        let range_start = Local
            .with_ymd_and_hms(2026, 7, 28, 0, 0, 0)
            .single()
            .unwrap();
        let range_end = range_start + Duration::days(7);
        let meetings = parse_records(
            &record(&[
                "1",
                "Calendar",
                "key",
                "Old meeting",
                "2026-07-01T10:00:00",
                "2026-07-01T11:00:00",
                "false",
                "false",
                "",
                "0",
            ]),
            range_start,
            range_end,
        )
        .unwrap();
        assert!(meetings.is_empty());
    }

    #[test]
    fn reports_a_script_side_failure() {
        let range_start = Local::now();
        let range_end = range_start + Duration::days(7);
        let error = parse_records(
            &format!("ERROR{UNIT_SEPARATOR}Outlook got an error: Access denied."),
            range_start,
            range_end,
        )
        .unwrap_err();
        assert!(error.contains("Access denied"));
    }

    #[test]
    fn separates_multiple_records() {
        let range_start = Local
            .with_ymd_and_hms(2026, 7, 28, 0, 0, 0)
            .single()
            .unwrap();
        let range_end = range_start + Duration::days(7);
        let payload = format!(
            "{}{RECORD_SEPARATOR}{}",
            record(&[
                "1",
                "Calendar",
                "a",
                "First",
                "2026-07-29T09:00:00",
                "2026-07-29T09:30:00",
                "false",
                "false",
                "",
                "2",
            ]),
            record(&[
                "1",
                "Calendar",
                "b",
                "Second",
                "2026-07-29T11:00:00",
                "2026-07-29T12:00:00",
                "false",
                "false",
                "Room 4",
                "0",
            ]),
        );
        let meetings = parse_records(&payload, range_start, range_end).unwrap();
        assert_eq!(meetings.len(), 2);
        assert_eq!(meetings[0].subject, "First");
        // An event without a location must arrive as an empty field, never as
        // AppleScript's "missing value" text, and must not reach the UI.
        assert_eq!(meetings[0].location, None);
        assert_eq!(meetings[1].subject, "Second");
        assert_eq!(meetings[1].location.as_deref(), Some("Room 4"));
        assert!(!meetings[1].is_meeting);
        // Distinct events must not collide on the generated id.
        assert_ne!(meetings[0].id, meetings[1].id);
    }

    #[test]
    fn gives_an_all_day_event_a_usable_end() {
        let range_start = Local
            .with_ymd_and_hms(2026, 7, 28, 0, 0, 0)
            .single()
            .unwrap();
        let range_end = range_start + Duration::days(7);
        let meetings = parse_records(
            &record(&[
                "1",
                "Calendar",
                "c",
                "Offsite",
                "2026-07-30T00:00:00",
                "2026-07-30T00:00:00",
                "true",
                "false",
                "",
                "0",
            ]),
            range_start,
            range_end,
        )
        .unwrap();
        assert_eq!(meetings.len(), 1);
        assert!(meetings[0].is_all_day);
        assert!(meetings[0].end_at.contains("T00:30:00"));
    }

    #[test]
    #[ignore = "requires a running classic Outlook and Automation consent"]
    fn reads_the_local_outlook_calendar() {
        let meetings = upcoming_meetings(7).expect("Apple Events calendar read should succeed");
        eprintln!("Apple Events returned {} meeting(s)", meetings.len());
    }
}
