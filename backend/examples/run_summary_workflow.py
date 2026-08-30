import requests
import time
import argparse
import json
import sys
import uuid # Import uuid to generate unique IDs
import logging

# --- Configuration ---
DEFAULT_BASE_URL = "http://localhost:5167"
DEFAULT_MODEL_PROVIDER = "openai"  # Or 'ollama', 'groq', 'openai' etc.
DEFAULT_MODEL_NAME = "gpt-4o-2024-11-20" # Adjust if needed (example)
DEFAULT_CHUNK_SIZE = 40000
DEFAULT_OVERLAP = 1000
DEFAULT_POLL_INTERVAL_SECONDS = 5  # How often to check the status
DEFAULT_MAX_POLL_ATTEMPTS = 24     # Max times to poll (e.g., 24 * 5s = 120s timeout)

# Configure basic logging
logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

# --- API Interaction Functions ---

def process_transcript(base_url, transcript_text, provider, model_name, chunk_size, overlap, meeting_id):
    """Sends the transcript to the processing endpoint."""
    url = f"{base_url}/process-transcript"
    payload = {
        "text": transcript_text,
        "model": provider,
        "model_name": model_name,
        "meeting_id": meeting_id, # *** ADDED meeting_id ***
        "chunk_size": chunk_size,
        "overlap": overlap
    }
    headers = {'Content-Type': 'application/json'}
    logger.info(f"Sending POST request to {url} with model '{provider}/{model_name}' and meeting_id '{meeting_id}'...")
    logger.debug(f"Payload: {json.dumps(payload, indent=2)}") # Log payload for debugging if needed

    try:
        response = requests.post(url, headers=headers, json=payload, timeout=30) # 30s timeout for initial request
        logger.info(f"POST Response Status Code: {response.status_code}")
        response.raise_for_status() # Raise an exception for bad status codes (4xx or 5xx)

        response_data = response.json()
        if "process_id" in response_data:
            # IMPORTANT: The backend returns 'process_id', which *is* the meeting_id we need for polling.
            returned_process_id = response_data['process_id']
            logger.info(f"Successfully initiated processing. Process ID received: {returned_process_id}")
            # Optional: Verify if returned_process_id matches the meeting_id sent
            if returned_process_id != meeting_id:
                 logger.warning(f"Returned process_id '{returned_process_id}' differs from generated meeting_id '{meeting_id}'. Using returned ID for polling.")
            return returned_process_id # Return the ID provided by the backend
        else:
            logger.error(f"'process_id' not found in response: {response_data}")
            return None

    except requests.exceptions.Timeout:
        logger.error(f"Error: Request to {url} timed out.")
        return None
    except requests.exceptions.RequestException as e:
        logger.error(f"Error during transcript processing request: {e}")
        if e.response is not None:
             logger.error(f"Response status: {e.response.status_code}, Response text: {e.response.text}")
        return None
    except json.JSONDecodeError:
        logger.error(f"Could not decode JSON response from {url}. Response text: {response.text}")
        return None

def poll_summary_status(base_url, meeting_id_for_polling, interval, max_attempts):
    """Polls the summary status endpoint until completion or error, using meeting_id."""
    # *** UPDATED endpoint path ***
    url = f"{base_url}/get-summary/{meeting_id_for_polling}"
    logger.info(f"Polling status endpoint: {url} (every {interval}s) for meeting_id '{meeting_id_for_polling}'")

    for attempt in range(max_attempts):
        logger.info(f"Polling attempt {attempt + 1}/{max_attempts}...")
        try:
            response = requests.get(url, timeout=20) # 20s timeout for polling request
            logger.info(f"GET Response Status Code: {response.status_code}")

            # Check for non-blocking statuses first (202 indicates processing)
            if response.status_code == 202:
                status_data = response.json()
                status = status_data.get("status", "processing").lower() # Assume processing if status missing
                logger.info(f"  Status: {status} (via 202 Accepted)")
                time.sleep(interval)
                continue # Go to next poll attempt

            response.raise_for_status() # Raise exception for other bad statuses (4xx, 5xx)

            # --- *** UPDATED Response Parsing Logic *** ---
            status_data = response.json()
            status = status_data.get("status", "unknown").lower()
            error_message = status_data.get("error")
            summary_data = status_data.get("data") # The actual summary is nested in 'data'
            meeting_name = status_data.get("meetingName")

            logger.info(f"  Status: {status}")
            if meeting_name:
                logger.info(f"  Meeting Name: {meeting_name}")

            if status == "completed":
                logger.info("Processing completed successfully!")
                if summary_data:
                     return summary_data
                else:
                     logger.error("Status is 'completed' but 'data' field is missing or empty in the response.")
                     return None
            elif status == "error" or status == "failed": # Check for both 'error' and 'failed' status
                logger.error(f"Error reported by backend: {error_message or 'Unknown error'}")
                return None
            elif status in ["processing", "pending", "started"]: # Backend might use these
                # Wait before the next poll (already handled by 202 check, but keep for robustness)
                time.sleep(interval)
            else:
                logger.warning(f"Received unknown status '{status}'. Response: {status_data}. Continuing to poll.")
                time.sleep(interval)


        except requests.exceptions.Timeout:
            logger.warning(f"Polling request timed out. Retrying...")
            time.sleep(interval) # Wait before retrying after timeout
        except requests.exceptions.RequestException as e:
            logger.error(f"Error during polling request: {e}. Stopping polling.")
            if e.response is not None:
                logger.error(f"Response status: {e.response.status_code}, Response text: {e.response.text}")
                # Handle 404 specifically - means meeting ID wasn't found (maybe typo or processing failed early)
                if e.response.status_code == 404:
                    logger.error(f"Meeting ID '{meeting_id_for_polling}' not found on server. Ensure processing started correctly.")
            return None
        except json.JSONDecodeError:
            logger.error(f"Could not decode JSON response from {url}. Response text: {response.text}")
            logger.error("Stopping polling.")
            return None

    logger.error(f"Reached maximum polling attempts ({max_attempts}) without completion.")
    return None

