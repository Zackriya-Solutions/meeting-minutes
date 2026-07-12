use crate::audio::recording_state::AudioChunk;
use std::collections::VecDeque;

const SAMPLE_RATE: usize = 16_000;
const INFERENCE_STEP_SAMPLES: usize = SAMPLE_RATE;
const MAX_WINDOW_SAMPLES: usize = SAMPLE_RATE * 15;
const MIN_STREAMING_RMS: f32 = 0.0005;
const CONFIRMED_TIME_TOLERANCE_SECONDS: f64 = 0.01;
const CONFIRMED_AUDIO_OVERLAP_SECONDS: f64 = 0.5;
const PROMPT_WORD_LIMIT: usize = 50;

#[derive(Debug)]
struct ConfirmedPromptWord {
    text: String,
    end: f64,
}

#[derive(Debug)]
pub enum TranscriptionInput {
    StreamingAudio(AudioChunk),
    UtteranceEnd(AudioChunk),
}

impl TranscriptionInput {
    pub fn chunk(&self) -> &AudioChunk {
        match self {
            Self::StreamingAudio(chunk) | Self::UtteranceEnd(chunk) => chunk,
        }
    }

    pub fn into_chunk(self) -> AudioChunk {
        match self {
            Self::StreamingAudio(chunk) | Self::UtteranceEnd(chunk) => chunk,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptionMode {
    WhisperStreaming,
    FinalOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRoute {
    Stream,
    Finalize,
    Ignore,
}

pub fn route_input(mode: TranscriptionMode, input: &TranscriptionInput) -> InputRoute {
    match (mode, input) {
        (TranscriptionMode::WhisperStreaming, TranscriptionInput::StreamingAudio(_)) => {
            InputRoute::Stream
        }
        (_, TranscriptionInput::UtteranceEnd(_)) => InputRoute::Finalize,
        (TranscriptionMode::FinalOnly, TranscriptionInput::StreamingAudio(_)) => InputRoute::Ignore,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamingWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub probability: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamingHypothesis {
    pub words: Vec<StreamingWord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamingSegment {
    pub text: String,
    pub audio_start_time: f64,
    pub audio_end_time: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StreamingUpdate {
    pub stable: Option<StreamingSegment>,
    pub preview: Option<StreamingSegment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamingWindow {
    pub samples: Vec<f32>,
    pub start_time: f64,
    pub prompt: String,
}

#[derive(Debug, Default)]
pub struct WhisperStreamingSession {
    audio: Vec<f32>,
    audio_start_time: Option<f64>,
    new_samples_since_inference: usize,
    previous_unconfirmed: Vec<StreamingWord>,
    confirmed_words: Vec<StreamingWord>,
    last_confirmed_end: Option<f64>,
    confirmed_prompt: VecDeque<ConfirmedPromptWord>,
    preview: Option<StreamingSegment>,
}

impl WhisperStreamingSession {
    pub fn push_audio(&mut self, chunk: AudioChunk) {
        if chunk.data.is_empty() {
            return;
        }

        if self.audio_start_time.is_none() {
            self.audio_start_time = Some(chunk.timestamp);
        }

        self.new_samples_since_inference = self
            .new_samples_since_inference
            .saturating_add(chunk.data.len());
        self.audio.extend_from_slice(&chunk.data);

        if self.audio.len() > MAX_WINDOW_SAMPLES {
            let trim = self.audio.len() - MAX_WINDOW_SAMPLES;
            self.audio.drain(..trim);
            if let Some(start) = self.audio_start_time.as_mut() {
                *start += trim as f64 / SAMPLE_RATE as f64;
            }
        }
    }

    pub fn take_inference_window(&mut self) -> Option<StreamingWindow> {
        if self.audio.is_empty() || self.new_samples_since_inference < INFERENCE_STEP_SAMPLES {
            return None;
        }

        let new_samples = self.new_samples_since_inference.min(self.audio.len());
        self.new_samples_since_inference = 0;
        let recent_audio = &self.audio[self.audio.len() - new_samples..];
        let rms = (recent_audio
            .iter()
            .map(|sample| sample * sample)
            .sum::<f32>()
            / recent_audio.len() as f32)
            .sqrt();
        if rms < MIN_STREAMING_RMS {
            return None;
        }

        Some(StreamingWindow {
            samples: self.audio.clone(),
            start_time: self.audio_start_time.unwrap_or(0.0),
            prompt: self.prompt(),
        })
    }

    pub fn accept_relative_hypothesis(
        &mut self,
        mut hypothesis: StreamingHypothesis,
        window_start_time: f64,
    ) -> StreamingUpdate {
        for word in &mut hypothesis.words {
            word.start += window_start_time;
            word.end += window_start_time;
        }
        self.accept_hypothesis(hypothesis)
    }

    fn accept_hypothesis(&mut self, hypothesis: StreamingHypothesis) -> StreamingUpdate {
        let current = self.strip_confirmed_prefix(hypothesis.words);
        let common_len = self.common_prefix_len(&current);
        let confirmed_words = current[..common_len].to_vec();
        let remaining = current[common_len..].to_vec();

        if let Some(last) = confirmed_words.last() {
            self.last_confirmed_end = Some(last.end);
            self.extend_prompt(&confirmed_words);
            self.confirmed_words.extend(confirmed_words.clone());
            self.trim_confirmed_audio(last.end);
        }

        self.previous_unconfirmed = remaining.clone();
        self.preview = self.segment_from_words(&remaining);

        StreamingUpdate {
            stable: self.segment_from_words(&self.confirmed_words),
            preview: self.preview.clone(),
        }
    }

    pub fn stable_text(&self) -> String {
        join_words(&self.confirmed_words)
    }

    pub fn confirmed_end_time(&self) -> Option<f64> {
        self.last_confirmed_end
    }

    fn prompt(&self) -> String {
        self.prompt_for_start(self.audio_start_time.unwrap_or(f64::INFINITY))
    }

    fn prompt_for_start(&self, window_start_time: f64) -> String {
        self.confirmed_prompt
            .iter()
            .filter(|word| word.end <= window_start_time + CONFIRMED_TIME_TOLERANCE_SECONDS)
            .map(|word| word.text.clone())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn reset_utterance(&mut self) {
        self.audio.clear();
        self.audio_start_time = None;
        self.new_samples_since_inference = 0;
        self.previous_unconfirmed.clear();
        self.confirmed_words.clear();
        self.preview = None;
        self.last_confirmed_end = None;
    }

    fn strip_confirmed_prefix(&self, words: Vec<StreamingWord>) -> Vec<StreamingWord> {
        let overlap = longest_word_overlap(&self.confirmed_words, &words);
        words.into_iter().skip(overlap).collect()
    }

    fn common_prefix_len(&self, current: &[StreamingWord]) -> usize {
        self.previous_unconfirmed
            .iter()
            .zip(current)
            .take_while(|(previous, current)| {
                normalize_word(&previous.text) == normalize_word(&current.text)
            })
            .count()
    }

    fn segment_from_words(&self, words: &[StreamingWord]) -> Option<StreamingSegment> {
        let first = words.first()?;
        let last = words.last()?;

        Some(StreamingSegment {
            text: join_words(words),
            audio_start_time: first.start,
            audio_end_time: last.end,
        })
    }

    fn extend_prompt(&mut self, words: &[StreamingWord]) {
        for word in words {
            self.confirmed_prompt.push_back(ConfirmedPromptWord {
                text: word.text.clone(),
                end: word.end,
            });
        }
        while self.confirmed_prompt.len() > PROMPT_WORD_LIMIT {
            self.confirmed_prompt.pop_front();
        }
    }

    fn trim_confirmed_audio(&mut self, confirmed_end: f64) {
        let Some(audio_start) = self.audio_start_time else {
            return;
        };
        let keep_from = (confirmed_end - CONFIRMED_AUDIO_OVERLAP_SECONDS).max(audio_start);
        let trim_samples = ((keep_from - audio_start) * SAMPLE_RATE as f64).round() as usize;
        let trim_samples = trim_samples.min(self.audio.len());
        if trim_samples == 0 {
            return;
        }

        self.audio.drain(..trim_samples);
        self.audio_start_time = Some(audio_start + trim_samples as f64 / SAMPLE_RATE as f64);
    }
}

fn longest_word_overlap(confirmed: &[StreamingWord], current: &[StreamingWord]) -> usize {
    let max_overlap = confirmed.len().min(current.len());
    (1..=max_overlap)
        .rev()
        .find(|&overlap| {
            confirmed[confirmed.len() - overlap..]
                .iter()
                .zip(&current[..overlap])
                .all(|(confirmed, current)| {
                    normalize_word(&confirmed.text) == normalize_word(&current.text)
                })
        })
        .unwrap_or(0)
}

fn normalize_word(word: &str) -> String {
    word.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn join_words(words: &[StreamingWord]) -> String {
    let mut text = String::new();
    for word in words {
        let value = word.text.trim();
        if value.is_empty() {
            continue;
        }

        let first_character = value.chars().next();
        let previous_character = text.chars().last();
        let attaches_to_previous = first_character
            .is_some_and(|character| !character.is_alphanumeric() || is_cjk_or_thai(character))
            || previous_character.is_some_and(is_cjk_or_thai);
        if !text.is_empty() && !attaches_to_previous {
            text.push(' ');
        }
        text.push_str(value);
    }
    text
}

fn is_cjk_or_thai(character: char) -> bool {
    matches!(
        character as u32,
        0x0E00..=0x0E7F
            | 0x3040..=0x30FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
    )
}

pub fn merge_transcript_parts(stable: &str, tail: &str) -> String {
    if !stable.chars().any(char::is_whitespace)
        && !tail.chars().any(char::is_whitespace)
        && stable.chars().chain(tail.chars()).any(is_cjk_or_thai)
    {
        let stable_characters: Vec<char> = stable.chars().collect();
        let tail_characters: Vec<char> = tail.chars().collect();
        let max_overlap = stable_characters.len().min(tail_characters.len());
        let overlap = (1..=max_overlap)
            .rev()
            .find(|&length| {
                stable_characters[stable_characters.len() - length..] == tail_characters[..length]
            })
            .unwrap_or(0);
        return stable_characters
            .into_iter()
            .chain(tail_characters.into_iter().skip(overlap))
            .collect();
    }

    let stable_words: Vec<&str> = stable.split_whitespace().collect();
    let tail_words: Vec<&str> = tail.split_whitespace().collect();
    let max_overlap = stable_words.len().min(tail_words.len());
    let overlap = (1..=max_overlap)
        .rev()
        .find(|&length| {
            stable_words[stable_words.len() - length..]
                .iter()
                .zip(&tail_words[..length])
                .all(|(stable, tail)| normalize_word(stable) == normalize_word(tail))
        })
        .unwrap_or(0);

    stable_words
        .into_iter()
        .chain(tail_words.into_iter().skip(overlap))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn coalesce_streaming_audio(pending: &mut AudioChunk, next: AudioChunk) {
    debug_assert_eq!(pending.sample_rate, next.sample_rate);
    pending.data.extend(next.data);
    pending.chunk_id = next.chunk_id;
}

#[cfg(test)]
mod tests {
    use super::{
        coalesce_streaming_audio, merge_transcript_parts, route_input, InputRoute,
        StreamingHypothesis, StreamingWord, TranscriptionInput, TranscriptionMode,
        WhisperStreamingSession,
    };
    use crate::audio::recording_state::{AudioChunk, DeviceType};

    fn word(text: &str, start: f64, end: f64) -> StreamingWord {
        StreamingWord {
            text: text.to_string(),
            start,
            end,
            probability: 0.9,
        }
    }

    fn hypothesis(words: Vec<StreamingWord>) -> StreamingHypothesis {
        StreamingHypothesis { words }
    }

    fn chunk(samples: usize, timestamp: f64) -> AudioChunk {
        AudioChunk {
            data: vec![0.1; samples],
            sample_rate: 16_000,
            timestamp,
            chunk_id: 1,
            device_type: DeviceType::Microphone,
        }
    }

    #[test]
    fn confirms_the_common_prefix_after_two_hypotheses() {
        let mut session = WhisperStreamingSession::default();

        let first = session.accept_hypothesis(hypothesis(vec![
            word("Hello", 0.0, 0.4),
            word("world", 0.4, 0.8),
            word("today", 0.8, 1.2),
        ]));
        assert!(first.stable.is_none());
        assert_eq!(first.preview.unwrap().text, "Hello world today");

        let second = session.accept_hypothesis(hypothesis(vec![
            word("Hello", 0.0, 0.4),
            word("world", 0.4, 0.8),
            word("tomorrow", 0.8, 1.3),
        ]));

        assert_eq!(second.stable.unwrap().text, "Hello world");
        assert_eq!(second.preview.unwrap().text, "tomorrow");
    }

    #[test]
    fn agreement_ignores_case_and_surrounding_punctuation() {
        let mut session = WhisperStreamingSession::default();
        session.accept_hypothesis(hypothesis(vec![word("Hello,", 0.0, 0.5)]));

        let update = session.accept_hypothesis(hypothesis(vec![
            word("hello", 0.0, 0.5),
            word("there", 0.5, 0.9),
        ]));

        assert_eq!(update.stable.unwrap().text, "hello");
        assert_eq!(update.preview.unwrap().text, "there");
    }

    #[test]
    fn cjk_comparison_units_render_without_inserted_spaces() {
        let mut session = WhisperStreamingSession::default();
        session.accept_hypothesis(hypothesis(vec![word("你", 0.0, 0.2), word("好", 0.2, 0.4)]));
        let update =
            session.accept_hypothesis(hypothesis(vec![word("你", 0.0, 0.2), word("好", 0.2, 0.4)]));

        assert_eq!(update.stable.unwrap().text, "你好");
    }

    #[test]
    fn already_confirmed_words_are_not_emitted_twice() {
        let mut session = WhisperStreamingSession::default();
        session.accept_hypothesis(hypothesis(vec![word("stable", 0.0, 0.5)]));
        session.accept_hypothesis(hypothesis(vec![word("stable", 0.0, 0.5)]));

        session.accept_hypothesis(hypothesis(vec![
            word("stable", 0.0, 0.5),
            word("next", 0.5, 0.9),
        ]));
        let update = session.accept_hypothesis(hypothesis(vec![
            word("stable", 0.0, 0.5),
            word("next", 0.5, 0.9),
        ]));

        assert_eq!(update.stable.unwrap().text, "stable next");
    }

    #[test]
    fn timestamp_drift_does_not_reconfirm_existing_words() {
        let mut session = WhisperStreamingSession::default();
        session.accept_hypothesis(hypothesis(vec![
            word("hello", 0.0, 0.4),
            word("world", 0.4, 0.8),
        ]));
        session.accept_hypothesis(hypothesis(vec![
            word("hello", 0.02, 0.42),
            word("world", 0.42, 0.82),
        ]));

        session.accept_hypothesis(hypothesis(vec![
            word("hello", 0.04, 0.44),
            word("world", 0.44, 0.94),
            word("next", 0.94, 1.2),
        ]));
        let update = session.accept_hypothesis(hypothesis(vec![
            word("hello", 0.05, 0.45),
            word("world", 0.45, 0.95),
            word("next", 0.95, 1.21),
        ]));

        assert_eq!(update.stable.unwrap().text, "hello world next");
    }

    #[test]
    fn stable_text_is_kept_for_single_utterance_finalization() {
        let mut session = WhisperStreamingSession::default();
        session.accept_hypothesis(hypothesis(vec![
            word("final", 2.0, 2.4),
            word("words", 2.4, 2.9),
        ]));
        session.accept_hypothesis(hypothesis(vec![
            word("final", 2.0, 2.4),
            word("words", 2.4, 2.9),
        ]));

        assert_eq!(session.stable_text(), "final words");
    }

    #[test]
    fn final_tail_overlap_is_not_duplicated() {
        assert_eq!(
            merge_transcript_parts("we discussed the roadmap", "the roadmap yesterday"),
            "we discussed the roadmap yesterday"
        );
        assert_eq!(merge_transcript_parts("你好", "好世界"), "你好世界");
    }

    #[test]
    fn pending_streaming_audio_is_coalesced_without_losing_samples() {
        let mut pending = chunk(8_000, 2.0);
        let mut next = chunk(9_600, 2.5);
        next.chunk_id = 2;

        coalesce_streaming_audio(&mut pending, next);

        assert_eq!(pending.timestamp, 2.0);
        assert_eq!(pending.data.len(), 17_600);
        assert_eq!(pending.chunk_id, 2);
    }

    #[test]
    fn inference_window_is_bounded_to_fifteen_seconds() {
        let mut session = WhisperStreamingSession::default();
        session.push_audio(chunk(20 * 16_000, 0.0));

        let window = session.take_inference_window().unwrap();

        assert_eq!(window.samples.len(), 15 * 16_000);
        assert_eq!(window.start_time, 5.0);
    }

    #[test]
    fn confirmation_trims_audio_before_a_short_context_overlap() {
        let mut session = WhisperStreamingSession::default();
        session.push_audio(chunk(5 * 16_000, 0.0));
        session.accept_hypothesis(hypothesis(vec![
            word("one", 0.5, 1.0),
            word("two", 1.0, 2.0),
            word("tail", 2.0, 2.5),
        ]));
        session.accept_hypothesis(hypothesis(vec![
            word("one", 0.5, 1.0),
            word("two", 1.0, 2.0),
            word("changed", 2.0, 2.6),
        ]));

        session.push_audio(chunk(16_000, 5.0));
        let window = session.take_inference_window().unwrap();

        assert_eq!(window.start_time, 1.5);
        assert_eq!(window.samples.len(), ((6.0_f64 - 1.5) * 16_000.0) as usize);
    }

    #[test]
    fn inference_waits_for_one_second_of_new_audio() {
        let mut session = WhisperStreamingSession::default();
        session.push_audio(chunk(8_000, 0.0));
        assert!(session.take_inference_window().is_none());

        session.push_audio(chunk(8_000, 0.5));
        assert!(session.take_inference_window().is_some());
        assert!(session.take_inference_window().is_none());
    }

    #[test]
    fn inference_skips_a_silent_window() {
        let mut session = WhisperStreamingSession::default();
        let mut silence = chunk(16_000, 0.0);
        silence.data.fill(0.0);
        session.push_audio(silence);

        assert!(session.take_inference_window().is_none());
    }

    #[test]
    fn whisper_routes_streaming_audio_and_utterance_end() {
        assert_eq!(
            route_input(
                TranscriptionMode::WhisperStreaming,
                &TranscriptionInput::StreamingAudio(chunk(1_600, 0.0)),
            ),
            InputRoute::Stream
        );
        assert_eq!(
            route_input(
                TranscriptionMode::WhisperStreaming,
                &TranscriptionInput::UtteranceEnd(chunk(1_600, 0.0)),
            ),
            InputRoute::Finalize
        );
    }

    #[test]
    fn final_only_providers_ignore_streaming_audio() {
        assert_eq!(
            route_input(
                TranscriptionMode::FinalOnly,
                &TranscriptionInput::StreamingAudio(chunk(1_600, 0.0)),
            ),
            InputRoute::Ignore
        );
        assert_eq!(
            route_input(
                TranscriptionMode::FinalOnly,
                &TranscriptionInput::UtteranceEnd(chunk(1_600, 0.0)),
            ),
            InputRoute::Finalize
        );
    }

    #[test]
    fn prompt_contains_only_confirmed_words_before_the_audio_window() {
        let mut session = WhisperStreamingSession::default();
        session.push_audio(chunk(16_000, 0.0));
        session.accept_hypothesis(hypothesis(vec![word("old", 0.0, 0.5)]));
        session.accept_hypothesis(hypothesis(vec![word("old", 0.0, 0.5)]));

        assert_eq!(session.prompt(), "");

        session.push_audio(chunk(15 * 16_000, 1.0));
        assert_eq!(session.prompt(), "old");
    }
}
