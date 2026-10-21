# Recording quit protection

Normal tray Quit and OS Quit/Cmd+Q requests retain the process while recording is initializing, capturing or being saved. The main window is focused so the user can stop and save first. Idle quit still reaches the existing database/sidecar cleanup. Window-close-to-tray behavior is unchanged.

A CAS/RAII startup guard is acquired before the first lifecycle await in both native start paths. Reentrant starts are rejected and every early error or unwind releases it. ACTIVE is published before STARTING is cleared; STOPPING protects final save. Stop also acquires exclusive RAII ownership before taking the manager; a repeated Stop returns without releasing the first save’s protection. New starts reject STOPPING. This does not prevent forced OS termination, process crashes or power loss.

The standalone production lifecycle tests run without audio permissions:

```sh
rustc --edition=2021 --test frontend/src-tauri/src/audio/recording_activity.rs -o /tmp/meetily-recording-activity-tests
/tmp/meetily-recording-activity-tests
```

Independent design and implementation review cleared the native paths. The standalone lifecycle tests pass, including duplicate-Stop protection. Full native CI runs separately with identical native source files and fork-only verification configuration kept outside this contribution. Native GUI/audio smoke has not been performed. Pending manual checks: Quit from tray/Cmd+Q during model startup, live and paused capture, stop-tail; then confirm idle quit performs normal cleanup and saved audio is retained.

This fixes explicit quit entry points independently of the close-preference feature proposed in PR864; it does not resubmit that feature or change its preference UI.
