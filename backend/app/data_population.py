import asyncio
import json
from datetime import datetime
import uuid
import logging
from db import DatabaseManager

logger = logging.getLogger(__name__)
logging.basicConfig(level=logging.INFO)

async def populate_fake_data():
    db = DatabaseManager()

    # Create fake meetings
    logger.info("Creating fake meetings...")
    meeting1 = await db.create_meeting(
        title="Weekly Sync",
        date=datetime.utcnow().strftime("%Y-%m-%d"),
        time="10:00",
        attendees=["alice@example.com", "bob@example.com"],
        tags=["sync", "team"],
        content="Discussion about weekly progress."
    )
    meeting2 = await db.create_meeting(
        title="Project Kickoff",
        date=datetime.utcnow().strftime("%Y-%m-%d"),
        time="09:00",
        attendees=["charlie@example.com", "dave@example.com"],
        tags=["kickoff", "project"],
        content="Initial project kickoff meeting."
    )
    logger.info(f"Created meetings: {meeting1}, {meeting2}")

    # Create fake summary processes
    logger.info("Creating fake summary processes...")
    process1 = await db.create_process()
    # Simulate a process that completed successfully
    await db.update_process(
        process_id=process1,
        status="COMPLETED",
        result={"summary": "Weekly Sync was productive."},
        chunk_count=1,
        processing_time=2.5,
        metadata={"meeting_id": meeting1}
    )

    process2 = await db.create_process()
    # Simulate a process that failed
    await db.update_process(
        process_id=process2,
        status="FAILED",
        error="Network timeout during processing.",
        metadata={"meeting_id": meeting2}
    )
    logger.info(f"Created processes: {process1}, {process2}")

    # Create fake transcript for process1
    transcript_text = (
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. "
        "Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua."
    )
    await db.save_transcript(
        process_id=process1,
        transcript_text=transcript_text,
        model="fake-model",
        model_name="Fake Model",
        chunk_size=512,
        overlap=64
    )
    # Optionally update the transcript with the meeting name
    await db.update_meeting_name(process1, "Weekly Sync")
    logger.info(f"Saved transcript for process: {process1}")

if __name__ == '__main__':
    asyncio.run(populate_fake_data())