# AI-Powered Meeting Assistant Architecture

## High-Level Architecture Diagram

![High Level Architecture](Diagram-High%20level%20architecture%20diagram.jpg)

```mermaid
graph TD
    subgraph Frontend[Frontend - Electron JS + Next JS]
        F[Provides user interface,<br/>sends/receives data, and<br/>handles live updates.]
    end

    subgraph OS[OS + Virtual Audio Driver]
        V[Captures audio streams from<br/>microphone and meeting app.]
    end

    subgraph Backend[Backend - FastAPI]
        B[Manages transcription<br/>requests, interacts with AI<br/>engines, and handles<br/>database storage.]
    end

    subgraph AI[AI Engine]
        AI1[Whisper + Qwen/Llama 3.2<br/>Performs transcription and<br/>summarization tasks.]
    end

    subgraph Storage[Storage]
        DB[(Local Database<br/>SQLite<br/>Stores transcripts,<br/>summaries, and<br/>metadata securely.)]
        VDB[(Knowledge Graph/VectorDB<br/>Indexes transcripts<br/>and summaries for<br/>semantic search<br/>and query.)]
    end

    subgraph APIs[APIs - Ollama with Agentic Tools]
        API[Extends AI functionality with<br/>external models and tools.<br/>We'll try to use small LLMs]
    end

    OS --> Frontend
    Frontend <--> Backend
    Backend --> AI
    Backend --> DB
    Backend --> VDB
    AI --> Backend
    API --> VDB
    API --> Backend
```

## Component Details

### 1. Frontend (Electron JS + Next JS)
- Provides responsive user interface
- Handles real-time updates and data display
- Manages user interactions and controls
- Communicates with backend via WebSocket and REST APIs

### 2. OS + Virtual Audio Driver
- Integrates with system audio
- Captures audio streams from various sources
- Routes audio data to the application
- Supports multiple platforms (macOS, Windows, Linux)

### 3. Backend (FastAPI)
- Manages audio processing pipeline
- Handles WebSocket connections for real-time updates
- Coordinates AI processing tasks
- Manages database operations
- Provides REST API endpoints

### 4. AI Engine
- Uses Whisper for accurate speech-to-text
- Employs Qwen/Llama 3.2 for summarization
- Processes audio in real-time
- Generates meeting insights

### 5. Storage
#### Local Database (SQLite)
- Stores meeting transcripts
- Maintains summaries and metadata
- Ensures data privacy and security
- Provides efficient data retrieval

#### Knowledge Graph/VectorDB
- Indexes meeting content
- Enables semantic search
- Supports natural language queries
- Maintains relationships between meeting data

### 6. APIs (Ollama with Agentic Tools)
- Extends AI capabilities
- Integrates external models
- Provides additional processing tools
- Uses smaller, efficient LLMs
