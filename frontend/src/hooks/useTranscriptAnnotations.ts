import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { NewTranscriptAnnotation, TranscriptAnnotation } from '@/types';
import { imageDataToDataUrl, preserveLiveAnnotationImageData } from '@/lib/transcript-annotation-input';

function mimeForFile(fileName: string): string {
  const extension = fileName.split('.').pop()?.toLowerCase();
  if (extension === 'jpg' || extension === 'jpeg') return 'image/jpeg';
  if (extension === 'webp') return 'image/webp';
  if (extension === 'gif') return 'image/gif';
  return 'image/png';
}

export function useTranscriptAnnotations(meetingId: string | null) {
  const [annotations, setAnnotations] = useState<TranscriptAnnotation[]>([]);

  useEffect(() => {
    let cancelled = false;
    setAnnotations([]);
    if (!meetingId) return () => { cancelled = true; };
    void invoke<TranscriptAnnotation[]>('api_get_transcript_annotations', { meetingId })
      .then(result => { if (!cancelled) setAnnotations(result); })
      .catch(error => console.error('Failed to load transcript annotations:', error));
    return () => { cancelled = true; };
  }, [meetingId]);

  const addAnnotation = useCallback(async (input: NewTranscriptAnnotation) => {
    if (!meetingId) return;
    const saved = await invoke<TranscriptAnnotation>('api_add_transcript_annotation', {
      meetingId,
      annotation: {
        type: input.type,
        anchorTime: input.anchorTime,
        text: input.text ?? null,
        imageData: input.imageData ?? null,
        imageMime: input.imageMime ?? null,
      },
    });
    const liveAnnotation = preserveLiveAnnotationImageData(saved, input);
    setAnnotations(previous => [...previous, liveAnnotation].sort((a, b) => a.anchorTime - b.anchorTime));
  }, [meetingId]);

  const getAnnotationImage = useCallback(async (annotation: TranscriptAnnotation) => {
    if (!meetingId || !annotation.imageFile) return null;
    try {
      const bytes = await invoke<number[]>('api_get_annotation_image', {
        meetingId,
        imageFile: annotation.imageFile,
      });
      return imageDataToDataUrl(bytes, mimeForFile(annotation.imageFile));
    } catch (error) {
      console.error('Failed to load annotation image:', error);
      return null;
    }
  }, [meetingId]);

  return { annotations, addAnnotation, getAnnotationImage };
}
