<div align="center" style="border-bottom: none">
    <h1>
        Privacy-First AI Meeting Assistant
    </h1>
    <a href="https://github.com/bykof/conversationaly/releases/"><img src="https://img.shields.io/badge/Pre_Release-Link-brightgreen" alt="Pre-Release"></a>
    <a href="https://github.com/bykof/conversationaly/releases"><img alt="GitHub Repo stars" src="https://img.shields.io/github/stars/bykof/conversationaly?style=flat"></a>
    <a href="https://github.com/bykof/conversationaly/releases"><img alt="GitHub Downloads (all assets, all releases)" src="https://img.shields.io/github/downloads/bykof/conversationaly/total?style=plastic"></a>
    <a href="https://github.com/bykof/conversationaly/releases"><img src="https://img.shields.io/badge/License-MIT-blue" alt="License"></a>
    <a href="https://github.com/bykof/conversationaly/releases"><img src="https://img.shields.io/badge/Supported_OS-macOS,_Windows-white" alt="Supported OS"></a>
    <a href="https://github.com/bykof/conversationaly/releases"><img alt="GitHub Tag" src="https://img.shields.io/github/v/tag/bykof/conversationaly?include_prereleases&color=yellow"></a>
    <br>
    <h3>
    <br>
    Open Source • Privacy-First • Enterprise-Ready
    </h3>
    <p align="center">

A privacy-first AI meeting assistant that captures, transcribes, and summarizes meetings entirely on your infrastructure. Built by expert AI engineers passionate about data sovereignty and open source solutions. Perfect for enterprises that need advanced meeting intelligence without compromising on privacy, compliance, or control.

</p>

<p align="center">
    <a href="https://youtu.be/6FnhSC_eSz8">View full Demo Video</a>
</p>

</div>

<details>
<summary>Table of Contents</summary>

