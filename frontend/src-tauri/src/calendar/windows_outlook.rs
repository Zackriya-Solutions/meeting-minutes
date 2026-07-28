//! Native Windows bridge to the locally installed classic Outlook.
//!
//! The bridge uses late-bound COM automation so Memento does not need to ship
//! or register an Outlook add-in and does not depend on a particular Office
//! interop assembly. Only standard appointment properties are read. In
//! particular, bodies, attachments, recipient addresses, and credentials are
//! never requested.

use super::local_outlook::{LocalOutlookCalendarStatus, LocalOutlookMeeting};
use chrono::{DateTime, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, TimeZone};
use std::collections::{HashMap, HashSet};
use std::ptr;
use windows::{
    core::{IUnknown, Interface, BSTR, GUID, PCWSTR},
    Win32::System::{
        Com::{
            CLSIDFromProgID, CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch,
            CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, DISPATCH_FLAGS, DISPATCH_METHOD,
            DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS,
        },
        Ole::{GetActiveObject, DISPID_PROPERTYPUT},
        Variant::{VARIANT, VT_DATE, VT_NULL},
    },
};

const OUTLOOK_PROG_ID: &str = "Outlook.Application";
const OL_FOLDER_CALENDAR: i32 = 9;
const OL_APPOINTMENT_ITEM: i32 = 1;
const OL_RESPONSE_DECLINED: i32 = 4;
const OL_MEETING_CANCELED: i32 = 5;
const OL_MEETING_RECEIVED_AND_CANCELED: i32 = 7;
const USER_DEFAULT_LCID: u32 = 0x0400;
const MAX_STORES: i32 = 50;
const MAX_FOLDER_DEPTH: usize = 5;
const MAX_CALENDAR_FOLDERS: usize = 100;
const MAX_EVENTS_PER_CALENDAR: usize = 500;
const MAX_RETURNED_EVENTS: usize = 200;

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|error| format!("cannot initialize local Outlook access: {error}"))?;
        }
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[derive(Clone)]
struct AutomationObject(IDispatch);

impl AutomationObject {
    fn dispatch_id(&self, name: &str) -> Result<i32, String> {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let property_name = PCWSTR(wide.as_ptr());
        let mut dispatch_id = 0_i32;
        unsafe {
            self.0
                .GetIDsOfNames(
                    &GUID::default(),
                    &property_name,
                    1,
                    USER_DEFAULT_LCID,
                    &mut dispatch_id,
                )
                .map_err(|error| format!("Outlook does not expose {name}: {error}"))?;
        }
        Ok(dispatch_id)
    }

    fn invoke(
        &self,
        name: &str,
        flags: DISPATCH_FLAGS,
        arguments: Vec<VARIANT>,
    ) -> Result<VARIANT, String> {
        let dispatch_id = self.dispatch_id(name)?;
        // IDispatch receives positional arguments in reverse order.
        let mut arguments: Vec<VARIANT> = arguments.into_iter().rev().collect();
        let mut property_put_id = DISPID_PROPERTYPUT;
        let is_property_put = flags == DISPATCH_PROPERTYPUT;
        let params = DISPPARAMS {
            rgvarg: if arguments.is_empty() {
                ptr::null_mut()
            } else {
                arguments.as_mut_ptr()
            },
            rgdispidNamedArgs: if is_property_put {
                &mut property_put_id
            } else {
                ptr::null_mut()
            },
            cArgs: arguments.len() as u32,
            cNamedArgs: u32::from(is_property_put),
        };
        let mut result = VARIANT::default();
        unsafe {
            self.0
                .Invoke(
                    dispatch_id,
                    &GUID::default(),
                    USER_DEFAULT_LCID,
                    flags,
                    &params,
                    Some(&mut result),
                    None,
                    None,
                )
                .map_err(|error| format!("Outlook operation {name} failed: {error}"))?;
        }
        Ok(result)
    }

    fn get(&self, name: &str) -> Result<VARIANT, String> {
        self.invoke(name, DISPATCH_PROPERTYGET, Vec::new())
    }

    fn call(&self, name: &str, arguments: Vec<VARIANT>) -> Result<VARIANT, String> {
        self.invoke(name, DISPATCH_METHOD, arguments)
    }

    fn set(&self, name: &str, value: VARIANT) -> Result<(), String> {
        self.invoke(name, DISPATCH_PROPERTYPUT, vec![value])
            .map(|_| ())
    }

    fn get_object(&self, name: &str) -> Result<Self, String> {
        object_from_variant(&self.get(name)?)
    }

    fn call_object(&self, name: &str, arguments: Vec<VARIANT>) -> Result<Self, String> {
        object_from_variant(&self.call(name, arguments)?)
    }

