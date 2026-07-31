# Local Outlook Calendar

Memento reads upcoming calendar entries from the locally installed Outlook
application. The integration is intended for offline and restricted enterprise
environments where Microsoft Graph and OAuth are unavailable or not approved.

## Data path

```text
Classic Outlook local profile / OST
            ↓ Outlook Object Model (COM/MAPI)
       Memento Rust process
            ↓ Tauri command response
       Upcoming meetings UI

Outlook for Mac local calendar store
            ↓ Apple Events / AppleScript Calendar Suite   ← default on macOS
       Memento Rust process
            ↓ Tauri command response
       Upcoming meetings UI

Outlook for Mac visible Calendar UI
            ↓ macOS AXUIElement (Accessibility)           ← fallback only
       Memento Rust process
            ↓ Tauri command response
       Upcoming meetings UI
```

There is no Microsoft Graph client, OAuth flow, calendar token, or external
calendar endpoint in this path.

The user must explicitly enable the integration in **Settings → Calendar**.
Until then, Memento checks only whether the required Outlook application is
installed; it does not launch Outlook or read the calendar. After enabling it,
the home screen and Calendar settings read the next seven days when opened,
refresh when Memento returns to the foreground, and refresh every five minutes
while visible. When the calendar is empty or a read fails, Memento retries every
minute. Manual refresh remains available, and concurrent reads are coalesced so
the two screens cannot trigger duplicate Outlook automation.

On macOS, the user approves one **Automation** consent alert ("Memento wants
access to control Microsoft Outlook"). Memento then starts Outlook in the
background and asks it for the calendar over Apple Events. Outlook is never
brought to the front and its window layout is never changed.

## Data minimization

The bridge reads only:

- meeting title;
- start and end time;
- calendar and local Outlook store name on Windows;
- location;
- all-day, recurring, meeting, and response-status flags.

It does not request appointment bodies, attachments, recipient names or
addresses, credentials, mail, or contacts. Calendar responses are kept in
memory and are not written to the Memento SQLite database. Event content must
not be added to analytics events or logs.

## Windows compatibility

The integration requires classic Outlook for Windows and its registered Outlook
Object Model. Microsoft documents that the new Outlook for Windows does not
support OOM or MAPI, so Memento reports it as unavailable instead of falling
back to a cloud API.

The bridge uses late-bound COM calls and therefore does not require an Outlook
add-in or a particular Office interop assembly. If Outlook is installed but not
running, the first calendar read starts it. Corporate Outlook Programmatic
Access or endpoint-security policy can still deny the operation; Memento
surfaces that failure and does not attempt a bypass.

## macOS compatibility

### Why Automation and not Accessibility

Both are TCC permissions, but they are not equally reachable for a standard
corporate account:

| Permission | Consent record | Owner | What the user does |
| --- | --- | --- | --- |
| Accessibility | `/Library/Application Support/com.apple.TCC/TCC.db` | `root:wheel` | unlock System Settings with an **administrator password** |
| Automation (Apple Events) | `~/Library/Application Support/com.apple.TCC/TCC.db` | the logged-in user | click **OK** in one alert |

Managed fleets normally do not hand out local administrator rights, which made
the Accessibility connector unusable there. Automation consent is per-user and
per-target-application, so the same locked-down account can approve Memento →
Microsoft Outlook on its own.

Requirements on the Memento side, both already in the bundle:

- `NSAppleEventsUsageDescription` in `Info.plist` — the reason string shown in
  the consent alert;
- `com.apple.security.automation.apple-events` in `entitlements.plist` — needed
  because the app ships with the hardened runtime.

An MDM Privacy Preferences (PPPC) profile can still pre-approve or pre-deny
Automation centrally. If it denies, Memento reports it and does not attempt a
bypass.

### Connector selection

`calendar/macos_provider.rs` picks the connector:

- classic Outlook → Apple Events (`macos_outlook_events.rs`), the default;
- New Outlook (`IsRunningNewOutlook = 1`) → Accessibility
  (`macos_outlook.rs`), because Microsoft has still not shipped AppleScript
  support for New Outlook;
- if an Apple Events read fails and an administrator had already approved
  Accessibility, the Accessibility connector is used as a fallback.

The Apple Events connector asks Outlook for each calendar's events in the
requested window through the documented Calendar Suite (`calendar event`:
`subject`, `start time`, `end time`, `location`, `all day flag`,
`is recurring`, `exchange id`, attendee count). It never reads `content` or
`plain text content`, which are the appointment body. Unlike the Accessibility
connector it is not limited to the rendered week and does not depend on
localized VoiceOver label wording.

Both connectors work offline against the locally synced Outlook store and never
contact Exchange, Graph, or OWA directly.

## Manual verification on Windows

1. Start classic Outlook and confirm its local profile can show the Calendar
   while the workstation is offline.
2. In Memento, open **Settings → Calendar** and enable the local calendar.
3. Confirm the preview includes single and recurring appointments from the next
   seven days.
4. Confirm canceled and declined meetings are absent.
5. Start a recording from an upcoming meeting and confirm the Outlook subject
   becomes the recording title.
6. Inspect the Memento database and logs to confirm appointment content is not
   persisted.
7. Repeat with Outlook closed; it should start on the first read.
8. Repeat with new Outlook only; Settings should report that local access is
   unavailable.

## Manual verification on macOS

Run this while signed in to a **standard, non-administrator** account; that is
the case the connector exists for.

1. Install Memento in `/Applications`. Outlook may be closed.
2. Open **Settings → Calendar** and choose **Allow Outlook access**.
3. Confirm macOS shows "Memento wants access to control Microsoft Outlook" and
   that it asks only for a click — no administrator password, no System
   Settings trip. Click **OK**.
4. Enable the local Outlook calendar and choose **Refresh**.
5. Confirm the preview lists meetings from the next seven days, including
   occurrences of recurring series, and that Outlook did **not** come to the
   front or change its view.
6. Start a recording from a preview item and confirm its title is used.
7. Confirm event titles do not appear in Memento logs or its SQLite database.
8. Deny the permission instead (**System Settings → Privacy & Security →
   Automation**, switch Microsoft Outlook off for Memento — again with no
   administrator prompt) and confirm Memento reports it and offers the
   permission button rather than failing silently.
9. Switch Outlook to New Outlook and confirm Settings reports that New Outlook
   has no local calendar automation.
