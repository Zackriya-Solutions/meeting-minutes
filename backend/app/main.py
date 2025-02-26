# Standard library imports
import asyncio
import json
import logging
import os
import uuid
from datetime import datetime
from functools import partial
from threading import Lock
from typing import Optional, Dict, Any, List

# Third-party imports
from fastapi import FastAPI, HTTPException, BackgroundTasks, UploadFile, File
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from pydantic import BaseModel
import uvicorn
from dotenv import load_dotenv

# Local application/library-specific imports
from db import DatabaseManager
from Process_transcrip import (
    TranscriptProcessor, MeetingSummarizer, Summary,
    SYSTEM_PROMPT, Agent, RunContext, Section, Block
)

# Load environment variables
load_dotenv()

# Configure logger with line numbers and function names
logger = logging.getLogger(__name__)
logger.setLevel(logging.DEBUG)

# Create console handler with formatting
console_handler = logging.StreamHandler()
console_handler.setLevel(logging.DEBUG)

# Create formatter with line numbers and function names
formatter = logging.Formatter(
    '%(asctime)s - %(levelname)s - [%(filename)s:%(lineno)d - %(funcName)s()] - %(message)s',
    datefmt='%Y-%m-%d %H:%M:%S'
)
console_handler.setFormatter(formatter)

# Add handler to logger if not already added
if not logger.handlers:
    logger.addHandler(console_handler)

app = FastAPI(
    title="Meeting Summarizer API",
    description="API for processing and summarizing meeting transcripts",
    version="1.0.0"
)

# Configure CORS
app.add_middleware(
    CORSMiddleware,
    allow_origins=[
        "http://localhost:3118",     # Dev server
        "http://localhost:*",        # Any local port
        "tauri://localhost",         # Tauri app
        "tauri://*",                # Any Tauri origin
        "app://localhost",          # Tauri app in release mode
        "app://*",                  # Any app:// origin
    ],
    allow_credentials=True,
    allow_methods=["*"],            # Allow all methods
    allow_headers=["*"],
    max_age=3600,  # Cache preflight requests for 1 hour
)


class Meeting(BaseModel):
    id: str
    title: str
    date: str
    time: Optional[str] = None
    attendees: List[str] = []
    tags: List[str] = []
    content: str
    created_at: str
    updated_at: str

class TranscriptRequest(BaseModel):
    """Request model for transcript text"""
    text: str
    model: str
    model_name: str
    chunk_size: Optional[int] = 5000
    overlap: Optional[int] = 1000


class TranscriptResponse(BaseModel):
    """Response model for transcript processing"""
    message: str
    num_chunks: int
    data: Dict[str, Any]


