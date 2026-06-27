export type DiarizationMode = 'live_plus_post_call' | 'post_call_only' | 'off';
export type OverlapHandling = 'multiple_speakers';
export type DiarizationStatus =
  | 'none'
  | 'provisional'
  | 'final'
  | 'fallback_to_live'
  | 'failed'
  | 'needs_review';

export interface DiarizationSettings {
  enabled: boolean;
  mode: DiarizationMode;
  showProvisionalLabels: boolean;
  postCallRefinementEnabled: boolean;
  overlapHandling: OverlapHandling;
  speakerReviewEnabled: boolean;
}

export const DEFAULT_DIARIZATION_SETTINGS: DiarizationSettings = {
  enabled: false,
  mode: 'live_plus_post_call',
  showProvisionalLabels: true,
  postCallRefinementEnabled: true,
  overlapHandling: 'multiple_speakers',
  speakerReviewEnabled: true,
};