    fn call_optional_object(
        &self,
        name: &str,
        arguments: Vec<VARIANT>,
    ) -> Result<Option<Self>, String> {
        optional_object_from_variant(&self.call(name, arguments)?)
    }

    fn string(&self, name: &str) -> Result<String, String> {
        string_from_variant(&self.get(name)?).map(|value| trimmed_bounded(value, 1_000))
    }

    fn optional_string(&self, name: &str) -> Option<String> {
        self.string(name).ok().filter(|value| !value.is_empty())
    }

    fn integer(&self, name: &str) -> Result<i32, String> {
        i32::try_from(&self.get(name)?)
            .map_err(|error| format!("Outlook property {name} is not an integer: {error}"))
    }

    fn boolean(&self, name: &str) -> Result<bool, String> {
        bool::try_from(&self.get(name)?)
            .map_err(|error| format!("Outlook property {name} is not a boolean: {error}"))
    }

    fn datetime(&self, name: &str) -> Result<DateTime<Local>, String> {
        let value = self.get(name)?;
        let ole_date = if value.vt() == VT_DATE {
            unsafe { value.Anonymous.Anonymous.Anonymous.date }
        } else {
            f64::try_from(&value)
                .map_err(|error| format!("Outlook property {name} is not a date: {error}"))?
        };
        local_datetime_from_ole_date(ole_date)
    }
}

struct CalendarFolder {
    object: AutomationObject,
    id: String,
    name: String,
    store_id: String,
    store_name: String,
}

fn trimmed_bounded(value: String, max_chars: usize) -> String {
    value.trim().chars().take(max_chars).collect::<String>()
}

fn string_from_variant(value: &VARIANT) -> Result<String, String> {
    let wide = BSTR::try_from(value)
        .map_err(|error| format!("Outlook returned a non-text value: {error}"))?;
    String::try_from(wide).map_err(|error| format!("Outlook returned invalid text: {error}"))
}

fn object_from_variant(value: &VARIANT) -> Result<AutomationObject, String> {
    IDispatch::try_from(value)
        .map(AutomationObject)
        .map_err(|error| format!("Outlook returned an invalid object: {error}"))
}

fn optional_object_from_variant(value: &VARIANT) -> Result<Option<AutomationObject>, String> {
    if value.is_empty() || value.vt() == VT_NULL {
        return Ok(None);
    }
    match IDispatch::try_from(value) {
        Ok(dispatch) => Ok(Some(AutomationObject(dispatch))),
        Err(_) => Ok(None),
    }
}

fn outlook_clsid() -> Result<GUID, String> {
    let wide: Vec<u16> = OUTLOOK_PROG_ID
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe { CLSIDFromProgID(PCWSTR(wide.as_ptr())) }
        .map_err(|_| "classic Outlook for Windows is not installed or not registered".to_string())
}

fn active_outlook(clsid: &GUID) -> Option<AutomationObject> {
    let mut unknown: Option<IUnknown> = None;
    unsafe { GetActiveObject(clsid, None, &mut unknown) }.ok()?;
    unknown?.cast::<IDispatch>().ok().map(AutomationObject)
}

fn open_outlook(clsid: &GUID) -> Result<AutomationObject, String> {
    if let Some(running) = active_outlook(clsid) {
        return Ok(running);
    }
    let application: IDispatch =
        unsafe { CoCreateInstance(clsid, None::<&IUnknown>, CLSCTX_LOCAL_SERVER) }.map_err(
            |error| {
                format!(
            "cannot start classic Outlook. Open Outlook and its local profile, then retry: {error}"
        )
            },
        )?;
    Ok(AutomationObject(application))
}

fn local_datetime_from_ole_date(value: f64) -> Result<DateTime<Local>, String> {
    if !value.is_finite() {
        return Err("Outlook returned an invalid calendar date".to_string());
    }
    let base = NaiveDate::from_ymd_opt(1899, 12, 30)
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .ok_or_else(|| "cannot initialize Outlook calendar date conversion".to_string())?;
    let micros = (value * 86_400_000_000.0).round();
    if micros < i64::MIN as f64 || micros > i64::MAX as f64 {
        return Err("Outlook calendar date is out of range".to_string());
    }
    let local_naive: NaiveDateTime = base
        .checked_add_signed(Duration::microseconds(micros as i64))
        .ok_or_else(|| "Outlook calendar date is out of range".to_string())?;
    match Local.from_local_datetime(&local_naive) {
        LocalResult::Single(value) => Ok(value),
        LocalResult::Ambiguous(first, _) => Ok(first),
        LocalResult::None => {
            Err("Outlook returned a time skipped by the local time zone".to_string())
        }
    }
}