class SummaryProcessor:
    """Handles the processing of summaries in a thread-safe way"""
    def __init__(self):
        try:
            self.db = DatabaseManager()
            self._lock = Lock()
            
            # Load API key and validate
            api_key = os.getenv('ANTHROPIC_API_KEY')
            if not api_key:
                logger.error("ANTHROPIC_API_KEY environment variable not set")
                raise ValueError("ANTHROPIC_API_KEY environment variable not set")
            
            logger.info("Initializing SummaryProcessor components")
            self.transcript_processor = TranscriptProcessor()
            self.summarizer = MeetingSummarizer(api_key)
            self.agent = Agent(
                model=self.summarizer.model,
                system_prompt=SYSTEM_PROMPT
            )
            self.collection = None
            self.final_summary_result = None
            logger.info("SummaryProcessor initialized successfully")
        except Exception as e:
            logger.error(f"Failed to initialize SummaryProcessor: {str(e)}", exc_info=True)
            raise

    async def process_transcript(self, text: str, model: str, model_name: str, chunk_size: int = 5000, overlap: int = 1000) -> tuple:
        """Process a transcript text"""
        try:
            if not text:
                raise ValueError("Empty transcript text provided")
            
            # Validate chunk_size and overlap
            if chunk_size <= 0:
                raise ValueError("chunk_size must be positive")
            if overlap < 0:
                raise ValueError("overlap must be non-negative")
            if overlap >= chunk_size:
                overlap = chunk_size - 1  # Ensure overlap is less than chunk_size
            
            # Ensure step size is positive
            step_size = chunk_size - overlap
            if step_size <= 0:
                chunk_size = overlap + 1  # Adjust chunk_size to ensure positive step
                
            logger.info("Initializing ChromaDB collection")
            self.transcript_processor.initialize_collection()
            self.collection = self.transcript_processor.collection
            
            if not self.collection:
                raise ValueError("Failed to initialize ChromaDB collection")
            
            logger.info(f"Processing transcript of length {len(text)} with chunk_size={chunk_size}, overlap={overlap}")
            # Pass text as positional arg, chunk_size and overlap as keyword args
            num_chunks, all_json_data = await self.transcript_processor.process_transcript(
                text=text,  # Pass as keyword arg to be explicit
                model=model,
                model_name=model_name,
                chunk_size=chunk_size,
                overlap=overlap
            )
            logger.info(f"Successfully processed transcript into {num_chunks} chunks")
            
            return num_chunks, all_json_data
        except Exception as e:
            logger.error(f"Error processing transcript: {str(e)}", exc_info=True)
            raise

    async def process_summary(self, process_id: str) -> Dict[str, Any]:
        """Process a summary in a thread-safe way"""
        try:
            logger.info(f"Processing summary for process {process_id}")
            if not self.collection:
                logger.info("Initializing ChromaDB collection")
                self.transcript_processor.initialize_collection()
                self.collection = self.transcript_processor.collection
                
                if not self.collection:
                    raise ValueError("Failed to initialize ChromaDB collection")
            
            # Run the agent to generate summary
            logger.info("Running agent to generate summary")
            try:
                run_result = await self.agent.run(
                    'What is the summary of the following meeting? Use tools to get the data'
                )
                logger.info("Successfully received model response")
                
                if not run_result or not hasattr(run_result, 'data'):
                    raise ValueError("Invalid response from agent: missing data")
                
                total_summary_in_pydantic = self.summarizer.generate_summary(run_result.data)
                
                # Validate summary has content
                if not any([
                    total_summary_in_pydantic.Agenda.blocks,
                    total_summary_in_pydantic.Decisions.blocks,
                    total_summary_in_pydantic.ActionItems.blocks,
                    total_summary_in_pydantic.ClosingRemarks.blocks
                ]):
                    raise ValueError("No content found in summary")

                # Convert to JSON using Pydantic's json() method
                json_data = total_summary_in_pydantic.model_dump_json(indent=2)
                raw_summary = json.loads(json_data)

                # Format the summary to match the frontend Summary type
                formatted_summary = {}
                for section_name, section_data in raw_summary.items():
                    formatted_blocks = []
                    for block in section_data.get('blocks', []):
                        formatted_block = {
                            'id': block.get('id', str(uuid.uuid4())),
                            'type': block.get('type', 'text'),  # Default to 'text' if not specified
                            'content': block.get('content', ''),
                            'color': block.get('color', 'default')
                        }
                        formatted_blocks.append(formatted_block)

                    formatted_summary[section_name] = {
                        'title': section_data.get('title', section_name),
                        'blocks': formatted_blocks
                    }

                # Return the result with usage information
                result = {
                    "summary": formatted_summary,
                    "usage": {
                        "requests": run_result._usage.requests if hasattr(run_result, '_usage') else 0,
                        "request_tokens": run_result._usage.request_tokens if hasattr(run_result, '_usage') else 0,
                        "response_tokens": run_result._usage.response_tokens if hasattr(run_result, '_usage') else 0,
                        "total_tokens": run_result._usage.total_tokens if hasattr(run_result, '_usage') else 0
                    }
                }
                
                logger.info(f"Successfully generated summary for process {process_id}")
                return result
                
            except Exception as e:
                logger.error(f"Error running agent: {str(e)}", exc_info=True)
                if "empty model response" in str(e).lower():
                    raise ValueError("Empty model response received. This may indicate an issue with the API key or model configuration.")
                raise
            
        except Exception as e:
            logger.error(f"Error processing summary: {str(e)}", exc_info=True)
            raise

    def cleanup(self):
        """Cleanup resources"""
        try:
            logger.info("Cleaning up resources")
            self.transcript_processor.cleanup()
            logger.info("Cleanup completed successfully")
        except Exception as e:
            logger.error(f"Error during cleanup: {str(e)}", exc_info=True)

