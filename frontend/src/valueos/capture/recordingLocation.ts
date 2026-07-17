// VALUEOS: colocate the upstream audio meeting folder with the user's configured transcript
// folder. Upstream writes the recording's audio.mp4 + transcripts.json + metadata.json +
// .checkpoints/ under its "save_folder" recording preference (default
// ~/Movies/meetily-recordings). Before each recording we point that preference at the
// configured transcript folder via the already-registered get/set_recording_preferences
// commands — so ALL artifacts for a call live together, with NO upstream edit. Best-effort:
// a no-op when nothing is configured (keep the upstream default), and callers must treat a
// failure here as non-fatal (the ValueOS .txt still writes to the configured folder either way).

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/** Mirrors the upstream Rust `RecordingPreferences` struct (snake_case, no serde rename). */
export interface RecordingPreferences {
  save_folder: string;
  auto_save: boolean;
  file_format: string;
  preferred_mic_device?: string | null;
  preferred_system_device?: string | null;
  system_audio_backend?: string | null;
}

/**
 * Point the upstream recording save_folder at `folder`, preserving all other preferences
 * (auto_save, file_format, preferred devices, backend). No-op when `folder` is empty.
 */
export async function applyConfiguredSaveFolder(invoke: InvokeFn, folder: string | null): Promise<void> {
  if (!folder || !folder.trim()) return; // nothing configured → keep the upstream default
  let prev: RecordingPreferences | null = null;
  try {
    prev = await invoke<RecordingPreferences>('get_recording_preferences');
  } catch {
    prev = null; // fall back to a minimal valid preferences object below
  }
  const base: RecordingPreferences = prev ?? { save_folder: folder, auto_save: true, file_format: 'mp4' };
  const preferences: RecordingPreferences = { ...base, save_folder: folder };
  await invoke('set_recording_preferences', { preferences });
}
