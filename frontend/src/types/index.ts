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
  speaker?: string | null;
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
  reasoning_stripped?: boolean;
  normalization_fallback?: boolean;
  // Legacy format fields
  MeetingName?: string;
  _section_order?: string[];
  [key: string]: any; // For legacy section data
}

export type MeetingSummary = Summary | SummaryDataResponse;

export type SummaryProcessStatus =
  | 'pending'
  | 'processing'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'error'
  | 'idle';

export interface ProcessTranscriptResponse {
  message: string;
  process_id: string;
}

export interface CancelSummaryResponse {
  cancelled: boolean;
  message: string;
  meeting_id: string;
}

export interface SummaryProcessResponse {
  status: SummaryProcessStatus;
  meetingName: string | null;
  meeting_id: string;
  start: string | null;
  end: string | null;
  data: unknown | null;
  error: string | null;
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
  speaker?: string | null;
}

export type NameSource = 'user' | 'voice' | 'conversation';
export type SuggestionSource = 'voice' | 'conversation';

export interface MeetingSpeaker {
  speaker_key: string;
  display_name: string | null;
  speech_seconds: number;
  /** Transcript rows currently labelled with this speaker */
  row_count: number;
  /** Seconds covered by those rows */
  row_seconds: number;
  /** The person this speaker is linked to */
  person_id: string | null;
  /** Who set display_name: typed or confirmed by the user, matched by voice, or found in the
   *  conversation. Null for names set before people existed (treated as typed). */
  name_source: NameSource | null;
  suggested_person_id: string | null;
  suggested_name: string | null;
  suggestion_source: SuggestionSource | null;
  /** Why the name is suggested, e.g. "voice match 0.68" or "addressed as Noah at 01:12" */
  suggestion_reason: string | null;
  /** Up to 8 s from the start of the speaker's longest single-speaker row; null without one */
  sample_start_s: number | null;
  sample_end_s: number | null;
}

export interface SpeakerJobStatus {
  meeting_id: string;
  /** Which job: identifying speakers, or finding names in the conversation */
  kind: 'identify' | 'naming';
  state: 'queued' | 'running';
  /** 'audio' | 'waiting' | 'download' | 'segmentation' | 'embeddings' | 'clustering' | 'splitting' | 'saving' | 'naming' | 'done'; null while queued */
  stage: string | null;
  percent: number;
  message: string;
}

/** A person whose voice is remembered across meetings (Settings → Speakers). */
export interface Person {
  id: string;
  name: string;
  meeting_count: number;
  /** Creation time of the newest meeting the person is linked in */
  last_seen: string | null;
}

/** A speaker of another meeting that was named by voice after a name was typed or confirmed. */
export interface PropagatedLink {
  meeting_id: string;
  speaker_key: string;
  person_id: string;
}

export interface NameOutcome {
  propagated: PropagatedLink[];
}

/** Payload of `diarization-complete`, for both job kinds. */
export interface SpeakerJobComplete {
  meeting_id: string;
  kind: 'identify' | 'naming';
  speaker_count: number;
  automatic: boolean;
  warning?: string | null;
  /** Naming: speakers given an auto name */
  named: number;
  /** Naming: speakers given a suggestion */
  suggested: number;
}

export interface DiarizationModelsStatus {
  installed: boolean;
  total_bytes: number;
  downloaded_bytes: number;
  directory: string;
}

/** How to play a meeting's recording (`api_prepare_meeting_playback`). */
export interface PlaybackSource {
  /** The audio file; played over the asset protocol. */
  path: string;
  /** Length in seconds of container time, which is the recording clock. */
  duration_s: number;
  /** (clock_s, file_s) points; the identity for every recording today. */
  time_table: [number, number][];
}
