import argparse
import json
import logging
import os
import uuid
from typing import Any, Dict, List, Optional

from chromadb import Client as ChromaClient, Settings
from dotenv import load_dotenv
from groq import file_from_path
from pydantic import BaseModel
from pydantic_ai import Agent, RunContext
from pydantic_ai.models.anthropic import AnthropicModel
from pydantic_ai.models.groq import GroqModel
from pydantic_ai.models.ollama import OllamaModel

# Set up logging
logging.basicConfig(
    level=logging.DEBUG,
    format="%(asctime)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

load_dotenv()  # Load environment variables from .env file

class Block(BaseModel):
    id: str
    type: str
    content: str
    color: Optional[str] = None
    timestamp: Optional[str] = None
    speaker: Optional[str] = None

class Section(BaseModel):
    title: str
    blocks: List[Block]

class ActionItem(BaseModel):
    """Represents an action item from the meeting"""
    title: str
    content: str

class Summary(BaseModel):
    MeetingName: str
    SectionSummary: Dict[str, List[Block]]
    CriticalDeadlines: Section
    KeyItemsDecisions: Section
    ImmediateActionItems: Section
    NextSteps: Section
    OtherImportantPoints: Section
    ClosingRemarks: Section

class MeetingMinutes(BaseModel):
    """Represents the meeting minutes response based on a section of the transcript. 
    The information shall have stuff like what is discussed, important dates, etc
    The section shall not have sub sections but only a title and blocks. Remember to split sub sections and all to blocks"""
    Section1 : Section
    Section2 : Section
    Section3 : Section
    Section4 : Section

class Transcript(BaseModel):
    text: str
    model: str
    model_name: str
    chunk_size: int = 5000
    overlap: int = 1000

class TranscriptProcessor:
    """Handles the processing and storage of meeting transcripts"""
    def __init__(self):
        """Initialize the transcript processor"""
        self.collection_name = "all_transcripts"
        self.chroma_client = None
        self.collection = None
        self.initialize_collection()

    def __del__(self):
        """Cleanup ChromaDB connection"""
        if self.chroma_client:
            try:
                self.collection = None
                self.chroma_client = None
            except Exception as e:
                logger.error(f"Error cleaning up ChromaDB: {e}")

    def initialize_collection(self):
        """Initialize or get the ChromaDB collection"""
        try:
            if self.chroma_client:
                self.collection = None
                self.chroma_client = None
                
            # Create new client with settings
            settings = Settings(
                allow_reset=True,
                is_persistent=True
            )
            self.chroma_client = ChromaClient(settings)
            
            try:
                # Try to get existing collection
                self.collection = self.chroma_client.get_collection(name=self.collection_name)
                logger.info(f"Retrieved existing collection: {self.collection_name}")
            except Exception:
                # Create new collection if it doesn't exist
                logger.info(f"Creating new collection: {self.collection_name}")
                self.collection = self.chroma_client.create_collection(name=self.collection_name)
            
            if not self.collection:
                raise RuntimeError("Failed to initialize ChromaDB collection")
                
        except Exception as e:
            logger.error(f"Error initializing ChromaDB: {e}")
            raise
    
    def cleanup(self):
        """Cleanup ChromaDB resources"""
        if self.chroma_client:
            try:
                self.collection = None
                self.chroma_client = None
            except Exception as e:
                logger.error(f"Error during cleanup: {e}")
    
    async def process_transcript(self, text: str = None, model="claude", model_name="claude-3-5-sonnet-latest", transcript_path: str = None, chunk_size: int = 5000, overlap: int = 1000):
        """Process and store transcript in chunks"""
        try:
            # Clear any existing collection
            if self.collection:
                try:
                    existing_data = self.collection.get()
                    if 'ids' in existing_data and existing_data['ids']:
                        self.collection.delete(ids=existing_data['ids'])
                except Exception as e:
                    logger.error(f"Error clearing collection: {e}", exc_info=True)
            
            if isinstance(transcript_path, str):
                if os.path.exists(transcript_path):
                    with open(transcript_path, 'r') as f:
                        transcript = f.read()
                    logger.info(f"Loaded transcript from file: {transcript_path}")
                else:
                    transcript = transcript_path
            else:
                transcript = text
            
            logger.info(f"Processing transcript of length {len(transcript)} with chunk_size={chunk_size}, overlap={overlap}")

            
            # Add chunks to collection
            if not self.collection:
                self.initialize_collection()

            all_json_data = []

            if model == "claude":
                api_key = os.getenv("ANTHROPIC_API_KEY")
                model = AnthropicModel(model_name, api_key=api_key)
                agent = Agent(
                    model, 
                    result_type=Summary, 
                    result_retries=15, 
                )
            elif model == "ollama":
                chunk_size = 1900
                overlap = 200
                api_key = os.getenv("ANTHROPIC_API_KEY")
                model = OllamaModel(model_name)
                agent = Agent(
                    model,
                    result_type=Summary,
                    result_retries=15,
                )
            elif model == "groq":
                api_key = os.getenv("GROQ_API_KEY")
                model = GroqModel(model_name, api_key=api_key)
                agent = Agent(
                    model, 
                    result_type=Summary, 
                    result_retries=15, 
                )
            else:
                raise ValueError(f"Invalid model: {model}")
            # Split transcript into chunks
            chunks = [transcript[i:i+chunk_size] for i in range(0, len(transcript), chunk_size-overlap)]
            
            for i, chunk in enumerate(chunks):
                logger.info(f"Adding chunk {i} to collection {chunk[:20]}......")
                # Run the summary with proper tool response handling
                

                summary = await agent.run(
                    f"""Given is a meeting transcript {chunk}. If no data made, just return [] for the field. example - if no action item in chunk, simply respond with ActionItems: block []
                    Each block content shall be very descriptive. It should give context
                    Please give json out and don't add anything else like </function> or final_result> or anything within tags. Just plain json data""",
                )
                
                # Handle the summary result
                if hasattr(summary, 'data'):
                    logger.info(f",.,AAA,.,.Successfully generated summary for process {summary.data}, {type(summary.data)}")
                    pretty_print_json(summary.data)
                    final_summary = summary.data
                else:
                    logger.info(f",.,.,.,.,.,.Successfully generated summary for process {summary}")
                    pretty_print_json(summary)
                    final_summary = summary
                
                # Convert the final_summary which is in <class '__main__.Summary'> to json
                total_summary_in_pydantic = final_summary



                # Validate summary has content
                # if not any([
                #     total_summary_in_pydantic.SummaryResponse.blocks
                # ]):
                #     raise ValueError("No content found in summary")
                
                total_summary_in_json = json.dumps(total_summary_in_pydantic.dict(), indent=2)
                logger.info(f"JSON Successfully generated summary for process {total_summary_in_json}")


                all_json_data.append(total_summary_in_json)
                
                
                self.collection.add(
                    documents=[chunk],
                    metadatas=[
                        {
                            "source": f"chunk_{i}", 
                            "processed": False,
                            "type": "transcript",
                            "summary": total_summary_in_json
                            }
                        ],
                    ids=[f"id_{i}"]
                )
                
            
            logger.info(f"Added {len(chunks)} chunks to collection")
            return len(chunks), all_json_data
            
        except Exception as e:
            logger.error(f"Error processing transcript: {str(e)}", exc_info=True)
            raise

class MeetingSummarizer:
    """Handles the meeting summarization using AI models"""
    def __init__(self, api_key: str):
        self.model = AnthropicModel('claude-3-5-sonnet-latest', api_key=api_key)
        self.Agenda = Section(title="Agenda", blocks=[])
        self.Decisions = Section(title="Decisions", blocks=[])
        self.ActionItems = Section(title="Action Items", blocks=[])
        self.ClosingRemarks = Section(title="Closing Remarks", blocks=[])
        
    def create_block(self, title: str, content: str, block_type: str = "item", color: str = "default") -> Block:
        """Create a new block with a unique ID"""
        return Block(
            id=str(uuid.uuid4()),
            type=block_type,
            content=content,
            color=color
        )
        
    def add_action_item(self, ctx: RunContext, title: str, content: str):
        """Add an action item to the summary"""
        block = self.create_block(title, content, "action")
        self.ActionItems.blocks.append(block)
        return f"Successfully added action item: {block.title}"
        
    def add_agenda_item(self, ctx: RunContext, title: str, content: str):
        """Add an agenda item to the summary"""
        block = self.create_block(title, content, "agenda")
        self.Agenda.blocks.append(block)
        return f"Successfully added agenda item: {block.title}"
        
    def add_decision(self, ctx: RunContext, title: str, content: str):
        """Add a decision to the summary"""
        block = self.create_block(title, content, "decision")
        self.Decisions.blocks.append(block)
        return f"Successfully added decision: {block.title}"
        
    def generate_summary(self, ctx: RunContext) -> Summary:
        """Generate the final summary response"""
        return Summary(
            MeetingName="",
            SectionSummary={
                "Agenda": self.Agenda.blocks,
                "Decisions": self.Decisions.blocks,
                "ActionItems": self.ActionItems.blocks,
                "ClosingRemarks": self.ClosingRemarks.blocks
            },
            CriticalDeadlines=self.Decisions,
            KeyItemsDecisions=self.ActionItems,
            ImmediateActionItems=self.ActionItems,
            NextSteps=self.ActionItems,
            OtherImportantPoints=self.ActionItems,
            ClosingRemarks=self.ClosingRemarks
        )

SYSTEM_PROMPT = """You are a meeting summarizer agent. Your task is to:

1. EXTRACT INFORMATION
- Use query_transcript to get information about the meeting
- Ask one question at a time and wait for the response
- Process each response completely before making the next tool call
- IMPORTANT: Make only ONE tool call at a time and wait for its response
- If query_transcript returns "CHROMADB_EMPTY", proceed to finalization

2. ORGANIZE INFORMATION
After gathering information, organize it into:
- Agenda items (use add_agenda_item ONE at a time)
- Key decisions made (use add_decision ONE at a time)
- Action items assigned (use add_action_item ONE at a time)
- Any other important points

3. SAVE AND FINALIZE
- Use tools sequentially, waiting for each response
- Once all information is processed, call delete_processed_chunks
- Finally call get_final_summary

Available tools:
- query_transcript
- add_action_item
- add_agenda_item
- add_decision
- save_final_summary_result
- get_final_summary
- delete_processed_chunks

The transcript is stored in ChromaDB - use query_transcript to access it.
Remember to make only ONE tool call at a time and wait for its response.
If you get CHROMADB_EMPTY: All chunks have been processed,
please save the summary to a file and end the process by calling final_result.

Do not run after CHROMADB_EMPTY is received.
"""

# Initialize components
summarizer = MeetingSummarizer(api_key=os.getenv("ANTHROPIC_API_KEY"))
processor = TranscriptProcessor()

# Create an agent first
agent = Agent(
    summarizer.model, 
    result_type=Summary, 
    result_retries=15, 
    system_prompt=SYSTEM_PROMPT
)

# Define tools
@agent.tool
async def query_transcript(ctx: RunContext, query: str) -> str:
    """Query the transcript to extract information. Returns the content and chunk IDs for deletion."""
    try:
        # Check if there are any chunks left
        collection_data = processor.collection.get()
        if not collection_data['ids']:
            return "CHROMADB_EMPTY: All chunks have been processed."
            
        # Get unprocessed chunks
        results = processor.collection.query(
            query_texts=[query],
            n_results=1
        )
        
        if not results or not results['documents'] or not results['documents'][0]:
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
                processor.collection.delete(ids=chunk_ids)
                logger.info(f"Deleted {len(chunk_ids)} processed chunks")
                
                # Verify deletion
                remaining = processor.collection.get()
                logger.info(f"Remaining chunks: {len(remaining['ids'])}")
                
            except Exception as e:
                logger.error(f"Error deleting chunks: {e}")
                return f"Error deleting chunks: {str(e)}"
            
        return combined_result.strip()
        
    except Exception as e:
        logger.error(f"Error querying transcript: {e}")
        return f"Error: {str(e)}"

@agent.tool
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

@agent.tool
async def add_action_item(ctx: RunContext, title: str, content: str) -> str:
    """Add an action item to the summary"""
    result = summarizer.add_action_item(ctx, title, content)
    return f"Successfully added action item: {result}"

@agent.tool
async def add_agenda_item(ctx: RunContext, title: str, content: str) -> str:
    """Add an agenda item to the summary"""
    result = summarizer.add_agenda_item(ctx, title, content)
    return f"Successfully added agenda item: {result}"

@agent.tool
async def add_decision(ctx: RunContext, title: str, content: str) -> str:
    """Add a decision to the summary"""
    result = summarizer.add_decision(ctx, title, content)
    return f"Successfully added decision: {result}"

@agent.tool
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
        summary = summarizer.generate_summary(ctx)
        
        # Validate summary has content
        if not any([
            len(summary.SectionSummary["Agenda"]),
            len(summary.SectionSummary["Decisions"]),
            len(summary.SectionSummary["ActionItems"]),
            len(summary.SectionSummary["ClosingRemarks"]),
            len(summary.CriticalDeadlines),
            len(summary.KeyItemsDecisions),
            len(summary.ImmediateActionItems),
            len(summary.NextSteps),
            len(summary.OtherImportantPoints)
        ]):
            return "Error: No content found in summary. Please add some items first."

        # Convert to JSON using Pydantic's json() method which handles nested models
        json_data = summary.model_dump_json(indent=2)
        
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

@agent.tool
async def get_final_summary(ctx: RunContext) -> Summary:
    """Get the final meeting summary result"""
    return summarizer.generate_summary(ctx)

# Update agent with tools after they are defined
agent.tools = [
    query_transcript,
    add_action_item,
    add_agenda_item,
    add_decision,
    save_final_summary_result,
    get_final_summary,
    delete_processed_chunks
]

logger.info("Initialized QA Agent")

def pretty_print_json(obj):
    """Utility function to pretty print JSON objects"""
    if hasattr(obj, 'model_dump_json'):
        print(obj.model_dump_json(indent=2))
    else:
        print(json.dumps(obj, indent=2))

# Example usage
if __name__ == "__main__":
    try:
        # Set up argument parser
        parser = argparse.ArgumentParser(description='Process a meeting transcript using AI.')
        parser.add_argument('--transcript_path', type=str, help='Path to the transcript file')
        parser.add_argument('--model', type=str, default='claude', choices=['groq', 'claude', 'ollama'], 
                          help='Model to use for processing (default: claude)')
        parser.add_argument('--model-name', type=str, default='claude-3-5-sonnet-latest', 
                          help='Name of the model to use for processing (default: claude-3-5-sonnet-latest)')
        parser.add_argument('--chunk-size', type=int, default=5000, 
                          help='Size of the chunks to be used for processing (default: 5000)')
        parser.add_argument('--overlap', type=int, default=1000, 
                          help='Overlap between the chunks to be used for processing (default: 1000)')
        args = parser.parse_args()

        # Validate transcript path
        if not os.path.exists(args.transcript_path):
            raise ValueError(f"File not found: {args.transcript_path}")
        elif not os.path.isfile(args.transcript_path):
            raise ValueError(f"Path is not a file: {args.transcript_path}")
        else:
            logger.info(f"File exists and is a file: {args.transcript_path}")

        # Set up async loop
        import asyncio
        loop = asyncio.get_event_loop()
        
        # Process transcript
        num_chunks, all_json = loop.run_until_complete(
            processor.process_transcript(model=args.model, model_name=args.model_name, transcript_path=args.transcript_path, chunk_size=args.chunk_size, overlap=args.overlap)
        )
        logger.info(f"Processed transcript into {num_chunks} chunks")
        logger.info(f"Successfully processed transcript into {all_json} chunks, type {type(all_json)}")
        
        # Create a new JSON object for final summary
        final_summary = {
            "MeetingName": "",
            "SectionSummary": {
                "title": "Section Summary",
                "blocks": []
            },
            "CriticalDeadlines" : {
                "title": "Critical Deadlines",
                "blocks": []
            },
            "KeyItemsDecisions" : {
                "title": "Key Items & Decisions",
                "blocks": []
            },
            "ImmediateActionItems" : {
                "title": "Immediate Action Items",
                "blocks": []
            },
            "NextSteps" : {
                "title": "Next Steps",
                "blocks": []
            },
            "OtherImportantPoints" : {
                "title": "Other Important Points",
                "blocks": []
            },
            "ClosingRemarks" : {
                "title": "Closing Remarks",
                "blocks": []
            }
        }
        
        # Save raw JSON
        with open('all_json.json', 'w') as f:
            json.dump(all_json, f, indent=2)
            

        # Combine all JSON objects
        for json_obj in all_json:
            logger.info(f"Processing JSON object {json_obj}, type {type(json_obj)}")
            json_dict = json.loads(json_obj)
            final_summary["MeetingName"] =  json_dict["MeetingName"]
            final_summary["SectionSummary"]["blocks"].extend(json_dict["SectionSummary"]["blocks"])
            final_summary["CriticalDeadlines"]["blocks"].extend(json_dict["CriticalDeadlines"]["blocks"])
            final_summary["KeyItemsDecisions"]["blocks"].extend(json_dict["KeyItemsDecisions"]["blocks"])
            final_summary["ImmediateActionItems"]["blocks"].extend(json_dict["ImmediateActionItems"]["blocks"])
            final_summary["NextSteps"]["blocks"].extend(json_dict["NextSteps"]["blocks"])
            final_summary["OtherImportantPoints"]["blocks"].extend(json_dict["OtherImportantPoints"]["blocks"])
            final_summary["ClosingRemarks"]["blocks"].extend(json_dict["ClosingRemarks"]["blocks"])
            
        logger.info(f"Final summary: {final_summary}")
        
        # Save final summary
        final_summary_str = json.dumps(final_summary, indent=2)
        with open("final_summary.json", "w") as f:
            f.write(final_summary_str)

        logger.info(f"Final summary saved to final_summary.json")
            
    except Exception as e:
        logger.error(f"Error during summarization: {str(e)}")
        processor.cleanup()
        exit(1)