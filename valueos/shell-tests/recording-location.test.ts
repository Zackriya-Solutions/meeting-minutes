import { describe, it, expect, vi } from 'vitest';
import { applyConfiguredSaveFolder } from '@/valueos/capture/recordingLocation';

// VALUEOS WS2: colocate the upstream audio meeting folder with the configured transcript
// folder by setting the recording save_folder preference before capture. Pure logic — the
// Tauri invoke is injected, so no native backend is needed.

describe('applyConfiguredSaveFolder', () => {
  it('no-ops when no folder is configured (keeps the upstream default)', async () => {
    const invoke = vi.fn();
    await applyConfiguredSaveFolder(invoke as any, null);
    await applyConfiguredSaveFolder(invoke as any, '   ');
    expect(invoke).not.toHaveBeenCalled();
  });

  it('points save_folder at the configured folder, preserving the other preferences', async () => {
    const prev = {
      save_folder: '/old/default',
      auto_save: false,
      file_format: 'wav',
      preferred_mic_device: 'Mic A',
      system_audio_backend: 'coreaudio',
    };
    const invoke = vi.fn(async (cmd: string) =>
      cmd === 'get_recording_preferences' ? prev : undefined,
    );
    await applyConfiguredSaveFolder(invoke as any, '/Users/me/VA Transcripts');

    expect(invoke).toHaveBeenCalledWith('get_recording_preferences');
    const setCall = invoke.mock.calls.find((c) => c[0] === 'set_recording_preferences');
    expect(setCall).toBeTruthy();
    const prefs = (setCall![1] as any).preferences;
    expect(prefs.save_folder).toBe('/Users/me/VA Transcripts'); // overridden
    expect(prefs.auto_save).toBe(false); // preserved
    expect(prefs.file_format).toBe('wav'); // preserved
    expect(prefs.preferred_mic_device).toBe('Mic A'); // preserved
    expect(prefs.system_audio_backend).toBe('coreaudio'); // preserved
  });

  it('still writes a valid preferences object when reading the current prefs fails', async () => {
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === 'get_recording_preferences') throw new Error('no store yet');
      return undefined;
    });
    await applyConfiguredSaveFolder(invoke as any, '/VA');
    const setCall = invoke.mock.calls.find((c) => c[0] === 'set_recording_preferences');
    expect(setCall).toBeTruthy();
    expect((setCall![1] as any).preferences).toMatchObject({
      save_folder: '/VA',
      auto_save: true,
      file_format: 'mp4',
    });
  });
});
