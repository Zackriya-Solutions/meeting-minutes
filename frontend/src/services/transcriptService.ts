/**
 * Transcript Service
 *
 * Handles all transcription-related Tauri backend calls and events.
 * Pure 1-to-1 wrapper - no error handling changes, exact same behavior as direct invoke/listen calls.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { TranscriptUpdate, Transcript } from '@/types';

export interface TranscriptionErrorPayload {
  error: string;
  userMessage: string;
  actionable: boolean;
  phase: 'startup' | 'active';
}

export interface ModelDownloadCompletePayload {
  modelName: string;
}

/**
 * Transcript Service
 * Singleton service for managing transcription operations and transcript history
 */
export class TranscriptService {
  /**
   * Get transcript history from backend (for reload sync)
   * @returns Promise<Transcript[]>
   */
  async getTranscriptHistory(): Promise<Transcript[]> {
    return invoke<Transcript[]>('get_transcript_history');
  }

  // Event Listeners

  /**
   * Listen for real-time transcript updates
   * @param callback - Function to call when new transcript segment arrives
   * @returns Promise that resolves to unlisten function
   */
  async onTranscriptUpdate(callback: (update: TranscriptUpdate) => void): Promise<UnlistenFn> {
    return listen<TranscriptUpdate>('transcript-update', (event) => {
      callback(event.payload);
    });
  }

  /**
   * Listen for transcription-error event (structured errors)
   * @param callback - Function to call when transcription error occurs
   * @returns Promise that resolves to unlisten function
   */
  async onTranscriptionError(callback: (error: TranscriptionErrorPayload) => void): Promise<UnlistenFn> {
    return listen<TranscriptionErrorPayload>('transcription-error', (event) => {
      callback(event.payload);
    });
  }

  /**
   * Listen for Whisper model download complete event
   * @param callback - Function to call when Whisper model download completes
   * @returns Promise that resolves to unlisten function
   */
  async onModelDownloadComplete(callback: (modelName: string) => void): Promise<UnlistenFn> {
    return listen<ModelDownloadCompletePayload>('model-download-complete', (event) => {
      callback(event.payload.modelName);
    });
  }

  /**
   * Listen for Parakeet model download complete event
   * @param callback - Function to call when Parakeet model download completes
   * @returns Promise that resolves to unlisten function
   */
  async onParakeetModelDownloadComplete(callback: (modelName: string) => void): Promise<UnlistenFn> {
    return listen<ModelDownloadCompletePayload>('parakeet-model-download-complete', (event) => {
      callback(event.payload.modelName);
    });
  }
}

// Export singleton instance
export const transcriptService = new TranscriptService();
