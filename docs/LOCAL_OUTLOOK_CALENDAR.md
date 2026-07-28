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

Outlook for Mac visible Calendar UI
            ↓ macOS AXUIElement (Accessibility)
       Memento Rust process
            ↓ Tauri command response
       Upcoming meetings UI
```

There is no Microsoft Graph client, OAuth flow, calendar token, or external
calendar endpoint in this path.

The user must explicitly enable the integration in **Settings → Calendar**.
Until then, Memento only checks whether classic Outlook is registered; it does
not launch Outlook or read the profile. After enabling it, the home screen reads
the next seven days while visible and refreshes every five minutes.

On macOS, the user must additionally approve Memento under **System Settings →
Privacy & Security → Accessibility**. Memento then opens Outlook, navigates to
Calendar and Today where the installed Outlook exposes those controls, attempts
to select Week view, and parses only visible VoiceOver labels. It does not use
AppleScript as a calendar data source.

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

The macOS connector uses the public Accessibility API and therefore works
independently of Exchange, Graph, OWA, and Outlook's AppleScript dictionary. It
supports Russian and English event labels and is intentionally best-effort:
Microsoft can change the accessibility hierarchy or localized label wording in
an Outlook update.

Only events exposed in the visible Calendar view can be returned. Hidden
calendars, events outside the currently rendered week, and details omitted from
the VoiceOver label are not available. If corporate policy blocks Accessibility
control, Memento reports the missing permission and does not attempt a bypass.

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

1. Install Memento in `/Applications` and start Microsoft Outlook.
2. Open **Settings → Calendar** and choose **Allow Accessibility**.
3. Enable Memento in **System Settings → Privacy & Security → Accessibility**,
   then return to Memento.
4. Enable the local Outlook calendar and choose **Refresh**.
5. Confirm Outlook switches to Calendar/Today and that the preview lists the
   visible meetings from the next seven days.
6. Start a recording from a preview item and confirm its title is used.
7. Confirm event titles do not appear in Memento logs or its SQLite database.
