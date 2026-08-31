import type { StoredAnnotation } from '@/services/indexedDBService';
import type { TranscriptAnnotation } from '@/types';

export function formatStoredAnnotations(annotations: StoredAnnotation[]): TranscriptAnnotation[] {
  return annotations.map(annotation => {
    const formatted: TranscriptAnnotation = {
      id: annotation.id,
      type: annotation.type,
      anchorTime: annotation.anchorTime,
      createdAt: annotation.createdAt,
    };
    if (annotation.text !== undefined) formatted.text = annotation.text;
    if (annotation.imageData !== undefined) formatted.imageData = annotation.imageData;
    if (annotation.imageMime !== undefined) formatted.imageMime = annotation.imageMime;
    return formatted;
  });
}
