use crate::api::TranscriptSegment;
use crate::audio::common::{
    create_transcript_segments, split_segment_at_silence, write_transcripts_json,
};
use crate::audio::constants::AUDIO_EXTENSIONS;
use crate::audio::decoder::decode_audio_file;
use crate::audio::vad::get_speech_chunks_with_progress;
use crate::state::AppState;
use anyhow::{anyhow, Result};
use base64::Engine;
use log::{debug, info, warn};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, Runtime};

const VAD_REDEMPTION_TIME_MS: u32 = 2000;
const MAX_SEGMENT_SAMPLES: usize = 25 * 16000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedBatchTranscriptionProgress {
    pub meeting_id: String,
    pub stage: String,
    pub progress_percentage: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedBatchTranscriptionResult {
    pub meeting_id: String,
    pub segments_count: usize,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiTranscriptionResponse {
    text: String,
}

#[derive(Debug, Deserialize)]
struct GeminiGenerateContentResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
}

#[derive(Debug, Deserialize)]
struct GeminiContent {
    parts: Option<Vec<GeminiPart>>,
}

#[derive(Debug, Deserialize)]
struct GeminiPart {
    text: Option<String>,
}

fn emit_progress<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    stage: &str,
    progress_percentage: u32,
    message: &str,
) {
    let _ = app.emit(
        "hosted-transcription-progress",
        HostedBatchTranscriptionProgress {
            meeting_id: meeting_id.to_string(),
            stage: stage.to_string(),
            progress_percentage,
            message: message.to_string(),
        },
    );
}

fn find_audio_file(folder: &Path) -> Result<PathBuf> {
    let candidates = [
        "audio.mp4",
        "audio.m4a",
        "audio.wav",
        "audio.mp3",
        "audio.flac",
        "audio.ogg",
        "recording.mp4",
        "audio.mkv",
        "audio.webm",
        "audio.wma",
    ];

    for name in candidates {
        let path = folder.join(name);
        if path.exists() {
            return Ok(path);
        }
    }

    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                return Ok(path);
            }
        }
    }

    Err(anyhow!("No audio file found in {}", folder.display()))
}

fn encode_wav_16khz_mono(samples: &[f32]) -> Vec<u8> {
    let sample_rate = 16_000u32;
    let channels = 1u16;
    let bits_per_sample = 16u16;
    let bytes_per_sample = bits_per_sample / 8;
    let data_size = samples.len() as u32 * bytes_per_sample as u32;
    let byte_rate = sample_rate * channels as u32 * bytes_per_sample as u32;
    let block_align = channels * bytes_per_sample;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        wav.extend_from_slice(&pcm.to_le_bytes());
    }

    wav
}

async fn transcribe_openai_segment(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    wav_bytes: Vec<u8>,
    language: Option<&str>,
) -> Result<String> {
    let mut form = Form::new().text("model", model.to_string()).part(
        "file",
        Part::bytes(wav_bytes)
            .file_name("segment.wav")
            .mime_str("audio/wav")?,
    );

    if let Some(lang) = language {
        form = form.text("language", lang.to_string());
    }

    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!(
            "OpenAI transcription failed ({}): {}",
            status,
            body
        ));
    }

    let parsed: OpenAiTranscriptionResponse = serde_json::from_str(&body)?;
    Ok(parsed.text.trim().to_string())
}

async fn transcribe_gemini_segment(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    wav_bytes: Vec<u8>,
    language: Option<&str>,
) -> Result<String> {
    let language_hint = language.unwrap_or("he");
    let encoded = base64::engine::general_purpose::STANDARD.encode(wav_bytes);
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let request_body = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [
                {
                    "text": format!(
                        "Transcribe this meeting audio segment verbatim. The primary language is {}. Return only the transcript text. Do not summarize, translate, add labels, or include timestamps.",
                        language_hint
                    )
                },
                {
                    "inline_data": {
                        "mime_type": "audio/wav",
                        "data": encoded
                    }
                }
            ]
        }],
        "generationConfig": {
            "temperature": 0.0
        }
    });

    let response = client.post(url).json(&request_body).send().await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!(
            "Gemini transcription failed ({}): {}",
            status,
            body
        ));
    }

    let parsed: GeminiGenerateContentResponse = serde_json::from_str(&body)?;
    let text = parsed
        .candidates
        .unwrap_or_default()
        .into_iter()
        .filter_map(|candidate| candidate.content)
        .flat_map(|content| content.parts.unwrap_or_default())
        .filter_map(|part| part.text)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    Ok(text)
}

async fn save_hosted_transcripts<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    folder_path: &Path,
    segments: &[TranscriptSegment],
) -> Result<()> {
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;
    let pool = app_state.db_manager.pool();
    let mut conn = pool.acquire().await?;
    let mut tx = sqlx::Connection::begin(&mut *conn).await?;

    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;

    for segment in segments {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&segment.id)
        .bind(meeting_id)
        .bind(&segment.text)
        .bind(&segment.timestamp)
        .bind(segment.audio_start_time)
        .bind(segment.audio_end_time)
        .bind(segment.duration)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    write_transcripts_json(folder_path, segments)?;
    Ok(())
}

