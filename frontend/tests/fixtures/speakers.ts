import type { MeetingSpeaker } from '../../src/types';

/** A meeting speaker with one row and no name, link or suggestion; override what a test needs. */
export function makeSpeaker(speaker_key: string, overrides: Partial<MeetingSpeaker> = {}): MeetingSpeaker {
  return {
    speaker_key,
    display_name: null,
    speech_seconds: 1,
    row_count: 1,
    row_seconds: 1,
    person_id: null,
    name_source: null,
    suggested_person_id: null,
    suggested_name: null,
    suggestion_source: null,
    suggestion_reason: null,
    sample_start_s: null,
    sample_end_s: null,
    ...overrides,
  };
}
