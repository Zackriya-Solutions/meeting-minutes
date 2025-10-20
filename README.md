<div align="center" style="border-bottom: none">
    <h1>
        <img src="docs/Meetily-6.png" style="border-radius: 10px;" />
        <br>
        Privacy-First AI Meeting Assistant
    </h1>
    <a href="https://trendshift.io/repositories/13272" target="_blank"><img src="https://trendshift.io/api/badge/repositories/13272" alt="Zackriya-Solutions%2Fmeeting-minutes | Trendshift" style="width: 250px; height: 55px;" width="250" height="55"/></a>
    <br>
    <br>
    <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases/"><img src="https://img.shields.io/badge/Pre_Release-Link-brightgreen" alt="Pre-Release"></a>
    <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases"><img alt="GitHub Repo stars" src="https://img.shields.io/github/stars/zackriya-solutions/meeting-minutes?style=flat">
</a>
 <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases"> <img alt="GitHub Downloads (all assets, all releases)" src="https://img.shields.io/github/downloads/zackriya-solutions/meeting-minutes/total?style=plastic"> </a>
    <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases"><img src="https://img.shields.io/badge/License-MIT-blue" alt="License"></a>
    <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases"><img src="https://img.shields.io/badge/Supported_OS-macOS,_Windows-white" alt="Supported OS"></a>
    <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases"><img alt="GitHub Tag" src="https://img.shields.io/github/v/tag/zackriya-solutions/meeting-minutes?include_prereleases&color=yellow">
</a>
    <br>
    <h3>
    <br>
    Open Source • Privacy-First • Enterprise-Ready
    </h3>
    <p align="center">
    Get latest <a href="https://www.zackriya.com/meetily-subscribe/"><b>Product updates</b></a> <br><br>
    <a href="https://meetily.zackriya.com"><b>Website</b></a> •
    <a href="https://www.linkedin.com/company/106363062/"><b>LinkedIn</b></a> •
    <a href="https://discord.gg/crRymMQBFH"><b>Meetily Discord</b></a> •
    <a href="https://discord.com/invite/vCFJvN4BwJ"><b>Privacy-First AI</b></a> •
    <a href="https://www.reddit.com/r/meetily/"><b>Reddit</b></a>
</p>
    <p align="center">
    
 A privacy-first AI meeting assistant that captures, transcribes, and summarizes meetings entirely on your infrastructure. Built by expert AI engineers passionate about data sovereignty and open source solutions. Perfect for enterprises that need advanced meeting intelligence without compromising on privacy, compliance, or control. 
</p>

<p align="center">
    <img src="docs/demo_small.gif" width="650" alt="Meetily Demo" />
    <br>
    <a href="https://youtu.be/5k_Q5Wlahuk">View full Demo Video</a>
</p>

</div>

<details>
<summary>Table of Contents</summary>

