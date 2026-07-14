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

/// Read a response body, turning non-2xx statuses into an error that carries the
/// gateway's explanation (it answers 4xx with `{"status":N,"message":"..."}` — dropping
/// it via `error_for_status()` reduced real causes to an opaque "400 Bad Request").
async fn read_body_checked(resp: reqwest::Response, ctx: &str) -> Result<String, String> {
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("salutespeech {ctx} read: {e}"))?;
    if status.is_success() {
        return Ok(text);
    }
    let detail: String = text.trim().chars().take(300).collect();
    if detail.is_empty() {
        Err(format!("salutespeech {ctx}: HTTP {status}"))
    } else {
        Err(format!("salutespeech {ctx}: HTTP {status}: {detail}"))
    }
}

/// [`read_body_checked`] + JSON parse.
async fn read_json_checked(resp: reqwest::Response, ctx: &str) -> Result<serde_json::Value, String> {
    let text = read_body_checked(resp, ctx).await?;
    serde_json::from_str(&text).map_err(|e| format!("salutespeech {ctx} parse: {e}"))
}

/// Run async recognition with speaker separation over `pcm16` (16 kHz mono, 16-bit LE PCM)
/// and return the detected speaker turns. `expected_speakers` is the user's speaker-count
/// hint (in-meeting control pill); when set it is forwarded as
/// `speaker_separation_options.count_of_speaker` (SmartSpeech proto field name).
pub async fn diarize_pcm16(
    cfg: &SaluteSpeechConfig,
    pcm16: Vec<u8>,
    expected_speakers: Option<u32>,
) -> Result<Vec<CloudTurn>, String> {
    if pcm16.is_empty() {
        return Ok(Vec::new());
    }

    let auth = SaluteSpeechAuth::new(cfg.auth_key.clone(), cfg.oauth_url.clone(), cfg.scope.clone());
    let token = auth.access_token().await?;
    let base = rest_base(&cfg.recognize_url);
    let client = reqwest::Client::new();

    // 1) Upload the audio.
    let resp = client
        .post(format!("{base}/data:upload"))
        .bearer_auth(&token)
        .header(reqwest::header::CONTENT_TYPE, "audio/x-pcm;bit=16;rate=16000")
        .header(reqwest::header::USER_AGENT, "GigaChat-Meetily")
        .body(pcm16)
        .send()
        .await
        .map_err(|e| format!("salutespeech upload failed: {e}"))?;
    let upload = read_json_checked(resp, "upload").await?;
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
            "speaker_separation_options": speaker_separation_options(expected_speakers)
        },
        "request_file_id": request_file_id
    });
    let resp = client
        .post(format!("{base}/speech:async_recognize"))
        .bearer_auth(&token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, "GigaChat-Meetily")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("salutespeech async_recognize failed: {e}"))?;
    let started = read_json_checked(resp, "async_recognize").await?;
    let task_id = started
        .pointer("/result/id")
        .and_then(|v| v.as_str())
        .ok_or("salutespeech async_recognize: missing task id")?
        .to_string();

    // 3) Poll until the task is DONE.
    let mut response_file_id: Option<String> = None;
    for _ in 0..MAX_POLLS {
        let resp = client
            .get(format!("{base}/task:get"))
            .query(&[("id", task_id.as_str())])
            .bearer_auth(&token)
            .header(reqwest::header::USER_AGENT, "GigaChat-Meetily")
            .send()
            .await
            .map_err(|e| format!("salutespeech task:get failed: {e}"))?;
        let task = read_json_checked(resp, "task:get").await?;
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
    let resp = client
        .get(format!("{base}/data:download"))
        .query(&[("response_file_id", response_file_id.as_str())])
        .bearer_auth(&token)
        .header(reqwest::header::USER_AGENT, "GigaChat-Meetily")
        .send()
        .await
        .map_err(|e| format!("salutespeech data:download failed: {e}"))?;
    let text = read_body_checked(resp, "data:download").await?;
    let result: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("salutespeech data:download parse: {e}"))?;

    Ok(parse_turns(&result))
}

/// Build the `speaker_separation_options` request object: always enabled, with the
/// user's expected-speaker-count hint attached when provided. The REST field is `count`
/// (verified live 2026-07-14: the gateway strictly rejects unknown fields, and the gRPC
/// proto name `count_of_speaker` gets HTTP 400 "unknown field"; `count` returns 200 and
/// the task completes).
fn speaker_separation_options(expected_speakers: Option<u32>) -> serde_json::Value {
    match expected_speakers {
        Some(n) if n >= 1 => json!({ "enable": true, "count": n }),
        _ => json!({ "enable": true }),
    }
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
    fn speaker_separation_options_carries_the_count_hint() {
        assert_eq!(speaker_separation_options(None), json!({ "enable": true }));
        assert_eq!(
            speaker_separation_options(Some(3)),
            json!({ "enable": true, "count": 3 })
        );
        // A degenerate 0 hint is dropped rather than sent.
        assert_eq!(speaker_separation_options(Some(0)), json!({ "enable": true }));
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