pub async fn run_hosted_batch_transcription<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    meeting_folder_path: String,
    provider: Option<String>,
    model: Option<String>,
    language: Option<String>,
) -> Result<HostedBatchTranscriptionResult> {
    let config = crate::api::api::api_get_transcript_config(app.clone(), app.clone().state(), None)
        .await
        .map_err(|e| anyhow!(e))?
        .ok_or_else(|| anyhow!("No transcript provider configured"))?;

    let provider = provider.unwrap_or(config.provider);
    if !crate::audio::transcription::is_hosted_transcription_provider(&provider) {
        return Err(anyhow!(
            "Provider '{}' is not a hosted batch provider",
            provider
        ));
    }

    let model = model.unwrap_or_else(|| {
        if !config.model.trim().is_empty() {
            config.model
        } else if provider == "openai" {
            crate::config::DEFAULT_OPENAI_TRANSCRIPTION_MODEL.to_string()
        } else {
            crate::config::DEFAULT_GEMINI_TRANSCRIPTION_MODEL.to_string()
        }
    });

    let api_key = config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Missing API key for {}", provider))?
        .to_string();

    let folder_path = PathBuf::from(&meeting_folder_path);
    let audio_path = find_audio_file(&folder_path)?;

    emit_progress(
        &app,
        &meeting_id,
        "decoding",
        5,
        "Decoding recorded audio...",
    );
    let decoded = tokio::task::spawn_blocking({
        let audio_path = audio_path.clone();
        move || decode_audio_file(&audio_path)
    })
    .await??;

    let audio_samples = tokio::task::spawn_blocking(move || decoded.to_whisper_format()).await?;
    emit_progress(&app, &meeting_id, "vad", 15, "Detecting speech segments...");

    let speech_segments = tokio::task::spawn_blocking(move || {
        get_speech_chunks_with_progress(
            &audio_samples,
            VAD_REDEMPTION_TIME_MS,
            |_progress, _segments| true,
        )
    })
    .await??;

    let mut processable_segments = Vec::new();
    for segment in speech_segments {
        if segment.samples.len() > MAX_SEGMENT_SAMPLES {
            processable_segments.extend(split_segment_at_silence(&segment, MAX_SEGMENT_SAMPLES));
        } else {
            processable_segments.push(segment);
        }
    }

    if processable_segments.is_empty() {
        return Err(anyhow!("No speech detected in recorded audio"));
    }

    let total = processable_segments.len();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let mut transcripts = Vec::new();

    for (index, segment) in processable_segments.iter().enumerate() {
        if segment.samples.len() < 1600 {
            debug!("Skipping very short hosted transcription segment {}", index);
            continue;
        }

        let progress = 15 + ((index as f32 / total as f32) * 75.0) as u32;
        emit_progress(
            &app,
            &meeting_id,
            "transcribing",
            progress,
            &format!(
                "Transcribing segment {} of {} with {}...",
                index + 1,
                total,
                provider
            ),
        );

        let wav_bytes = encode_wav_16khz_mono(&segment.samples);
        let text = match provider.as_str() {
            "openai" => {
                transcribe_openai_segment(&client, &api_key, &model, wav_bytes, language.as_deref())
                    .await?
            }
            "gemini" => {
                transcribe_gemini_segment(&client, &api_key, &model, wav_bytes, language.as_deref())
                    .await?
            }
            _ => unreachable!(),
        };

        let trimmed = text.trim();
        if !trimmed.is_empty() {
            transcripts.push((
                trimmed.to_string(),
                segment.start_timestamp_ms,
                segment.end_timestamp_ms,
            ));
        }
    }

    let segments = create_transcript_segments(&transcripts);
    emit_progress(
        &app,
        &meeting_id,
        "saving",
        95,
        "Saving hosted transcript...",
    );
    save_hosted_transcripts(&app, &meeting_id, &folder_path, &segments).await?;

    emit_progress(
        &app,
        &meeting_id,
        "complete",
        100,
        "Hosted transcription complete",
    );
    info!(
        "Hosted transcription complete for meeting {}: {} segments via {}/{}",
        meeting_id,
        segments.len(),
        provider,
        model
    );

    if segments.is_empty() {
        warn!("Hosted transcription produced no transcript segments");
    }

    Ok(HostedBatchTranscriptionResult {
        meeting_id,
        segments_count: segments.len(),
        provider,
        model,
    })
}

#[tauri::command]
pub async fn start_hosted_batch_transcription_command<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    meeting_folder_path: String,
    provider: Option<String>,
    model: Option<String>,
    language: Option<String>,
) -> Result<HostedBatchTranscriptionResult, String> {
    run_hosted_batch_transcription(
        app,
        meeting_id,
        meeting_folder_path,
        provider,
        model,
        language,
    )
    .await
    .map_err(|e| e.to_string())
}