# Initialize processor
processor = SummaryProcessor()

# Define tools
@processor.agent.tool
async def query_transcript(ctx: RunContext, query: str) -> str:
    """Query the transcript to extract information. Returns the content and chunk IDs for deletion."""
    try:
        logger.info(f"Querying transcript with: {query}")
        
        # Check if there are any chunks left
        if not processor.collection:
            logger.error("No ChromaDB collection available")
            return "Error: No transcript loaded. Please process a transcript first."
            
        collection_data = processor.collection.get()
        if not collection_data['ids']:
            logger.info("No chunks left to process")
            return "CHROMADB_EMPTY: All chunks have been processed."
            
        # Get unprocessed chunks
        logger.info("Querying ChromaDB for relevant chunks")
        results = processor.collection.query(
            query_texts=[query],
            n_results=1
        )
        
        if not results or not results['documents'] or not results['documents'][0]:
            logger.info("No results found for query")
            return "No results found for the query"
            
        # Process and immediately delete chunks
        combined_result = ""
        chunk_ids = []
        
        for doc, metadata, id in zip(results['documents'][0], results['metadatas'][0], results['ids'][0]):
            combined_result += f"\n{doc}\n"
            chunk_ids.append(id)
        
        # Delete the chunks we just processed
        if chunk_ids:
            try:
                logger.info(f"Deleting {len(chunk_ids)} processed chunks")
                processor.collection.delete(ids=chunk_ids)
                
                # Verify deletion
                remaining = processor.collection.get()
                logger.info(f"Remaining chunks: {len(remaining['ids'])}")
                
            except Exception as e:
                logger.error(f"Error deleting chunks: {str(e)}", exc_info=True)
                return f"Error deleting chunks: {str(e)}"
            
        return combined_result.strip()
        
    except Exception as e:
        logger.error(f"Error querying transcript: {str(e)}", exc_info=True)
        return f"Error: {str(e)}"

@processor.agent.tool
async def delete_processed_chunks(ctx: RunContext) -> str:
    """Delete all processed chunks from the collection"""
    try:
        if not hasattr(ctx, 'processed_chunks') or not ctx.processed_chunks:
            return "No chunks to delete"
            
        chunk_ids = list(ctx.processed_chunks)
        processor.collection.delete(ids=chunk_ids)
        
        # Clear the processed chunks
        ctx.processed_chunks.clear()
        
        return f"Successfully deleted {len(chunk_ids)} chunks"
        
    except Exception as e:
        logger.error(f"Error deleting chunks: {e}")
        return f"Error deleting chunks: {str(e)}"

@processor.agent.tool
async def add_action_item(ctx: RunContext, title: str, content: str) -> str:
    """Add an action item to the summary"""
    try:
        logger.info(f"Adding action item: {title}")
        result = processor.summarizer.add_action_item(ctx, title, content)
        logger.info("Successfully added action item")
        return f"Successfully added action item: {result}"
    except Exception as e:
        logger.error(f"Error adding action item: {str(e)}", exc_info=True)
        return f"Error adding action item: {str(e)}"

@processor.agent.tool
async def add_agenda_item(ctx: RunContext, title: str, content: str) -> str:
    """Add an agenda item to the summary"""
    try:
        logger.info(f"Adding agenda item: {title}")
        result = processor.summarizer.add_agenda_item(ctx, title, content)
        logger.info("Successfully added agenda item")
        return f"Successfully added agenda item: {result}"
    except Exception as e:
        logger.error(f"Error adding agenda item: {str(e)}", exc_info=True)
        return f"Error adding agenda item: {str(e)}"

@processor.agent.tool
async def add_decision(ctx: RunContext, title: str, content: str) -> str:
    """Add a decision to the summary"""
    try:
        logger.info(f"Adding decision: {title}")
        result = processor.summarizer.add_decision(ctx, title, content)
        logger.info("Successfully added decision")
        return f"Successfully added decision: {result}"
    except Exception as e:
        logger.error(f"Error adding decision: {str(e)}", exc_info=True)
        return f"Error adding decision: {str(e)}"

