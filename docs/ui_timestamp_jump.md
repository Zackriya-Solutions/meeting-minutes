# Click-to-Jump Audio Timestamps (PR-44c)

Wave 14c adds clickable timestamps on transcript segments. When a
meeting has an audio file attached, clicking the `[MM:SS]` label on
any segment seeks the audio player to that position.

## What ships in this PR

- `TranscriptSegment` renders the per-segment timestamp as a `<button>`
  instead of plain text.
- `VirtualizedTranscriptView` accepts an optional
  `onTimestampClick?: (sec: number) => void` prop. When omitted the
  button is rendered disabled with neutral styling (no functional
  impact on legacy call sites).
- `TranscriptPanel` (the meeting-details variant) accepts an optional
  `audioPath?: string | null` prop. When supplied it renders a
  compact audio player (play/pause, seek bar, time display) using
  `useAudioPlayer(audioPath)` and wires the player `seek` into the
  virtualized view’s `onTimestampClick`.
- Four locales (en-US, en-GB, zh-CN, zh-TW) gain `transcript.view.player`
  translations for the new controls.

## What does NOT ship in this PR

- **Audio file discovery from `meeting.folder_path`** — PR-44d adds the
  `get_meeting_audio_path` Tauri command + the `useMeetingAudioPath`
  hook. The command joins `meeting.folder_path` with the conventional
  filename and returns the absolute path or `null`.
- **Parallel WAV export at recording time** — PR-44e closes the mp4-vs-WAV
  gap: `incremental_saver::finalize()` now writes `audio.wav` alongside
  `audio.mp4`, so every meeting recorded after PR-44e ships auto-lights
  the player. Pre-PR-44e meetings (no wav) resolve to `null` and stay
  hidden — legacy support is graceful.
- **Auto-discovery of non-conventional filenames** — if the audio file
  in the meeting folder is named anything other than `audio.wav`, the
  hook returns `null`. Add more probe filenames in the backend command
  if needed.
- **Waveform visualization**, **speaker-relative time**, and other
  richer player features — tracked for future waves.
## How to enable click-to-jump on a meeting

In the meeting-details page (or any future surface that mounts a
`TranscriptPanel`), resolve the audio path and pass it through:

```tsx
<TranscriptPanel
  /* ...existing props... */
  audioPath={meeting.audio_path ?? null}
/>
```

Until PR-44d lands the discovery helper, you can wire the value
manually for testing:

```tsx
const audioPath = meeting.folder_path
  ? `${meeting.folder_path}/recording.wav`
  : null;
```

## UI Behavior

- **No `audioPath`**: player UI is hidden entirely; timestamps render
  as a disabled gray button (no functional regression vs plain text).
- **`audioPath` but file unreadable**: `useAudioPlayer` reports
  `error`; player controls disable and a red icon appears next to the
  seek bar.
- **`audioPath` loaded**: player controls activate. Clicking a
  transcript timestamp seeks audio to that second. If audio was
  playing, it continues from the new position; if paused, it stays
  paused at the new position.

## i18n

All new strings live under `transcript.view.player` in each locale:

| key | en-US | zh-CN |
|---|---|---|
| `play` / `pause` | Play / Pause | 播放 / 暂停 |
| `play_title` / `pause_title` | Play audio / Pause audio | 播放音频 / 暂停音频 |
| `seek_title` | Seek to position | 跳转到指定时间 |
| `timestamp_jump_tooltip` | Click timestamp to jump… | 点击时间戳跳转到音频位置 |
| `no_audio` | No audio available… | 此会议无音频可播放 |
| `loading` | Loading audio... | 加载音频中... |
| `error` | Audio playback failed | 音频播放失败 |

## Accessibility

- The timestamp button is keyboard-focusable, announces an
  `aria-label` of `Jump to MM:SS`, and stops click propagation so
  segment selection is not triggered.
- Player controls each have `title` + `aria-label` in the active locale.
- When the player errors out, the alert icon announces the error
  string via `aria-label`.

## Files changed

| File | Purpose |
|---|---|
| `frontend/src/components/VirtualizedTranscriptView.tsx` | timestamp button + props + tooltip |
| `frontend/src/components/MeetingDetails/TranscriptPanel.tsx` | audioPath prop + compact player |
| `frontend/locales/{en-US,en-GB,zh-CN,zh-TW}/transcript.json` | `view.player` translations |
| `docs/ui_timestamp_jump.md` | this document |
| `docs/superpowers/specs/2026-07-14-ui-wave14c.md` | spec |

## Future work (next waves)

- **PR-44d**: `useMeetingAudioPath(meeting)` hook with discovery +
  graceful fallback when audio is absent.
- **PR-45**: Waveform visualization in the compact player.
- **PR-46**: Speaker-relative time display toggle.