# --- Main Execution ---

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Test the transcript summarization API workflow.")
    parser.add_argument("transcript_file", help="Path to the .txt transcript file.")
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL, help=f"Base URL of the API (default: {DEFAULT_BASE_URL})")
    parser.add_argument("--provider", default=DEFAULT_MODEL_PROVIDER, help=f"Model provider (default: {DEFAULT_MODEL_PROVIDER})")
    parser.add_argument("--model-name", default=DEFAULT_MODEL_NAME, help=f"Specific model name (default: {DEFAULT_MODEL_NAME})")
    parser.add_argument("--interval", type=int, default=DEFAULT_POLL_INTERVAL_SECONDS, help=f"Polling interval in seconds (default: {DEFAULT_POLL_INTERVAL_SECONDS})")
    parser.add_argument("--attempts", type=int, default=DEFAULT_MAX_POLL_ATTEMPTS, help=f"Maximum polling attempts (default: {DEFAULT_MAX_POLL_ATTEMPTS})")
    parser.add_argument("--chunk-size", type=int, default=DEFAULT_CHUNK_SIZE, help=f"Chunk size for processing (default: {DEFAULT_CHUNK_SIZE})")
    parser.add_argument("--overlap", type=int, default=DEFAULT_OVERLAP, help=f"Overlap size for processing (default: {DEFAULT_OVERLAP})")
    # Optional: Add argument to provide meeting_id if needed, otherwise generate one
    # parser.add_argument("--meeting-id", help="Optional: Specify a meeting ID to use.")


    args = parser.parse_args()

    # 1. Read transcript file
    try:
        with open(args.transcript_file, 'r', encoding='utf-8') as f:
            transcript_content = f.read()
        logger.info(f"Successfully read transcript file: {args.transcript_file}")
        if not transcript_content.strip():
             logger.error("Transcript file is empty.")
             sys.exit(1)
    except FileNotFoundError:
        logger.error(f"Transcript file not found at '{args.transcript_file}'")
        sys.exit(1)
    except Exception as e:
        logger.error(f"Error reading transcript file: {e}")
        sys.exit(1)

    # *** Generate a unique meeting ID for this run ***
    # meeting_id = args.meeting_id if args.meeting_id else f"test-meeting-{uuid.uuid4()}"
    meeting_id = f"test-meeting-{uuid.uuid4()}" # Generate unique ID
    logger.info(f"Generated Meeting ID for this run: {meeting_id}")

    # 2. Process Transcript (POST request)
    # Pass the generated meeting_id
    process_id_from_api = process_transcript(
        args.base_url,
        transcript_content,
        args.provider,
        args.model_name,
        args.chunk_size,
        args.overlap,
        meeting_id # Pass the generated ID
    )

    if not process_id_from_api:
        logger.error("Failed to initiate transcript processing. Exiting.")
        sys.exit(1)

    # 3. Poll for Summary (GET requests)
    # Use the process_id returned by the API (which is the meeting_id) for polling
    summary_result = poll_summary_status(
        args.base_url,
        process_id_from_api, # Use the ID received from the /process-transcript response
        args.interval,
        args.attempts
    )

    # 4. Display Result
    if summary_result:
        logger.info("\\n--- Summary Received ---")
        # Pretty print the JSON result
        print(json.dumps(summary_result, indent=2))
        logger.info("------------------------")
    else:
        logger.error("\\nFailed to retrieve summary.")
        sys.exit(1)

    logger.info("Script finished.")