fn response_status_label(status: i32) -> &'static str {
    match status {
        1 => "organized",
        2 => "tentative",
        3 => "accepted",
        4 => "declined",
        5 => "not_responded",
        _ => "none",
    }
}

fn collect_calendar_children(
    parent: &AutomationObject,
    store_id: &str,
    store_name: &str,
    depth: usize,
    seen: &mut HashSet<String>,
    calendars: &mut Vec<CalendarFolder>,
) {
    if depth >= MAX_FOLDER_DEPTH || calendars.len() >= MAX_CALENDAR_FOLDERS {
        return;
    }
    let Ok(folders) = parent.get_object("Folders") else {
        return;
    };
    let Ok(count) = folders.integer("Count") else {
        return;
    };
    for index in 1..=count.min(MAX_CALENDAR_FOLDERS as i32) {
        let Ok(folder) = folders.call_object("Item", vec![index.into()]) else {
            continue;
        };
        let is_calendar = folder.integer("DefaultItemType") == Ok(OL_APPOINTMENT_ITEM)
            || folder
                .optional_string("FolderClass")
                .is_some_and(|value| value.eq_ignore_ascii_case("IPF.Appointment"));
        if !is_calendar {
            continue;
        }
        add_calendar_folder(folder.clone(), store_id, store_name, seen, calendars);
        collect_calendar_children(&folder, store_id, store_name, depth + 1, seen, calendars);
        if calendars.len() >= MAX_CALENDAR_FOLDERS {
            break;
        }
    }
}

fn add_calendar_folder(
    folder: AutomationObject,
    store_id: &str,
    store_name: &str,
    seen: &mut HashSet<String>,
    calendars: &mut Vec<CalendarFolder>,
) {
    let Some(id) = folder.optional_string("EntryID") else {
        return;
    };
    if !seen.insert(id.clone()) {
        return;
    }
    let name = folder
        .optional_string("Name")
        .unwrap_or_else(|| "Calendar".to_string());
    calendars.push(CalendarFolder {
        object: folder,
        id,
        name: trimmed_bounded(name, 200),
        store_id: store_id.to_string(),
        store_name: trimmed_bounded(store_name.to_string(), 200),
    });
}

fn calendar_folders(namespace: &AutomationObject) -> Result<Vec<CalendarFolder>, String> {
    let mut calendars = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(stores) = namespace.get_object("Stores") {
        if let Ok(count) = stores.integer("Count") {
            for index in 1..=count.min(MAX_STORES) {
                let Ok(store) = stores.call_object("Item", vec![index.into()]) else {
                    continue;
                };
                let store_id = store.optional_string("StoreID").unwrap_or_default();
                let store_name = store
                    .optional_string("DisplayName")
                    .unwrap_or_else(|| "Outlook".to_string());
                let Ok(calendar) =
                    store.call_object("GetDefaultFolder", vec![OL_FOLDER_CALENDAR.into()])
                else {
                    continue;
                };
                add_calendar_folder(
                    calendar.clone(),
                    &store_id,
                    &store_name,
                    &mut seen,
                    &mut calendars,
                );
                collect_calendar_children(
                    &calendar,
                    &store_id,
                    &store_name,
                    0,
                    &mut seen,
                    &mut calendars,
                );
            }
        }
    }

    if calendars.is_empty() {
        let calendar =
            namespace.call_object("GetDefaultFolder", vec![OL_FOLDER_CALENDAR.into()])?;
        add_calendar_folder(calendar, "", "Outlook", &mut seen, &mut calendars);
    }

    if calendars.is_empty() {
        return Err("the local Outlook profile has no readable calendar folders".to_string());
    }
    Ok(calendars)
}

