export interface Meeting {
    id: string;
    title: string;
    date: string;
    time?: string;
    attendees: string[];
    tags: string[];
    content: string;
    created_at: string;
    updated_at: string;
} 

export interface Message {
    id: string;
    content: string;
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