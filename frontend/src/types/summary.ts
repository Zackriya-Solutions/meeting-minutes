export interface Block {
    id?: string;
    type?: string;
    content: string;
    color?: string;
    timestamp?: string;
    speaker?: string;
}

export interface Section {
    title: string;
    blocks: Block[];
}

export interface Summary {
    // Simple format (from current summary.ts)
    key_points?: string[];
    action_items?: string[];
    decisions?: string[];
    main_topics?: string[];
    participants?: string[];
    
    // Structured format (from types.ts and index.ts)
    key_points_section?: Section;
    action_items_section?: Section;
    decisions_section?: Section;
    main_topics_section?: Section;
    
    // Legacy format support
    MeetingName?: string;
    SectionSummary?: {
        Agenda?: Block[];
        Decisions?: Block[];
        ActionItems?: Block[];
        ClosingRemarks?: Block[];
        [key: string]: Block[] | undefined;
    };
    
    // Other sections
    CriticalDeadlines?: Section;
    KeyItemsDecisions?: Section;
    ImmediateActionItems?: Section;
    NextSteps?: Section;
    OtherImportantPoints?: Section;
    ClosingRemarks?: Section;
    
    // Allow for dynamic properties
    [key: string]: any;
}

export interface SummaryResponse {
    summary: Summary;
    raw_summary?: string;
    status?: "completed" | "processing" | "error";
    meetingName?: string | null;
    process_id?: string;
    data?: Summary | null;
    start?: string | null;
    end?: string | null;
    error?: string;
}

export interface ProcessRequest {
    transcript: string;
    metadata?: {
        meeting_title?: string;
        date?: string;
        duration?: number;
    };
    text?: string;
    model?: string;
    model_name?: string;
    chunk_size?: number;
    overlap?: number;
}
