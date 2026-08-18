import { invoke } from '@tauri-apps/api/core';

export interface StartAudioImportRequest {
  sourcePath: string;
  title: string;
  language: string | null;
  model: string | null;
  provider: string | null;
  denoiseAudio: boolean;
}

type InvokeCommand = (
  command: string,
  args?: Record<string, unknown>
) => Promise<unknown>;

export function startAudioImportCommand(
  request: StartAudioImportRequest,
  invokeCommand: InvokeCommand = (command, args) => invoke(command, args)
): Promise<unknown> {
  return invokeCommand('start_import_audio_command', {
    sourcePath: request.sourcePath,
    title: request.title,
    language: request.language,
    model: request.model,
    provider: request.provider,
    denoiseAudio: request.denoiseAudio,
  });
}
