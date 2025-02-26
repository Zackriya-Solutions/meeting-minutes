export interface Message {
  id: string;
  content: string;
  timestamp: string;
}

export interface Transcript {
  text: string;
  model?: string;
  model_name?: string;
  chunk_size?: number;
  overlap?: number;
  timestamp: string;
}

export interface Block {
  id: string;
  type: string;
  content: string;
  color?: string;
  timestamp?: string;
  speaker?: string;
}

export interface Section {
  title: string;
  blocks: Block[];
}

export interface ApiResponse {
  message: string;
  num_chunks: number;
  data: any[];
}