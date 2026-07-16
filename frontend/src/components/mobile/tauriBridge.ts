/**
 * Voice Me mobile UI — bridge to the Rust/Tauri core.
 *
 * Thin, defensive wrappers around the same commands/events the desktop UI
 * uses. Every call degrades gracefully (returns fallbacks instead of
 * throwing) so the mobile shell stays usable while individual subsystems
 * come up.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type { VmMeeting, VmModel, VmSegment } from './types';

// ── Meetings ─────────────────────────────────────────────────────────────

export async function fetchMeetings(): Promise<VmMeeting[]> {
  try {
    const rows = await invoke<Record<string, unknown>[]>('api_get_meetings');
    return (rows || []).map((r) => ({
      id: String(r.id),
      title: String(r.title ?? 'Untitled meeting'),
      created_at: typeof r.created_at === 'string' ? r.created_at : undefined,
    }));
  } catch (e) {
    console.warn('[vm] api_get_meetings failed', e);
    return [];
  }
}

export interface MeetingDetail {
  id: string;
  title: string;
  created_at?: string;
  segments: VmSegment[];
}

export async function fetchMeetingDetail(meetingId: string): Promise<MeetingDetail | null> {
  try {
    const d = await invoke<{
      id: string;
      title: string;
      created_at?: string;
      transcripts?: {
        id: string;
        text: string;
        audio_start_time?: number | null;
      }[];
    }>('api_get_meeting', { meetingId });
    return {
      id: d.id,
      title: d.title,
      created_at: d.created_at,
      segments: (d.transcripts || []).map((t, i) => ({
        id: t.id ?? String(i),
        text: t.text,
        timestamp: typeof t.audio_start_time === 'number' ? t.audio_start_time : i * 5,
      })),
    };
  } catch (e) {
    console.warn('[vm] api_get_meeting failed', e);
    return null;
  }
}

// ── Recording ────────────────────────────────────────────────────────────

export async function startRecording(): Promise<void> {
  // No devices passed: the Rust core picks the default microphone.
  await invoke('start_recording');
}

export async function stopRecording(): Promise<void> {
  const { appDataDir } = await import('@tauri-apps/api/path');
  const dataDir = await appDataDir();
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  await invoke('stop_recording', {
    args: { save_path: `${dataDir}/recording-${timestamp}.wav` },
  });
}

export async function pauseRecording(): Promise<void> {
  await invoke('pause_recording');
}

export async function resumeRecording(): Promise<void> {
  await invoke('resume_recording');
}

export interface LiveTranscriptUpdate {
  text: string;
  audio_start_time?: number;
  sequence_id?: number;
  is_partial?: boolean;
}

export function onTranscriptUpdate(
  cb: (u: LiveTranscriptUpdate) => void
): Promise<UnlistenFn> {
  return listen<LiveTranscriptUpdate>('transcript-update', (e) => cb(e.payload));
}

/** Best-effort audio level stream; resolves to a cleanup fn. */
export async function watchAudioLevels(
  cb: (levels: number[]) => void
): Promise<() => void> {
  let unlisten: UnlistenFn | null = null;
  try {
    unlisten = await listen<{ levels?: unknown[] }>('audio-levels', (e) => {
      const raw = e.payload?.levels;
      if (!Array.isArray(raw)) return;
      const nums = raw
        .map((l) => {
          if (typeof l === 'number') return l;
          if (l && typeof l === 'object') {
            const o = l as Record<string, unknown>;
            for (const k of ['level', 'rms', 'peak', 'value']) {
              if (typeof o[k] === 'number') return o[k] as number;
            }
          }
          return 0;
        })
        .map((v) => Math.max(0, Math.min(1, v)));
      if (nums.length) cb(nums);
    });
    await invoke('start_audio_level_monitoring', { deviceNames: [] }).catch(() => {});
  } catch (e) {
    console.warn('[vm] audio level monitoring unavailable', e);
  }
  return () => {
    unlisten?.();
    invoke('stop_audio_level_monitoring').catch(() => {});
  };
}

// ── Whisper models ───────────────────────────────────────────────────────

const RECOMMENDED_MODEL = 'base';

function normalizeModelStatus(status: unknown): VmModel['status'] {
  if (typeof status === 'string') {
    return status.toLowerCase() === 'available' ? 'downloaded' : 'available';
  }
  if (status && typeof status === 'object') {
    const key = Object.keys(status)[0]?.toLowerCase() ?? '';
    if (key === 'downloading') return 'downloading';
    // Missing/Error/Corrupted → treat as not usable locally yet
    return 'available';
  }
  return 'available';
}

export async function fetchModels(): Promise<VmModel[]> {
  try {
    const rows = await invoke<
      { name: string; size_mb: number; description?: string; status: unknown }[]
    >('whisper_get_available_models');
    return (rows || []).map((m) => ({
      name: m.name,
      size_mb: m.size_mb,
      description: m.description ?? '',
      recommended: m.name.toLowerCase().includes(RECOMMENDED_MODEL),
      status: normalizeModelStatus(m.status),
      progress: 0,
    }));
  } catch (e) {
    console.warn('[vm] whisper_get_available_models failed', e);
    return [];
  }
}

