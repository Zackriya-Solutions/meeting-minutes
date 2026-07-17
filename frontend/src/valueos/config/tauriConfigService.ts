// VALUEOS: REAL config — the chosen folder path is remembered in the webview (localStorage,
// non-sensitive), while the native folder picker, writable probe, and file write are Rust
// commands (they need the OS dialog + fs).
import { ConfigService, TRANSCRIPT_FOLDER_KEY } from './configService';
import { callValueOs } from '../transport/invoke';

export function createTauriConfigService(): ConfigService {
  return {
    async getTranscriptFolder() {
      return localStorage.getItem(TRANSCRIPT_FOLDER_KEY);
    },
    async setTranscriptFolder(path: string) {
      localStorage.setItem(TRANSCRIPT_FOLDER_KEY, path);
    },
    pickFolder() {
      return callValueOs<string | null>('valueos_pick_folder');
    },
    validateWritable(path: string) {
      return callValueOs<boolean>('valueos_validate_writable', { path });
    },
    async writeTranscriptFile(fileName: string, content: string) {
      const folder = localStorage.getItem(TRANSCRIPT_FOLDER_KEY);
      if (!folder) throw new Error('Transcript folder not set');
      // camelCase keys — Tauri v2 maps them to the snake_case Rust params.
      return callValueOs<string>('valueos_write_transcript_file', {
        folder,
        fileName,
        content,
      });
    },
    async deleteTranscriptFile(path: string) {
      if (!path) return;
      await callValueOs<void>('valueos_delete_file', { path });
    },
  };
}
