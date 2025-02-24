# Meeting Minutes - Rust Implementation (Experimental)

This is an experimental Rust-based implementation of the Meeting Minutes AI assistant, located in the `/experiment` directory of the main project. It aims to provide better performance and native integration compared to the main implementation.

## Features

- Real-time audio recording from both microphone and system audio
- Live transcription using Whisper ASR (locally running)
- Native desktop integration using Tauri instead of Electron
- Speaker diarization support
- Rich text editor for note-taking
- Privacy-focused: All processing happens locally

## Prerequisites

- Node.js (v18 or later)
- Rust (latest stable)
- pnpm (v8 or later)
- macOS (for system audio capture)
- [Xcode Command Line Tools 16.2](https://developer.apple.com/download/all/?q=xcode) (Released December 11, 2024)


## Project Structure

The main project structure is:
```
/meeting-minutes
├── backend/          # Main Python backend
├── docs/            # Project documentation
├── frontend/        # Main Electron frontend
└── experiment/      # Experimental implementations
    ├── rust_based_implementation/  # This implementation
    │   ├── src/                   # Next.js frontend
    │   ├── src-tauri/             # Rust backend
    │   └── whisper-server-package/ # Local transcription server
    ├── screenpipe/                # Audio processing library
    └── simple_recorder.rs         # Basic audio implementation
```

## Installation

1. Clone the main repository:
   ```bash
   git clone <repository-url>
   cd meeting-minutes/frontend
   ```



2. Install dependencies:
   ```bash
   pnpm install
   ```

## Running the App

Use the provided script to run the app:
```bash
./clean_build.sh
```

This script will:
1. Install dependencies
2. Check for and download the Whisper model if needed
3. Start the Whisper server
4. Launch the Tauri app in development mode

## Implementation Details

This implementation differs from the main project by:
- Using Rust instead of Python for the backend
- Using Tauri instead of Electron for desktop integration
- Running Whisper locally instead of using external APIs
- Implementing real-time audio processing in Rust
- Using the screenpipe library for audio capture

## Development Status

This is an experimental implementation that explores:
- Using Rust for better performance in audio processing
- Native desktop integration with Tauri
- Local transcription with Whisper
- Real-time audio processing capabilities

For the production implementation, please see the main project in the root directory.

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- [Whisper ASR](https://github.com/openai/whisper) for transcription
- [Tauri](https://tauri.app/) for the desktop framework
- [screenpipe-audio](https://github.com/screenpipe/screenpipe-audio) for audio capture
