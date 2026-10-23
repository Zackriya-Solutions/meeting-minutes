# Brief: native PipeWire/PulseAudio system-audio capture on Linux

## Status of this brief

Drafted by Claude (Opus) during a debugging session with Loïc, to hand off to another
agent for implementation. Not yet started. See "Reference material" at the bottom for
everything already investigated this session.

## Problem

On Linux, Meetily's "System Audio" capture only works today because cpal (the audio
library Meetily uses) has **no native PulseAudio/PipeWire backend** — only ALSA and
JACK. To capture "what's playing" (e.g. a video call), a PipeWire/PulseAudio monitor
source has to be manually exposed as a named ALSA pseudo-device (`~/.asoundrc`,
`type pulse`, with a `hint { show on }` block, and the PCM name itself must contain
the substring `"monitor"` since that's what `configure_linux_audio()` filters on).

Without that manual setup — i.e. on any stock Linux install — the Settings → System
Audio dropdown is simply empty, or (until this session's fixes landed) silently fails
to record even when an entry is picked. Two bugs in that ALSA-hack path were already
found and fixed this session (branch `fix/linux-system-audio-device-name-mismatch`,
PR #702) — see "Reference material" below for full detail. Those fixes make the hack
*work correctly*, but the underlying fragility remains: Linux users still need to
hand-register (or run an external script/service, which is what Loïc is doing on his
own machine, see below) an ALSA pseudo-device for every audio output they want to
capture. That's a personal workaround, not something upstream can ship as "Linux
support."

macOS already avoids this whole class of problem: `capture/core_audio.rs` talks to
Core Audio directly via the `cidre` crate, bypassing cpal entirely for system audio.
This task is the Linux equivalent of that.

## Goal

Capture Linux system audio via PipeWire/PulseAudio's own client API directly, instead
of through cpal's ALSA host, and instead of requiring any `~/.asoundrc` setup. Should
work out of the box on any modern Linux desktop (PipeWire, which ships a
PulseAudio-compatible server by default — the near-universal case today — as well as
classic standalone PulseAudio), listing real sink names/descriptions with no manual
configuration.

## Suggested technical approach

- **Crate choice**: `libpulse-binding` + `libpulse-simple-binding` are the pragmatic
  choice — they speak the PulseAudio protocol, which PipeWire also implements
  (pipewire-pulse), so the same code works on both PipeWire-via-pulse (the common
  case) and classic PulseAudio, without needing PipeWire's own (heavier, more
  PipeWire-specific) native API. Worth a quick spike to confirm before committing,
  but this is the expected right call.
- **New module**: `frontend/src-tauri/src/audio/capture/pulse_linux.rs` (or similar
  name), `#[cfg(target_os = "linux")]`, structured as the Linux sibling of
  `capture/core_audio.rs`.
  - Enumerate sinks and their monitor sources with real names/descriptions (e.g. via
    `pa_context_get_sink_info_list` or the `libpulse-binding` equivalent). This
    replaces `configure_linux_audio()`'s ALSA-hint-based enumeration for the "System
    Audio" section specifically — drop the `name.contains("monitor")` heuristic
    entirely, since we'd be talking to Pulse directly and get real sink metadata.
  - Open a record stream against the monitor source's real Pulse name (e.g.
    `pa_stream_new` / the simple-binding record API), feeding samples into the
    existing pipeline the same way `SystemAudioCapture` (macOS) does — see
    `capture/system.rs` and `stream.rs::create_core_audio_stream` for the established
    integration pattern (spawn a task, chunk samples, call
    `AudioCapture::process_audio_data`).
- **Wire-up**: in `frontend/src-tauri/src/audio/stream.rs::AudioStream::create_with_backend()`,
  add a Linux-native-Pulse branch analogous to the existing `use_core_audio` branch
  for macOS, selected instead of the CPAL/ALSA path when `device_type ==
  DeviceType::System` on Linux.
- `frontend/src-tauri/src/audio/devices/configuration.rs::get_device_and_config()`'s
  Linux/Output branch would then resolve devices via the same Pulse API rather than
  `cpal::host_from_id(cpal::HostId::Alsa)`.
- The **Microphone** capture path is unaffected — it already works fine via cpal/ALSA
  on Linux. Don't touch it.

## Non-goals / out of scope

- Don't change Microphone capture.
- Don't remove the two bug fixes already on `fix/linux-system-audio-device-name-mismatch`
  (the `" (System Audio)"` suffix-stripping fix in `configuration.rs`, and the
  `snd_config_update_free_global()` ALSA-cache-reload fix in `discovery.rs`) as part
  of this task. Once this native-Pulse path lands, that ALSA/cpal code for Linux
  System Audio may become entirely dead — but treat that as a separate cleanup
  decision to make with Loïc afterwards, not something to fold in here.
- No Windows/macOS changes.

## Acceptance criteria

- On a Linux install with PipeWire and **zero** `~/.asoundrc` customization, Settings
  → System Audio lists real sink names (e.g. "Built-in Speakers", "JBL Tune 770NC")
  with no manual setup.
- Selecting one and recording actually captures system audio — verify by playing
  audio during a recording and checking the output file has a non-silent system
  track, e.g.:
  ```
  ffmpeg -i recording.mp4 -af volumedetect -f null - 2>&1 | grep volume
  ```
- Switching outputs (plugging a new DAC, connecting a different Bluetooth device)
  makes the new device appear without requiring a full app restart.
- `cargo check` / `cargo build --release` succeed. This machine needs
  `LIBCLANG_PATH=/usr/lib/llvm18/lib` for an unrelated `whisper-rs-sys` bindgen issue
  — see `CLAUDE.md` → "Local build environment gotchas" at repo root for the full
  story (system clang got upgraded past what whisper-rs-sys's bindgen tolerates).
- AppImage bundling needs `NO_STRIP=1` on this machine too (unrelated linuxdeploy/RELR
  issue, same section of `CLAUDE.md`).

## Branch / PR

- New branch off `main`: suggested name `feat/linux-native-pulse-system-audio`. This
  is an architectural change, not a fix-on-top-of-a-fix — don't stack it on
  `fix/linux-system-audio-device-name-mismatch`.
- If PR #702 has merged upstream by the time this starts, rebase onto latest `main`
  first.
- Open as its own PR referencing issues/PR #701 / #702 for context once it's
  implemented and verified end-to-end on Loïc's machine (real recording, real
  playback check as above — not just `cargo check`).

## Reference material already produced this session

- Issue (full root-cause detail on the ALSA-hack fragility):
  https://github.com/Zackriya-Solutions/meetily/issues/701
- PR (the two already-fixed bugs): https://github.com/Zackriya-Solutions/meetily/pull/702
- `CLAUDE.md` → "Session notes" section at repo root: full history of what was
  investigated/fixed/tried this session, including the local build environment
  gotchas (`LIBCLANG_PATH`, `NO_STRIP`) that will bite again on this task.
- (Outside this repo, for context only — not to be touched by this task): Loïc has a
  personal systemd `--user` service in his dotfiles
  (`~/dotfiles/stow/audio-monitors/`) that auto-generates `~/.asoundrc` monitor
  entries as a stopgap. Once this task lands, that service becomes unnecessary on
  his machine — worth telling him when this is done, but it's his to remove, not part
  of this task's scope.