@processor.agent.tool
async def save_final_summary_result(ctx: RunContext) -> str:
    """
    Save the final meeting summary result to a file
    args:
        ctx (RunContext): The run context

    returns:    
        str: Status message indicating success or failure
    """
    try:
        # Get the final summary result
        summary = processor.summarizer.generate_summary(ctx)
        
        # Validate summary has content
        if not any([
            summary.Agenda.blocks,
            summary.Decisions.blocks,
            summary.ActionItems.blocks,
            summary.ClosingRemarks.blocks
        ]):
            return "Error: No content found in summary. Please add some items first."

        # Convert to JSON using Pydantic's json() method which handles nested models
        json_data = summary.model_dump_json(indent=2)

        final_summary_result = json_data
        
        # Save to file with error handling
        try:
            with open('final_summary_result.json', 'w') as f:
                f.write(json_data)
            return "Successfully saved final summary result to file"
        except IOError as e:
            logger.error(f"Failed to write summary to file: {e}")
            return f"Error saving to file: {str(e)}"
            
    except Exception as e:
        logger.error(f"Error generating or saving summary: {e}")
        return f"Error processing summary: {str(e)}"

@processor.agent.tool
async def get_final_summary(ctx: RunContext) -> Summary:
    """Get the final meeting summary result"""
    try:
        logger.info("Generating final summary")
        summary = processor.summarizer.generate_summary(ctx)
        logger.info("Successfully generated final summary")
        return summary
    except Exception as e:
        logger.error(f"Error generating final summary: {str(e)}", exc_info=True)
        raise

async def process_transcript_background(process_id: str, transcript: TranscriptRequest):
    """Background task to process transcript"""
    try:
        logger.info(f"Starting background processing for process_id: {process_id}")
        
        # Initialize components with proper dependencies
        deps = {
            "db": processor.db,
            "transcript_processor": processor.transcript_processor,
            "summarizer": processor.summarizer
        }
        
        ctx = RunContext(
            deps=deps,
            model=processor.summarizer.model,
            usage={},
            prompt=SYSTEM_PROMPT
        )
        ctx.process_id = process_id
        
        # Process transcript
        num_chunks, all_json_data = await processor.transcript_processor.process_transcript(
            text=transcript.text,
            model=transcript.model,
            model_name=transcript.model_name,
            chunk_size=transcript.chunk_size,
            overlap=transcript.overlap
        )
        
        # Create final summary structure
        final_summary = {
            "MeetingName": "",
            "SectionSummary": {
                "title": "Section Summary",
                "blocks": []
            },
            "CriticalDeadlines": {
                "title": "Critical Deadlines",
                "blocks": []
            },
            "KeyItemsDecisions": {
                "title": "Key Items & Decisions",
                "blocks": []
            },
            "ImmediateActionItems": {
                "title": "Immediate Action Items",
                "blocks": []
            },
            "NextSteps": {
                "title": "Next Steps",
                "blocks": []
            },
            "OtherImportantPoints": {
                "title": "Other Important Points",
                "blocks": []
            },
            "ClosingRemarks": {
                "title": "Closing Remarks",
                "blocks": []
            }
        }
        
        # Process each chunk's data
        for json_str in all_json_data:
            json_dict = json.loads(json_str)
            final_summary["MeetingName"] = json_dict["MeetingName"]
            final_summary["SectionSummary"]["blocks"].extend(json_dict["SectionSummary"]["blocks"])
            final_summary["CriticalDeadlines"]["blocks"].extend(json_dict["CriticalDeadlines"]["blocks"])
            final_summary["KeyItemsDecisions"]["blocks"].extend(json_dict["KeyItemsDecisions"]["blocks"])
            final_summary["ImmediateActionItems"]["blocks"].extend(json_dict["ImmediateActionItems"]["blocks"])
            final_summary["NextSteps"]["blocks"].extend(json_dict["NextSteps"]["blocks"])
            final_summary["OtherImportantPoints"]["blocks"].extend(json_dict["OtherImportantPoints"]["blocks"])
            final_summary["ClosingRemarks"]["blocks"].extend(json_dict["ClosingRemarks"]["blocks"])
        
        # Update database with meeting name
        if final_summary["MeetingName"]:
            await processor.db.update_meeting_name(process_id, final_summary["MeetingName"])
        
        # Save final result
        await processor.db.update_process(process_id, status="completed", result=json.dumps(final_summary))
        logger.info(f"Background processing completed for process_id: {process_id}")
        
    except Exception as e:
        error_msg = str(e)
        logger.error(f"Error in background processing for {process_id}: {error_msg}")
        await processor.db.update_process(process_id, status="failed", error=error_msg)