- [Introduction](#introduction)
- [Why Conversationaly?](#why-conversationaly)
- [Features](#features)
- [Installation](#installation)
- [Key Features in Action](#key-features-in-action)
- [System Architecture](#system-architecture)
- [For Developers](#for-developers)
- [Contributing](#contributing)
- [License](#license)

</details>

## Introduction

Conversationaly is a privacy-first AI meeting assistant that runs entirely on your local machine. It captures your meetings, transcribes them in real-time, and generates summaries, all without sending any data to the cloud. This makes it the perfect solution for professionals and enterprises who need to maintain complete control over their sensitive information.

## Why Conversationaly?

While there are many meeting transcription tools available, this solution stands out by offering:

- **Privacy First:** All processing happens locally on your device.
- **Cost-Effective:** Uses open-source AI models instead of expensive APIs.
- **Flexible:** Works offline and supports multiple meeting platforms.
- **Customizable:** Self-host and modify for your specific needs.

<details>
<summary>The Privacy Problem</summary>

Meeting AI tools create significant privacy and compliance risks across all sectors:

- **$4.4M average cost per data breach** (IBM 2024)
- **€5.88 billion in GDPR fines** issued by 2025
- **400+ unlawful recording cases** filed in California this year

Whether you're a defense consultant, enterprise executive, legal professional, or healthcare provider, your sensitive discussions shouldn't live on servers you don't control. Cloud meeting tools promise convenience but deliver privacy nightmares with unclear data storage practices and potential unauthorized access.

**Conversationaly solves this:** Complete data sovereignty on your infrastructure, zero vendor lock-in, and full control over your sensitive conversations.

</details>

## Features

- **Local First:** All processing is done on your machine. No data ever leaves your computer.
- **Real-time Transcription:** Get a live transcript of your meeting as it happens.
- **AI-Powered Summaries:** Generate summaries of your meetings using powerful language models.
- **Multi-Platform:** Works on macOS, Windows, and Linux.
- **Open Source:** Conversationaly is open source and free to use.
- **Flexible AI Provider Support:** Choose from Ollama (local), Claude, Groq, OpenRouter, or use your own OpenAI-compatible endpoint.

## Installation

### 🪟 **Windows**

1. Download the latest `x64-setup.exe` from [Releases](https://github.com/bykof/conversationaly/releases/latest)
2. Run the installer

### 🍎 **macOS**

1. Download `conversationaly_0.4.0_aarch64.dmg` from [Releases](https://github.com/bykof/conversationaly/releases/latest)
2. Open the downloaded `.dmg` file
3. Drag **Conversationaly** to your Applications folder
4. Open **Conversationaly** from Applications folder

### 🐧 **Linux**

Build from source following our detailed guides:

- [Building on Linux](docs/building_in_linux.md)
- [General Build Instructions](docs/BUILDING.md)

**Quick start:**

```bash
git clone https://github.com/bykof/conversationaly
cd conversationaly/frontend
pnpm install
./build-gpu.sh
```

## Key Features in Action

### 🎯 Local Transcription

Transcribe meetings entirely on your device using **Whisper** or **Parakeet** models. No cloud required.

### 📥 Import & Enhance `Beta`

Import existing audio files to generate transcripts, or enhance to re-transcribe any recorded meeting with a different model or language, all processed locally.

> Contributed by [Jeremi Joslin](https://github.com/jeremi), improved by [Vishnu P S](https://github.com/p-s-vishnu) and [Mohammed Safvan](https://github.com/mohammedsafvan)

### 🤖 AI-Powered Summaries

Generate meeting summaries with your choice of AI provider. **Ollama** (local) is recommended, with support for Claude, Groq, OpenRouter, and OpenAI.

### 🔒 Privacy-First Design

All data stays on your machine. Transcription models, recordings, and transcripts are stored locally.

### 🌐 Custom OpenAI Endpoint Support

Use your own OpenAI-compatible endpoint for AI summaries. Perfect for organizations with custom AI infrastructure or preferred providers.

### 🎙️ Professional Audio Mixing

Capture microphone and system audio simultaneously with intelligent ducking and clipping prevention.

### ⚡ GPU Acceleration

Built-in support for hardware acceleration across platforms:

- **macOS**: Apple Silicon (Metal) + CoreML
- **Windows/Linux**: NVIDIA (CUDA), AMD/Intel (Vulkan)

Automatically enabled at build time - no configuration needed.

## System Architecture

Conversationaly is a single, self-contained application built with [Tauri](https://tauri.app/). It uses a Rust-based backend to handle all the core logic, and a Next.js frontend for the user interface.

For more details, see the [Architecture documentation](docs/architecture.md).

## For Developers

If you want to contribute to Conversationaly or build it from source, you'll need to have Rust and Node.js installed. For detailed build instructions, please see the [Building from Source guide](docs/BUILDING.md).

## Contributing

We welcome contributions from the community! If you have any questions or suggestions, please open an issue or submit a pull request. Please follow the established project structure and guidelines. For more details, refer to the [CONTRIBUTING.md](CONTRIBUTING.md) file.

Thanks for all the contributions. Our community is what makes this project possible.

## License

MIT License - Feel free to use this project for your own purposes.

## Acknowledgments

- Conversationaly is a fork of [Meetily](https://github.com/Zackriya-Solutions/meeting-minutes) by Zackriya Solutions, which it builds on under the MIT license.
- We borrowed some code from [Whisper.cpp](https://github.com/ggerganov/whisper.cpp).
- We borrowed some code from [Screenpipe](https://github.com/mediar-ai/screenpipe).
- We borrowed some code from [transcribe-rs](https://crates.io/crates/transcribe-rs).
- Thanks to **NVIDIA** for developing the **Parakeet** model.
- Thanks to [istupakov](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx) for providing the **ONNX conversion** of the Parakeet model.

## Star History

[![Star History Chart](https://api.star-history.com/chart?repos=bykof/conversationaly&type=date&legend=top-left)](https://www.star-history.com/?repos=bykof%2Fconversationaly&type=date&legend=bottom-right)
