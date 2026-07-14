//! Cloud speaker diarization via SaluteSpeech's async recognition API.
//!
//! Flow (verified live against speech.giga.chat): upload the meeting audio →
//! `speech:async_recognize` with `speaker_separation_options.enable` → poll `task:get`
//! → `data:download`. The result is an array of recognition entries; the per-speaker
//! **partial** entries (`eou=false`, `speaker_info.speaker_id >= 0`) carry the turn
//! boundaries (`results[0].start/end`, absolute seconds from file start). The final
//! `eou=true` aggregate has `speaker_id = -1` (mixed) and is ignored.
//!
//! Returns turns; the caller ([`crate::pipeline::diarization_commands`]) maps them onto
//! transcript segments with the same attribution used by the local diarizer.

use std::time::Duration;

use serde_json::json;

use super::auth::SaluteSpeechAuth;
use super::SaluteSpeechConfig;

const MAX_POLLS: usize = 90; // ~90 * 2s = 3 min ceiling
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// A speaker turn from cloud diarization: recording-relative milliseconds + cloud speaker id.
#[derive(Debug, Clone)]
pub struct CloudTurn {
    pub speaker_id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// The gateway REST base (e.g. `https://speech.giga.chat/rest/v1`), derived from the
/// configured recognize URL by dropping the final `/speech:recognize` segment.
fn rest_base(recognize_url: &str) -> &str {
    recognize_url
        .trim_end_matches('/')
        .rsplit_once('/')
        .map(|(base, _)| base)
        .unwrap_or(recognize_url)
}

/// Run async recognition with speaker separation over `pcm16` (16 kHz mono, 16-bit LE PCM)
/// and return the detected speaker turns.
pub async fn diarize_pcm16(cfg: &SaluteSpeechConfig, pcm16: Vec<u8>) -> Result<Vec<CloudTurn>, String> {
    if pcm16.is_empty() {
        return Ok(Vec::new());
    }

    let auth = SaluteSpeechAuth::new(cfg.auth_key.clone(), cfg.oauth_url.clone(), cfg.scope.clone());
    let token = auth.access_token().await?;
    let base = rest_base(&cfg.recognize_url);
    let client = reqwest::Client::new();

    // 1) Upload the audio.
    let upload: serde_json::Value = client
        .post(format!("{base}/data:upload"))
        .bearer_auth(&token)
        .header(reqwest::header::CONTENT_TYPE, "audio/x-pcm;bit=16;rate=16000")
        .header(reqwest::header::USER_AGENT, "GigaChat-Meetily")
        .body(pcm16)
        .send()
        .await
        .map_err(|e| format!("salutespeech upload failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("salutespeech upload error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("salutespeech upload parse: {e}"))?;
    let request_file_id = upload
        .pointer("/result/request_file_id")
        .and_then(|v| v.as_str())
        .ok_or("salutespeech upload: missing request_file_id")?
        .to_string();

    // 2) Kick off async recognition with speaker separation.
    let body = json!({
        "options": {
            "model": cfg.model,
            "audio_encoding": "PCM_S16LE",
            "sample_rate": 16000,
            "language": "ru-RU",
            "speaker_separation_options": { "enable": true }
        },
        "request_file_id": request_file_id
    });
    let started: serde_json::Value = client
        .post(format!("{base}/speech:async_recognize"))
        .bearer_auth(&token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, "GigaChat-Meetily")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("salutespeech async_recognize failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("salutespeech async_recognize error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("salutespeech async_recognize parse: {e}"))?;
    let task_id = started
        .pointer("/result/id")
        .and_then(|v| v.as_str())
        .ok_or("salutespeech async_recognize: missing task id")?
        .to_string();

    // 3) Poll until the task is DONE.
    let mut response_file_id: Option<String> = None;
    for _ in 0..MAX_POLLS {
        let task: serde_json::Value = client
            .get(format!("{base}/task:get"))
            .query(&[("id", task_id.as_str())])
            .bearer_auth(&token)
            .header(reqwest::header::USER_AGENT, "GigaChat-Meetily")
            .send()
            .await
            .map_err(|e| format!("salutespeech task:get failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("salutespeech task:get error: {e}"))?
            .json()
            .await
            .map_err(|e| format!("salutespeech task:get parse: {e}"))?;
        match task.pointer("/result/status").and_then(|v| v.as_str()).unwrap_or("") {
            "DONE" => {
                response_file_id = task
                    .pointer("/result/response_file_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                break;
            }
            "ERROR" | "CANCELED" => {
                return Err(format!("salutespeech recognition task {task}"));
            }
            _ => tokio::time::sleep(POLL_INTERVAL).await,
        }
    }
    let response_file_id =
        response_file_id.ok_or("salutespeech: recognition task did not complete in time")?;

    // 4) Download and parse the speaker-labeled result.
    let text = client
        .get(format!("{base}/data:download"))
        .query(&[("response_file_id", response_file_id.as_str())])
        .bearer_auth(&token)
        .header(reqwest::header::USER_AGENT, "GigaChat-Meetily")
        .send()
        .await
        .map_err(|e| format!("salutespeech data:download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("salutespeech data:download error: {e}"))?
        .text()
        .await
        .map_err(|e| format!("salutespeech data:download read: {e}"))?;
    let result: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("salutespeech data:download parse: {e}"))?;

    Ok(parse_turns(&result))
}

/// Extract turns from the download payload: per-speaker partial entries
/// (`speaker_id >= 0`) with a valid `results[0].start/end` range.
fn parse_turns(v: &serde_json::Value) -> Vec<CloudTurn> {
    let mut turns = Vec::new();
    let Some(arr) = v.as_array() else {
        return turns;
    };
    for entry in arr {
        let Some(sid) = entry.pointer("/speaker_info/speaker_id").and_then(|s| s.as_i64()) else {
            continue;
        };
        if sid < 0 {
            continue; // -1 = merged/unknown aggregate
        }
        let start = entry.pointer("/results/0/start").and_then(|s| s.as_str()).and_then(parse_go_duration);
        let end = entry.pointer("/results/0/end").and_then(|s| s.as_str()).and_then(parse_go_duration);
        if let (Some(start), Some(end)) = (start, end) {
            if end > start {
                turns.push(CloudTurn {
                    speaker_id: sid,
                    start_ms: (start * 1000.0).round() as i64,
                    end_ms: (end * 1000.0).round() as i64,
                });
            }
        }
    }
    turns
}

/// Parse a Go-style duration string ("2.280s", "2s", "0.040s") to seconds.
fn parse_go_duration(s: &str) -> Option<f64> {
    s.trim().strip_suffix('s').unwrap_or(s.trim()).parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rest_base_derives_gateway() {
        assert_eq!(
            rest_base("https://speech.giga.chat/rest/v1/speech:recognize"),
            "https://speech.giga.chat/rest/v1"
        );
    }

    #[test]
    fn parse_go_duration_handles_forms() {
        assert_eq!(parse_go_duration("2.280s"), Some(2.280));
        assert_eq!(parse_go_duration("2s"), Some(2.0));
        assert_eq!(parse_go_duration("0.040s"), Some(0.040));
        assert_eq!(parse_go_duration("bad"), None);
    }

    #[test]
    fn parse_turns_keeps_speakers_drops_aggregate() {
        // Mirrors the verified 2-speaker payload: two per-speaker partials + a -1 aggregate.
        let v = json!([
            {"speaker_info": {"speaker_id": 1}, "eou": false, "results": [{"start": "0.040s", "end": "2.280s"}]},
            {"speaker_info": {"speaker_id": -1}, "eou": true, "results": [{"start": "0.040s", "end": "7.160s"}]},
            {"speaker_info": {"speaker_id": 0}, "eou": false, "results": [{"start": "3.040s", "end": "7.160s"}]}
        ]);
        let turns = parse_turns(&v);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].speaker_id, 1);
        assert_eq!(turns[0].start_ms, 40);
        assert_eq!(turns[0].end_ms, 2280);
        assert_eq!(turns[1].speaker_id, 0);
        assert_eq!(turns[1].start_ms, 3040);
        assert_eq!(turns[1].end_ms, 7160);
    }
}
