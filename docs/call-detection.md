# Automatic call recording on macOS

In **Settings → Recording → Automatic call recording**, enable **Start recording when a call is detected**. Meetily must be running, onboarding must be complete, and a transcription model and audio permissions must be ready. Detection is off until enabled, and preferences persist on this computer.

This is experimental call-triggered recording. Calendar integration, reminders and meeting reconciliation are outside this change.

## Detection

On macOS 14.2 or later, the watcher polls CoreAudio process activity every two seconds. A supported app must use its microphone for at least four seconds before a single recording attempt. Merely opening the app or playing audio does not start a recording.

Supported desktop bundle identities are Zoom, Microsoft Teams, Slack, FaceTime and Webex. Browser detection is a separate option, off by default. It checks a recognised HTTPS Meet, Zoom or Teams conference URL in the active tab of Chrome, Edge, Brave, Arc or Safari while that browser captures microphone input. macOS may request Automation permission for each browser. Firefox and other browsers are not supported.

The detector reads process identities and conference URLs locally. It does not retain URLs, inspect participants, capture screenshots or record audio itself. Capture, transcription, notifications and saving use Meetily's existing recording commands and the user's selected devices/model/save preferences.

CoreAudio API references: [process list](https://developer.apple.com/documentation/coreaudio/kaudiohardwarepropertyprocessobjectlist), [input activity](https://developer.apple.com/documentation/coreaudio/kaudioprocesspropertyisrunninginput), and the ownership contract for `kAudioProcessPropertyBundleID` in Apple's AudioHardware.h.

## Recording lifecycle

- A detected call gets one automatic start attempt. A failed start surfaces an error and can be started manually after fixing model or permission setup.
- An existing manual recording prevents automatic start and is never claimed by the watcher.
- Stopping an automatic recording manually suppresses another automatic start until there has been no call activity for one minute.
- With **Stop recording when the call ends** enabled, one minute without call activity stops only the exact session the watcher started. The identity check and removal of the recording manager are atomic under its mutex. A replacement manual session cannot inherit automatic stopping.
- During an automatic recording, output activity in supported apps can keep a muted call alive. Browser continuation checks all tabs so switching away from a call tab does not itself end recording.
- Failed probes and sleep/wake gaps do not count as evidence that a call ended. Existing recordings continue.
- Turning detection off leaves the current recording running and relinquishes automatic stopping. Settings do not change normal audio/transcript persistence.

## Limits

Microphone previews, voice clips or other microphone use inside a supported app can resemble a call. Browser audio is attributed to a process, not an individual tab; other audio in the same browser can prolong detection while a conference tab remains open. Unrecognised URLs and joining a browser call in a background tab can be missed. Muting or changing devices may remove all observable activity; the one-minute grace period reduces interruptions but is not a guarantee. Back-to-back calls within that minute can remain one recording. Windows/Linux controls show the feature as unavailable.

## Verification

Run focused policy and URL/app matching tests without building transcription dependencies:

```sh
rustc --edition=2021 --test frontend/src-tauri/src/meeting_detection/state.rs -o /tmp/meetily-call-state-tests
/tmp/meetily-call-state-tests
rustc --edition=2021 --test frontend/src-tauri/src/meeting_detection/signals.rs -o /tmp/meetily-call-signals-tests
/tmp/meetily-call-signals-tests
```

Read-only native smoke test:

```sh
rustc --edition=2021 scripts/call-detection-probe.rs -o /tmp/meetily-call-probe
/tmp/meetily-call-probe
# Browser checks may request Automation permission:
/tmp/meetily-call-probe --browsers
```

Full checks follow the repository's normal build setup: frontend `pnpm build`, and `cargo check -p meetily --locked`. On a Mac with Command Line Tools selected but Xcode installed, set `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` for the command rather than changing the machine-wide selection. The existing build requires a llama-helper sidecar, FFmpeg, CMake and the transcription build dependencies.

Before merging, test a real desktop and browser call:

1. Enable detection after completing setup. Restart Meetily and verify the settings persist.
2. Leave Zoom/Teams open but idle. Confirm there is no recording.
3. Start a call and confirm one recording starts after approximately 4–6 seconds. Verify the selected input, system audio and transcript.
4. Stop manually while still in the call. Confirm no restart. End the call, wait one minute, and join another; confirm recording can start again.
5. Start a recording manually, then join and end a call. Confirm it stays running.
6. End an automatically recorded call. Confirm stopping/saving after the grace period, then reopen the meeting and check its audio and transcript.
7. Repeat in a supported browser with browser detection enabled. Check denied Automation permission, a switched tab, mute/unmute, audio-device changes and sleep/wake.
8. Disable detection while recording. Confirm the recording continues until manually stopped.

A read-only idle probe, policy tests and mocked UI checks do not establish real-call recording quality. Record actual call checks separately.
