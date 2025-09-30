export interface Message {
  id: string;
  content: string;
  timestamp: string;
}

export interface Transcript {
  id: string;
  text: string;
  timestamp: string;
  sequence_id?: number;
  chunk_start_time?: number;
  is_partial?: boolean;
  confidence?: number;
}

export interface TranscriptUpdate {
  text: string;
  timestamp: string;
  source: string;
  sequence_id: number;
  chunk_start_time: number;
  is_partial: boolean;
  confidence: number;
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
