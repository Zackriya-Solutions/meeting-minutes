import { useState, useCallback, useRef, useEffect } from 'react';
import { ChunkStatus, ProcessingProgress } from '../components/ChunkProgressDisplay';

export interface ProcessingSession {
  session_id: string;
  total_audio_duration_ms: number;
  chunk_duration_ms: number;
  start_time: number;
  is_paused: boolean;
  model_name: string;
}

export function useProcessingProgress() {
  const [progress, setProgress] = useState<ProcessingProgress>({
    total_chunks: 0,
    completed_chunks: 0,
    processing_chunks: 0,
    failed_chunks: 0,
    chunks: []
  });

  const [session, setSession] = useState<ProcessingSession | null>(null);
  const [isActive, setIsActive] = useState(false);
  const processingTimeRef = useRef<{ [chunkId: number]: number }>({});

  // Initialize a new processing session
  const initializeSession = useCallback((
    totalAudioDurationMs: number,
    chunkDurationMs: number = 30000, // 30 seconds default
    modelName: string = 'unknown'
  ) => {
    const totalChunks = Math.ceil(totalAudioDurationMs / chunkDurationMs);

    const newSession: ProcessingSession = {
      session_id: `session_${Date.now()}`,
      total_audio_duration_ms: totalAudioDurationMs,
      chunk_duration_ms: chunkDurationMs,
      start_time: Date.now(),
      is_paused: false,
      model_name: modelName
    };

    setSession(newSession);
    setProgress({
      total_chunks: totalChunks,
      completed_chunks: 0,
      processing_chunks: 0,
      failed_chunks: 0,
      chunks: Array.from({ length: totalChunks }, (_, i) => ({
        chunk_id: i,
        status: 'pending'
      }))
    });
    setIsActive(true);

    console.log(`Initialized processing session for ${totalChunks} chunks`);
  }, []);

  // Start processing a specific chunk
  const startChunkProcessing = useCallback((chunkId: number) => {
    processingTimeRef.current[chunkId] = Date.now();

    setProgress(prev => ({
      ...prev,
      processing_chunks: prev.processing_chunks + 1,
      chunks: prev.chunks.map(chunk =>
        chunk.chunk_id === chunkId
          ? { ...chunk, status: 'processing', start_time: Date.now() }
          : chunk
      )
    }));

    console.log(`Started processing chunk ${chunkId}`);
  }, []);

  // Complete a chunk with transcribed text
  const completeChunk = useCallback((chunkId: number, transcribedText: string) => {
    const startTime = processingTimeRef.current[chunkId];
    const endTime = Date.now();
    const duration = startTime ? endTime - startTime : 0;

    setProgress(prev => ({
      ...prev,
      completed_chunks: prev.completed_chunks + 1,
      processing_chunks: Math.max(0, prev.processing_chunks - 1),
      chunks: prev.chunks.map(chunk =>
        chunk.chunk_id === chunkId
          ? {
              ...chunk,
              status: 'completed',
              end_time: endTime,
              duration_ms: duration,
              text_preview: transcribedText.slice(0, 100) // First 100 chars
            }
          : chunk
      )
    }));

    delete processingTimeRef.current[chunkId];
    console.log(`Completed chunk ${chunkId} in ${duration}ms`);
  }, []);

  // Mark a chunk as failed
  const failChunk = useCallback((chunkId: number, errorMessage: string) => {
    setProgress(prev => ({
      ...prev,
      failed_chunks: prev.failed_chunks + 1,
      processing_chunks: Math.max(0, prev.processing_chunks - 1),
      chunks: prev.chunks.map(chunk =>
        chunk.chunk_id === chunkId
          ? {
              ...chunk,
              status: 'failed',
              error_message: errorMessage,
              end_time: Date.now()
            }
          : chunk
      )
    }));

    delete processingTimeRef.current[chunkId];
    console.log(`Failed chunk ${chunkId}: ${errorMessage}`);
  }, []);

  // Calculate estimated remaining time
  const calculateEstimatedTime = useCallback(() => {
    if (!session || progress.completed_chunks === 0) {
      return undefined;
    }

    const currentTime = Date.now();
    const elapsedTime = currentTime - session.start_time;
    const averageTimePerChunk = elapsedTime / progress.completed_chunks;
    const remainingChunks = progress.total_chunks - progress.completed_chunks;

    return remainingChunks * averageTimePerChunk;
  }, [session, progress.completed_chunks, progress.total_chunks]);

  // Update estimated time in progress
  useEffect(() => {
    const estimatedTime = calculateEstimatedTime();
    if (estimatedTime !== undefined) {
      setProgress(prev => ({
        ...prev,
        estimated_remaining_ms: estimatedTime
      }));
    }
  }, [calculateEstimatedTime]);

  // Pause processing
  const pauseProcessing = useCallback(() => {
    if (session) {
      setSession(prev => prev ? { ...prev, is_paused: true } : null);
      console.log('Processing paused');
    }
  }, [session]);

  // Resume processing
  const resumeProcessing = useCallback(() => {
    if (session) {
      setSession(prev => prev ? { ...prev, is_paused: false } : null);
      console.log('Processing resumed');
    }
  }, [session]);

  // Cancel processing
  const cancelProcessing = useCallback(() => {
    setIsActive(false);
    setSession(null);
    setProgress({
      total_chunks: 0,
      completed_chunks: 0,
      processing_chunks: 0,
      failed_chunks: 0,
      chunks: []
    });
    processingTimeRef.current = {};
    console.log('Processing cancelled');
  }, []);

  // Reset for new session
  const reset = useCallback(() => {
    setIsActive(false);
    setSession(null);
    setProgress({
      total_chunks: 0,
      completed_chunks: 0,
      processing_chunks: 0,
      failed_chunks: 0,
      chunks: []
    });
    processingTimeRef.current = {};
  }, []);

  // Save/load progress state for resume functionality
  const saveProgressState = useCallback(() => {
    if (!session) return null;

    const state = {
      session,
      progress,
      processing_times: processingTimeRef.current,
      is_active: isActive
    };

    localStorage.setItem('transcription_progress', JSON.stringify(state));
    return state;
  }, [session, progress, isActive]);

  const loadProgressState = useCallback(() => {
    try {
      const saved = localStorage.getItem('transcription_progress');
      if (!saved) return false;

      const state = JSON.parse(saved);
      setSession(state.session);
      setProgress(state.progress);
      setIsActive(state.is_active);
      processingTimeRef.current = state.processing_times || {};

      console.log('Loaded saved progress state');
      return true;
    } catch (error) {
      console.error('Failed to load progress state:', error);
      return false;
    }
  }, []);

  const clearSavedState = useCallback(() => {
    localStorage.removeItem('transcription_progress');
  }, []);

  // Check if processing is complete
  const isComplete = progress.total_chunks > 0 &&
    progress.completed_chunks === progress.total_chunks;

  // Check if there are any failed chunks
  const hasFailures = progress.failed_chunks > 0;

  return {
    // State
    progress,
    session,
    isActive,
    isComplete,
    hasFailures,
    isPaused: session?.is_paused || false,

    // Actions
    initializeSession,
    startChunkProcessing,
    completeChunk,
    failChunk,
    pauseProcessing,
    resumeProcessing,
    cancelProcessing,
    reset,

    // Persistence
    saveProgressState,
    loadProgressState,
    clearSavedState
  };
}