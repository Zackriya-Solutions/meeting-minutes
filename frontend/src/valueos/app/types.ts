// VALUEOS: shared types for the redesigned app flow.
import type { CaptureResult } from '../shell/flowTypes';

/** The metadata chosen in the wizard, before any transcript exists. Same shape as a
 *  CaptureResult minus the transcript text (which the Recording screen produces). */
export type StartCallMeta = Omit<CaptureResult, 'transcriptText'>;

/** A call currently being recorded — the single on-air call (there can only be one). */
export interface ActiveCall {
  meta: StartCallMeta;
  startedAt: number;
}
