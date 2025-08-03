# Meetily - Frontend

A modern desktop application for recording, transcribing, and analyzing meetings with AI assistance. Built with Next.js and Tauri for a native desktop experience.

## Features

- Real-time audio recording from both microphone and system audio
- Live transcription using Whisper ASR (locally running)
- Native desktop integration using Tauri
- Speaker diarization support
- Rich text editor for note-taking
- Privacy-focused: All processing happens locally

## Prerequisites

### For macOS:
- Node.js (v18 or later)
- Rust (latest stable)
- pnpm (v8 or later)
- [Xcode Command Line Tools](https://developer.apple.com/download/all/?q=xcode)

### For Windows:
- Node.js (v18 or later)
- Rust (latest stable)
- pnpm (v8 or later)
- Visual Studio Build Tools with C++ development tools
- Windows 10 or later

### For Linux:
- Node.js (v18 or later)
- Rust (latest stable)
- pnpm (v8 or later)
- **Debian/Ubuntu**: `sudo apt install -y libwebkit2gtk-4.1-dev libasound2-dev librsvg2-dev`
- **Fedora**: `sudo dnf install -y webkit2gtk4.1-devel alsa-lib-devel librsvg2-devel`
- **Arch**: `sudo pacman -S --needed --noconfirm webkit2gtk-4.1 alsa-lib librsvg`

## Project Structure

```
/frontend
├── src/                   # Next.js frontend code
├── src-tauri/             # Rust backend for Tauri
├── whisper-server-package/ # Local transcription server
│   ├── models/            # Whisper models
│   ├── whisper-server     # Pre-built server binary
│   └── run-server.sh      # Script to start the server
├── public/                # Static assets
└── package.json           # Project dependencies
```

## Installation

### For macOS:

1. Install prerequisites:
   ```bash
   # Install Homebrew if not already installed
   /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
   
   # Install Node.js
   brew install node
   
   # Install Rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   
   # Install pnpm
   npm install -g pnpm
   
   # Install Xcode Command Line Tools
   xcode-select --install
   ```

2. Clone the repository and navigate to the frontend directory:
   ```bash
   git clone https://github.com/Zackriya-Solutions/meeting-minutes
   cd meeting-minutes/frontend
   ```
  

3. Install dependencies:
   ```bash
   pnpm install
   ```

### For Windows:

1. Install prerequisites:
   - Install [Node.js](https://nodejs.org/) (v18 or later)
   - Install [Rust](https://www.rust-lang.org/tools/install)
   - Install pnpm: `npm install -g pnpm`
   - Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with C++ development tools

2. Clone the repository and navigate to the frontend directory:
   ```cmd
   git clone https://github.com/Zackriya-Solutions/meeting-minutes
   cd meeting-minutes/frontend
   ```

3. Install dependencies:
   ```cmd
   pnpm install
   ```

### For Linux:

1. Install prerequisites:
   ```bash
   # Install Node.js (v18 or later) using nvm (recommended)
   curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
   # Follow instructions to add nvm to your shell's startup file, then restart your terminal or source the file (e.g., source ~/.bashrc)
   nvm install 18
   nvm use 18
   
   # Install Rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   # Follow instructions to add cargo to your PATH
   
   # Install pnpm
   npm install -g pnpm
   ```

2. Clone the repository and navigate to the frontend directory:
   ```bash
   git clone https://github.com/Zackriya-Solutions/meeting-minutes
   cd meeting-minutes/frontend
   ```

3. Install dependencies:
   ```bash
   pnpm install
   ```

## Running the App

### For macOS:

Use the provided script to run the app in development mode:
```bash
./clean_run.sh
```

To build a production version:
```bash
./clean_build.sh
```

You can specify the log level (info, debug, trace):
```bash
./clean_run.sh debug
```

### For Windows:

Use the provided script to run the app in development mode:
```cmd
clean_run_windows.bat
```

To build a production version:
```cmd
clean_build_windows.bat
```

### For Linux:

Use the provided script to run the app in development mode:
```bash
./clean_run.sh
```

To build a production version:
```bash
./clean_build.sh
```

You can specify the log level (info, debug, trace):
```bash
./clean_run.sh debug
```

## Whisper Transcription Server

The application includes a pre-built Whisper server for real-time speech recognition:

- Located in `whisper-server-package/`
- Supports speaker diarization
- Runs locally for privacy
- Uses Metal acceleration on macOS

To run the Whisper server manually:
```bash
cd whisper-server-package
./run-server.sh
```

The server will be available at http://localhost:8178

## Development

### Frontend (Next.js)
- The frontend is built with Next.js and Tailwind CSS
- Source code is in the `src/` directory
- To run only the frontend: `pnpm run dev`

### Backend (Tauri)
- The Rust backend is in the `src-tauri/` directory
- Handles audio capture, file system access, and native integrations
- To run only the Tauri development server: `pnpm run tauri dev`

## Troubleshooting

### Common Issues on macOS
- If you encounter permission issues with scripts, make them executable:
  ```bash
  chmod +x clean_run.sh clean_build.sh whisper-server-package/run-server.sh
  ```
- For microphone access issues, ensure the app has microphone permissions in System Preferences
- If the Whisper server fails to start, check if port 8178 is already in use

### Common Issues on Windows
- If you encounter build errors, ensure Visual Studio Build Tools are properly installed
- For audio capture issues, check Windows privacy settings for microphone access
- If the app fails to start, try running Command Prompt as administrator

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the LICENSE file for details.
