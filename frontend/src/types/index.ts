export interface Message {
  id: string;
  content: string;
  timestamp: string;
}

export interface Transcript {
  id: string;
  text: string;
  timestamp: string; // Wall-clock time (e.g., "14:30:05")
  sequence_id?: number;
  chunk_start_time?: number; // Legacy field
  is_partial?: boolean;
  confidence?: number;
  // NEW: Recording-relative timestamps for playback sync
  audio_start_time?: number; // Seconds from recording start (e.g., 125.3)
  audio_end_time?: number;   // Seconds from recording start (e.g., 128.6)
  duration?: number;          // Segment duration in seconds (e.g., 3.3)
  // Audio-channel source: 'mic' (local user) | 'system' (remote participants)
  speaker?: string | null;
  // Resolved diarized speaker profile id (takes precedence once available)
  speaker_id?: number | null;
}

export interface TranscriptUpdate {
  text: string;
  timestamp: string; // Wall-clock time for reference
  source: string;
  sequence_id: number;
  chunk_start_time: number; // Legacy field
  is_partial: boolean;
  confidence: number;
  // NEW: Recording-relative timestamps for playback sync
  audio_start_time: number; // Seconds from recording start
  audio_end_time: number;   // Seconds from recording start
  duration: number;          // Segment duration in seconds
  // Audio-channel source: 'mic' (local user) | 'system' (remote participants)
  speaker?: string | null;
}

/**
 * Resolve the display label for a transcript segment's speaker.
 *
 * Precedence:
 *  1. Diarized identity — when a `speakersById` map is supplied and the segment
 *     has a `speaker_id` present in it, the speaker's display name wins.
 *  2. Audio-channel tag (`speaker`) — 'mic' is the local user ("You"),
 *     'system' is the remote participants ("Others").
 * Returns null when the speaker is unknown (render nothing).
 */
export function resolveSpeakerLabel(
  input: { speaker_id?: number | null; speaker?: string | null },
  speakersById?: Map<number, string> | null
): string | null {
  if (input.speaker_id != null && speakersById) {
    const name = speakersById.get(input.speaker_id);
    if (name) return name;
  }
  switch (input.speaker) {
    case 'mic':
      return 'You';
    case 'system':
      return 'Others';
    default:
      return null;
  }
}

/** Localize only system-generated labels; user-confirmed display names stay untouched. */
export function localizeSpeakerLabel(
  label: string | null,
  translate: (value: string) => string,
): string | null {
  if (!label) return null;
  const automatic = label.match(/^Speaker\s+(\d+)$/i);
  if (automatic) return `${translate('Speaker')} ${automatic[1]}`;
  if (label === 'You' || label === 'Others') return translate(label);
  return label;
}

/**
 * A diarized speaker profile for a saved meeting (from `get_meeting_speakers`).
 */
export interface SpeakerInfo {
  id: number;
  display_name: string;
  is_confirmed: boolean;
  segment_count: number;
}

/** Result of the `diarization_status` command. */
export interface DiarizationStatus {
  available: boolean;
  model_dir: string;
}

/** Result of the `diarize_meeting` command. */
export interface DiarizeMeetingResult {
  meeting_id: string;
  speaker_count: number;
  assigned_segments: number;
  total_segments: number;
}

/** Payload of the `diarization-complete` event. */
export interface DiarizationCompletePayload {
  meeting_id: string;
  speaker_count: number;
  assigned_segments: number;
}

export interface Block {
  id: string;
  type: string;
  content: string;
  color: string;
}

export interface Section {
  title: string;
  blocks: Block[];
}

export interface Summary {
  [key: string]: Section;
}

export interface ApiResponse {
  message: string;
  num_chunks: number;
  data: any[];
}

export interface SummaryResponse {
  status: string;
  summary: Summary;
  raw_summary?: string;
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

// BlockNote-specific types
export type SummaryFormat = 'legacy' | 'markdown' | 'blocknote';

export interface BlockNoteBlock {
  id: string;
  type: string;
  props?: Record<string, any>;
  content?: any[];
  children?: BlockNoteBlock[];
}

export interface SummaryDataResponse {
  markdown?: string;
  summary_json?: BlockNoteBlock[];
  // Legacy format fields
  MeetingName?: string;
  _section_order?: string[];
  [key: string]: any; // For legacy section data
}

// Pagination types for optimized transcript loading
export interface MeetingMetadata {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  folder_path?: string;
}

export interface PaginatedTranscriptsResponse {
  transcripts: Transcript[];
  total_count: number;
  has_more: boolean;
}

// Transcript segment data for virtualized display
export interface TranscriptSegmentData {
  id: string;
  timestamp: number; // audio_start_time in seconds
  endTime?: number; // audio_end_time in seconds
  text: string;
  confidence?: number;
  // Audio-channel source: 'mic' (local user) | 'system' (remote participants)
  speaker?: string | null;
  // Resolved diarized speaker profile id (takes precedence once available)
  speaker_id?: number | null;
}