- [Introduction](#introduction)
- [Why Meetily?](#why-meetily)
- [Features](#features)
- [Installation](#installation)
- [Key Features in Action](#key-features-in-action)
- [System Architecture](#system-architecture)
- [For Developers](#for-developers)
- [Enterprise Solutions](#enterprise-solutions)
- [Contributing](#contributing)
- [License](#license)

</details>

## Introduction

Meetily is a privacy-first AI meeting assistant that runs entirely on your local machine. It captures your meetings, transcribes them in real-time, and generates summaries, all without sending any data to the cloud. This makes it the perfect solution for professionals and enterprises who need to maintain complete control over their sensitive information.

## Why Meetily?

While there are many meeting transcription tools available, this solution stands out by offering:

-   **Privacy First:** All processing happens locally on your device.
-   **Cost-Effective:** Uses open-source AI models instead of expensive APIs.
-   **Flexible:** Works offline and supports multiple meeting platforms.
-   **Customizable:** Self-host and modify for your specific needs.

<details>
<summary>The Privacy Problem</summary>

Meeting AI tools create significant privacy and compliance risks across all sectors:

-   **$4.4M average cost per data breach** (IBM 2024)
-   **€5.88 billion in GDPR fines** issued by 2025
-   **400+ unlawful recording cases** filed in California this year

Whether you're a defense consultant, enterprise executive, legal professional, or healthcare provider, your sensitive discussions shouldn't live on servers you don't control. Cloud meeting tools promise convenience but deliver privacy nightmares with unclear data storage practices and potential unauthorized access.

**Meetily solves this:** Complete data sovereignty on your infrastructure, zero vendor lock-in, and full control over your sensitive conversations.

</details>

## Features

-   **Local First:** All processing is done on your machine. No data ever leaves your computer.
-   **Real-time Transcription:** Get a live transcript of your meeting as it happens.
-   **AI-Powered Summaries:** Generate summaries of your meetings using powerful language models.
-   **Multi-Platform:** Works on macOS, Windows, and Linux.
-   **Open Source:** Meetily is open source and free to use.

## Installation

The easiest way to get started with Meetily is to download the latest release for your operating system.

-   **macOS:** [Download the latest `.dmg` file](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest)
-   **Windows:** [Download the latest `.exe` installer](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest)
-   **Linux:** [Download the latest `.AppImage` file](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest)

[Placeholder: Screenshot of the installer]

## Key Features in Action

### 🎯 Local Transcription

Transcribe meetings entirely on your device using **Whisper** or **Parakeet** models. No cloud required.
<p align="center">
    <img src="docs/transcription.png" width="650" style="border-radius: 10px;" alt="Meetily Demo" />
</p>

### 🤖 AI-Powered Summaries

Generate meeting summaries with your choice of AI provider. **Ollama** (local) is recommended, with support for Claude, Groq, OpenRouter, and OpenAI.


<p align="center">
    <img src="docs/summary.png" width="650" style="border-radius: 10px;" alt="Summary generation" />
</p>

### 🔒 Privacy-First Design

All data stays on your machine. Transcription models, recordings, and transcripts are stored locally.

<p align="center">
    <img src="docs/local.png" width="650" style="border-radius: 10px;" alt="Local Transcription and storage" />
</p>

### 🎙️ Professional Audio Mixing

Capture microphone and system audio simultaneously with intelligent ducking and clipping prevention.

<p align="center">
    <img src="docs/device_selection.png" width="650" style="border-radius: 10px;" alt="Device selection" />
</p>

### ⚡ GPU Acceleration

Built-in support for hardware acceleration across platforms:
- **macOS**: Apple Silicon (Metal) + CoreML
- **Windows/Linux**: NVIDIA (CUDA), AMD/Intel (Vulkan)

Automatically enabled at build time - no configuration needed.

## System Architecture

Meetily is a single, self-contained application built with [Tauri](https://tauri.app/). It uses a Rust-based backend to handle all the core logic, and a Next.js frontend for the user interface.

For more details, see the [Architecture documentation](docs/architecture.md).

## For Developers

If you want to contribute to Meetily or build it from source, you'll need to have Rust and Node.js installed. For detailed build instructions, please see the [Building from Source guide](docs/BUILDING.md).

## Enterprise Solutions

**Meetily Enterprise** is available for on-premise deployment, giving organizations complete control over their meeting intelligence infrastructure. This enterprise version includes:

-   **100% On-Premise Deployment**: Your data never leaves your infrastructure
-   **Centralized Management**: Support for 100+ users with administrative controls
-   **Zero Vendor Lock-in**: Open source MIT license ensures complete ownership
-   **Compliance Ready**: Meet GDPR, SOX, HIPAA, and industry-specific requirements
-   **Custom Integration**: APIs and webhooks for enterprise systems

For enterprise solutions: [https://meetily.zackriya.com](https://meetily.zackriya.com)

## Contributing

We welcome contributions from the community! If you have any questions or suggestions, please open an issue or submit a pull request. Please follow the established project structure and guidelines. For more details, refer to the [CONTRIBUTING.md](CONTRIBUTING.md) file.

## License

MIT License - Feel free to use this project for your own purposes.


## Contributions

Thanks for all the contributions. Our community is what makes this project possible. Below is the list of contributors:

<a href="https://github.com/zackriya-solutions/meeting-minutes/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=zackriya-solutions/meeting-minutes" />
</a>


We welcome contributions from the community! If you have any questions or suggestions, please open an issue or submit a pull request. Please follow the established project structure and guidelines. For more details, refer to the [CONTRIBUTING](CONTRIBUTING.md) file.

## Acknowledgments

- We borrowes some code from [Whisper.cpp](https://github.com/ggerganov/whisper.cpp)
- We borrowes some code from [Screenpipe](https://github.com/mediar-ai/screenpipe)


## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=Zackriya-Solutions/meeting-minutes&type=Date)](https://star-history.com/#Zackriya-Solutions/meeting-minutes&Date)