@app.post("/process-transcript")
async def process_transcript_api(
    transcript: TranscriptRequest,
    background_tasks: BackgroundTasks
):
    """Process a transcript text with background processing"""
    try:
        # Create new process
        process_id = await processor.db.create_process()
        
        # Save transcript data
        await processor.db.save_transcript(
            process_id,
            transcript.text,
            transcript.model,
            transcript.model_name,
            transcript.chunk_size,
            transcript.overlap
        )
        
        # Start background processing
        background_tasks.add_task(
            process_transcript_background,
            process_id,
            transcript
        )
        
        return JSONResponse({
            "message": "Processing started",
            "process_id": process_id
        })
        
    except Exception as e:
        logger.error(f"Error in process_transcript_api: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))

@app.get("/get-summary/{process_id}")
async def get_summary(process_id: str):
    """Get the summary for a given process ID"""
    try:
        result = await processor.db.get_transcript_data(process_id)
        if not result:
            return JSONResponse(
                status_code=404,
                content={
                    "status": "error",
                    "meetingName": None,
                    "process_id": process_id,
                    "data": None,
                    "start": None,
                    "end": None,
                    "error": "Process ID not found"
                }
            )

        status = result["status"].lower()  # Convert to lowercase for case-insensitive comparison
        
        # Parse result data if available
        summary_data = None
        if result.get("result"):
            try:
                # The result is already a JSON string, so we need to parse it
                summary_data = json.loads(result["result"])
                if isinstance(summary_data, str):
                    # If it's still a string, it means it was double-encoded
                    summary_data = json.loads(summary_data)
            except json.JSONDecodeError as e:
                logger.error(f"Failed to parse JSON data for process {process_id}: {str(e)}")
                
        # Build response
        response = {
            "status": "processing" if status in ["processing", "pending"] else status,
            "meetingName": summary_data.get("MeetingName") if summary_data else None,
            "process_id": process_id,
            "start": result.get("start_time"),
            "end": result.get("end_time"),
            "data": summary_data
        }
        
        if status == "failed":
            response["status"] = "error"
            response["error"] = result.get("error", "Unknown error")
            return JSONResponse(status_code=400, content=response)
            
        elif status in ["processing", "pending"]:
            return JSONResponse(status_code=202, content=response)
            
        elif status == "completed":
            if not summary_data:
                response["status"] = "error"
                response["error"] = "Invalid or missing summary data"
                return JSONResponse(status_code=500, content=response)
            return JSONResponse(status_code=200, content=response)
            
        else:
            response["status"] = "error"
            response["error"] = f"Unknown status: {status}"
            return JSONResponse(status_code=400, content=response)
            
    except Exception as e:
        logger.error(f"Error getting summary for {process_id}: {str(e)}")
        return JSONResponse(
            status_code=500,
            content={
                "status": "error",
                "meetingName": None,
                "process_id": process_id,
                "data": None,
                "start": None,
                "end": None,
                "error": str(e)
            }
        )

@app.post("/upload-transcript")
async def upload_transcript(
    background_tasks: BackgroundTasks,
    model: str = "claude",
    model_name: str = "claude-3-5-sonnet-latest",
    chunk_size: Optional[int] = 5000,
    overlap: Optional[int] = 1000,
    file: UploadFile = File(...),
) -> Dict[str, str]:
    """Upload and process a transcript file"""
    logger.info(f"Received transcript file upload: {file.filename}")
    try:
        content = await file.read()
        transcript_text = content.decode()
        logger.info("Successfully decoded transcript file content")
        
        # Create transcript request
        transcript = TranscriptRequest(
            text=transcript_text,
            model=model,
            model_name=model_name,
            chunk_size=chunk_size,
            overlap=overlap
        )
        
        # Create new process
        process_id = await processor.db.create_process()
        
        # Save transcript data
        await processor.db.save_transcript(
            process_id,
            transcript_text,
            model,
            model_name,
            chunk_size,
            overlap
        )
        
        # Start background processing
        background_tasks.add_task(
            process_transcript_background,
            process_id,
            transcript
        )
        
        return JSONResponse({
            "message": "Processing started",
            "process_id": process_id
        })
        
    except Exception as e:
        logger.error(f"Error processing transcript file: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=str(e))

@app.post("/start-summarization")
async def start_summarization(background_tasks: BackgroundTasks) -> Dict[str, str]:
    """Start the summarization process"""
    logger.info("Received request to start summarization")
    try:
        # Create a new process in the database
        process_id = await processor.db.create_process()
        logger.info(f"Created process with ID: {process_id}")
        
        # Start background processing
        background_tasks.add_task(process_and_update, process_id)
        logger.info(f"Added background task for process {process_id}")
        
        return {"process_id": process_id}
    except Exception as e:
        logger.error(f"Error in start_summarization: {str(e)}", exc_info=True)
        raise HTTPException(status_code=500, detail=str(e))

async def process_and_update(process_id: str):
    """Process the summary and update the database"""
    logger.info(f"Starting background processing for {process_id}")
    try:
        # Process the summary
        result = await processor.process_summary(process_id)
        logger.info(f"Generated summary for {process_id}")
        
        # Update the database with the result
        await processor.db.update_process(process_id, "COMPLETED", result=result)
        logger.info(f"Updated process {process_id} as completed")
    except Exception as e:
        logger.error(f"Error in process_and_update for {process_id}: {str(e)}", exc_info=True)
        await processor.db.update_process(process_id, "FAILED", error=str(e))

@app.on_event("shutdown")
async def shutdown_event():
    """Cleanup on API shutdown"""
    logger.info("API shutting down, cleaning up resources")
    try:
        await processor.cleanup()
        logger.info("Successfully cleaned up resources")
    except Exception as e:
        logger.error(f"Error during cleanup: {str(e)}", exc_info=True)

@app.get("/meetings", response_model=List[Meeting])
async def get_meetings():
    """Get all meetings"""
    try:
        meetings = await processor.db.get_meetings()
        return meetings
    except Exception as e:
        logger.error(f"Error getting meetings: {str(e)}")
        raise HTTPException(status_code=500, detail="Failed to fetch meetings")

@app.get("/meetings/{meeting_id}", response_model=Meeting)
async def get_meeting(meeting_id: str):
    """Get a specific meeting"""
    try:
        meeting = await processor.db.get_meeting(meeting_id)
        if not meeting:
            raise HTTPException(status_code=404, detail="Meeting not found")
        return meeting
    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error getting meeting {meeting_id}: {str(e)}")
        raise HTTPException(status_code=500, detail="Failed to fetch meeting")

@app.delete("/meetings/{meeting_id}")
async def delete_meeting(meeting_id: str):
    """Delete a meeting"""
    try:
        meeting = await processor.db.get_meeting(meeting_id)
        if not meeting:
            raise HTTPException(status_code=404, detail="Meeting not found")
        
        await processor.db.delete_meeting(meeting_id)
        return {"message": "Meeting deleted successfully"}
    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error deleting meeting {meeting_id}: {str(e)}")
        raise HTTPException(status_code=500, detail="Failed to delete meeting")

class AddMeetingRequest(BaseModel):
    title: str
    date: str
    time: Optional[str] = None
    attendees: List[str] = []
    tags: List[str] = []
    content: Optional[str] = None

@app.post("/meetings")
async def add_meeting(request: AddMeetingRequest):
    """Add a new meeting"""
    try:
        meeting_id = await processor.db.add_meeting(
            title=request.title,
            date=request.date,
            time=request.time,
            attendees=request.attendees,
            tags=request.tags,
            content=request.content
        )
        return JSONResponse(status_code=201, content={"message": "Meeting added successfully", "meeting_id": meeting_id})
    except Exception as e:
        logger.error(f"Error adding meeting: {str(e)}")
        raise HTTPException(status_code=500, detail="Failed to add meeting")

if __name__ == "__main__":
    import multiprocessing
    multiprocessing.freeze_support()
    uvicorn.run(app, host="0.0.0.0", port=5167)