fn query_calendar(
    folder: &CalendarFolder,
    range_start: DateTime<Local>,
    range_end: DateTime<Local>,
) -> Result<Vec<LocalOutlookMeeting>, String> {
    let items = folder.object.get_object("Items")?;
    items.call("Sort", vec!["[Start]".into(), false.into()])?;
    items.set("IncludeRecurrences", true.into())?;
    let filter = format!(
        "[Start] <= '{}' AND [End] >= '{}'",
        range_end.format("%m/%d/%Y %I:%M %p"),
        range_start.format("%m/%d/%Y %I:%M %p")
    );
    let restricted = items.call_object("Restrict", vec![filter.as_str().into()])?;
    let mut current = restricted.call_optional_object("GetFirst", Vec::new())?;
    let mut meetings = Vec::new();

    for _ in 0..MAX_EVENTS_PER_CALENDAR {
        let Some(item) = current.take() else {
            break;
        };
        let start = item.datetime("Start");
        let end = item.datetime("End");
        if let (Ok(start), Ok(end)) = (start, end) {
            if start > range_end {
                break;
            }
            let meeting_status = item.integer("MeetingStatus").unwrap_or(0);
            let response_status = item.integer("ResponseStatus").unwrap_or(0);
            let canceled = meeting_status == OL_MEETING_CANCELED
                || meeting_status == OL_MEETING_RECEIVED_AND_CANCELED;
            if end >= range_start && !canceled && response_status != OL_RESPONSE_DECLINED {
                let entry_id = item.optional_string("EntryID").unwrap_or_default();
                let global_id = item
                    .optional_string("GlobalAppointmentID")
                    .unwrap_or_else(|| entry_id.clone());
                let subject = item
                    .optional_string("Subject")
                    .unwrap_or_else(|| "Untitled meeting".to_string());
                meetings.push(LocalOutlookMeeting {
                    id: format!("{}:{}", global_id, start.timestamp()),
                    calendar_id: folder.id.clone(),
                    calendar_name: folder.name.clone(),
                    store_name: folder.store_name.clone(),
                    subject: trimmed_bounded(subject, 500),
                    start_at: start.to_rfc3339(),
                    end_at: end.to_rfc3339(),
                    is_all_day: item.boolean("AllDayEvent").unwrap_or(false),
                    is_meeting: meeting_status != 0,
                    is_recurring: item.boolean("IsRecurring").unwrap_or(false),
                    location: item
                        .optional_string("Location")
                        .map(|value| trimmed_bounded(value, 500)),
                    response_status: response_status_label(response_status).to_string(),
                });
            }
        }
        current = restricted.call_optional_object("GetNext", Vec::new())?;
    }
    Ok(meetings)
}

pub fn status() -> Result<LocalOutlookCalendarStatus, String> {
    let _apartment = ComApartment::initialize()?;
    let clsid = match outlook_clsid() {
        Ok(value) => value,
        Err(_) => {
            return Ok(LocalOutlookCalendarStatus {
                supported: true,
                installed: false,
                running: false,
                provider: "local-classic-outlook",
                detail: "Classic Outlook is not installed. New Outlook does not provide local calendar access."
                    .to_string(),
            });
        }
    };
    let running = active_outlook(&clsid).is_some();
    Ok(LocalOutlookCalendarStatus {
        supported: true,
        installed: true,
        running,
        provider: "local-classic-outlook",
        detail: if running {
            "Classic Outlook is running; Memento can read its local calendar.".to_string()
        } else {
            "Classic Outlook is installed and will open when Memento reads the calendar."
                .to_string()
        },
    })
}

pub fn upcoming_meetings(days: u32) -> Result<Vec<LocalOutlookMeeting>, String> {
    let _apartment = ComApartment::initialize()?;
    let clsid = outlook_clsid()?;
    let outlook = open_outlook(&clsid)?;
    let namespace = outlook.call_object("GetNamespace", vec!["MAPI".into()])?;
    let calendars = calendar_folders(&namespace)?;
    let now = Local::now();
    let range_start = now;
    let range_end = now + Duration::days(i64::from(days.clamp(1, 31)));
    let mut by_id: HashMap<String, LocalOutlookMeeting> = HashMap::new();
    let mut readable_calendars = 0_usize;
    let mut last_error = None;

    for calendar in calendars {
        match query_calendar(&calendar, range_start, range_end) {
            Ok(events) => {
                readable_calendars += 1;
                for event in events {
                    by_id.entry(event.id.clone()).or_insert(event);
                }
            }
            Err(error) => {
                log::warn!(
                    "[local-outlook] calendar {} in store {} was not readable: {}",
                    calendar.name,
                    calendar.store_id,
                    error
                );
                last_error = Some(error);
            }
        }
    }

    if readable_calendars == 0 {
        return Err(last_error
            .unwrap_or_else(|| "the local Outlook profile has no readable calendars".to_string()));
    }

    let mut meetings: Vec<LocalOutlookMeeting> = by_id.into_values().collect();
    meetings.sort_by(|left, right| {
        let left_start = DateTime::parse_from_rfc3339(&left.start_at);
        let right_start = DateTime::parse_from_rfc3339(&right.start_at);
        match (left_start, right_start) {
            (Ok(left), Ok(right)) => left.cmp(&right),
            _ => left.start_at.cmp(&right.start_at),
        }
    });
    meetings.truncate(MAX_RETURNED_EVENTS);
    Ok(meetings)
}
