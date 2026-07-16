// VALUEOS: configuration — the local transcript folder that must be set (and writable)
// before capture is possible. Phase 2 uses a mock picker + in-memory persistence; Phase 3
// swaps to the Tauri dialog + fs (writable probe) and tauri-plugin-store persistence,
// behind this same interface.

/** Persisted key for the chosen transcript folder. */
export const TRANSCRIPT_FOLDER_KEY = 'valueos.transcriptFolder';

export interface ConfigService {
  getTranscriptFolder(): Promise<string | null>;
  setTranscriptFolder(path: string): Promise<void>;
  /** Open a native folder picker; returns null if cancelled. */
  pickFolder(): Promise<string | null>;
  /** True iff the folder exists and we can write to it. */
  validateWritable(path: string): Promise<boolean>;
  /** Write a transcript file under the configured folder; returns the full path. */
  writeTranscriptFile(fileName: string, content: string): Promise<string>;
}

export interface MockConfigOptions {
  initialFolder?: string | null;
  pickResult?: string | null; // what pickFolder returns
  writable?: boolean; // what validateWritable returns
}

/** ⚠️ MOCK config service — in-memory + a canned picker/validator. */
export function createMockConfigService(opts: MockConfigOptions = {}): ConfigService {
  let folder: string | null = opts.initialFolder ?? null;
  return {
    async getTranscriptFolder() {
      return folder;
    },
    async setTranscriptFolder(path: string) {
      folder = path;
    },
    async pickFolder() {
      return opts.pickResult ?? '/Users/you/ValueOS Transcripts';
    },
    async validateWritable() {
      return opts.writable ?? true;
    },
    async writeTranscriptFile(fileName: string, _content: string) {
      // MOCK: no real file write; returns the intended path. Phase 3 writes via Tauri fs.
      return `${folder ?? '/mock'}/${fileName}`;
    },
  };
}