export async function downloadModel(modelName: string): Promise<void> {
  await invoke('whisper_download_model', { modelName });
}

/**
 * Mark `modelName` as the active local transcription model.
 *
 * The Rust recording pipeline reads the saved transcript config to decide
 * which engine to use (Whisper vs Parakeet) and defaults to Parakeet when no
 * config row exists yet (see audio/transcription/engine.rs). Parakeet is not
 * set up on Android, so every Whisper download here must be paired with this
 * call or `start_recording` fails with "No Parakeet models are available."
 */
export async function selectWhisperModel(modelName: string): Promise<void> {
  try {
    await invoke('api_save_transcript_config', {
      provider: 'localWhisper',
      model: modelName,
      apiKey: null,
    });
  } catch (e) {
    console.warn('[vm] api_save_transcript_config failed', e);
  }
}

export async function hasTranscriptConfig(): Promise<boolean> {
  try {
    const config = await invoke<{ provider?: string; model?: string } | null>(
      'api_get_transcript_config'
    );
    return !!(config?.provider && config?.model);
  } catch {
    return false;
  }
}

export async function deleteModel(modelName: string): Promise<void> {
  await invoke('whisper_delete_corrupted_model', { modelName });
}

export function onModelDownloadProgress(
  cb: (modelName: string, progress: number) => void
): Promise<UnlistenFn> {
  return listen<{ modelName: string; progress: number }>(
    'model-download-progress',
    (e) => cb(e.payload.modelName, e.payload.progress)
  );
}

export function onModelDownloadComplete(
  cb: (modelName: string) => void
): Promise<UnlistenFn> {
  return listen<{ modelName: string }>('model-download-complete', (e) =>
    cb(e.payload.modelName)
  );
}

export async function hasDownloadedModel(): Promise<boolean> {
  try {
    return await invoke<boolean>('whisper_has_available_models');
  } catch {
    const models = await fetchModels();
    return models.some((m) => m.status === 'downloaded');
  }
}

// ── Permissions ──────────────────────────────────────────────────────────

export async function requestMicPermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('trigger_microphone_permission');
  } catch (e) {
    console.warn('[vm] trigger_microphone_permission failed', e);
    return false;
  }
}

// ── Summaries ────────────────────────────────────────────────────────────

export interface SummaryResult {
  status: 'ready' | 'empty';
  markdown: string;
}

function pickMarkdown(obj: unknown): string {
  if (!obj || typeof obj !== 'object') return '';
  const o = obj as Record<string, unknown>;
  for (const k of ['markdown', 'summary', 'result', 'text', 'content', 'data']) {
    const v = o[k];
    if (typeof v === 'string' && v.trim()) return v;
    if (v && typeof v === 'object') {
      const nested = pickMarkdown(v);
      if (nested) return nested;
    }
  }
  return '';
}

export async function fetchSummary(meetingId: string): Promise<SummaryResult> {
  try {
    const res = await invoke<unknown>('api_get_summary', { meetingId });
    const markdown = pickMarkdown(res);
    return markdown ? { status: 'ready', markdown } : { status: 'empty', markdown: '' };
  } catch {
    return { status: 'empty', markdown: '' };
  }
}

export async function generateSummary(
  meetingId: string,
  transcriptText: string,
  templateId: string
): Promise<void> {
  const config = await invoke<{
    provider?: string;
    model?: string;
  } | null>('api_get_model_config').catch(() => null);
  if (!config?.provider || !config?.model) {
    throw new Error('no_provider');
  }
  await invoke('api_process_transcript', {
    text: transcriptText,
    model: config.provider,
    modelName: config.model,
    meetingId,
    templateId,
  });
}

// ── Settings ─────────────────────────────────────────────────────────────

export interface VmModelConfig {
  provider: string;
  model: string;
  whisperModel: string;
  apiKey?: string | null;
  ollamaEndpoint?: string | null;
}

export async function getModelConfig(): Promise<VmModelConfig | null> {
  try {
    return await invoke<VmModelConfig | null>('api_get_model_config');
  } catch {
    return null;
  }
}

export async function saveModelConfig(config: VmModelConfig): Promise<boolean> {
  try {
    await invoke('api_save_model_config', {
      provider: config.provider,
      model: config.model,
      whisperModel: config.whisperModel,
      apiKey: config.apiKey ?? null,
      ollamaEndpoint: config.ollamaEndpoint ?? null,
    });
    return true;
  } catch (e) {
    console.warn('[vm] api_save_model_config failed', e);
    return false;
  }
}

export async function setLanguagePreference(language: string): Promise<void> {
  await invoke('set_language_preference', { language }).catch(() => {});
}
