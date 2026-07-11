use crate::audio::recording_state::AudioChunk;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const SAMPLE_RATE: usize = 16_000;
const INFERENCE_STEP_SAMPLES: usize = SAMPLE_RATE;
const MAX_WINDOW_SAMPLES: usize = SAMPLE_RATE * 15;
const CONFIRMED_TIME_TOLERANCE_SECONDS: f64 = 0.01;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamingWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub probability: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamingHypothesis {
    pub words: Vec<StreamingWord>,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StreamingSegment {
    pub text: String,
    pub audio_start_time: f64,
    pub audio_end_time: f64,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StreamingUpdate {
    pub confirmed: Option<StreamingSegment>,
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

        self.new_samples_since_inference = 0;
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

    pub fn accept_hypothesis(&mut self, hypothesis: StreamingHypothesis) -> StreamingUpdate {
        let current = self.unconfirmed_words(hypothesis.words);
        let common_len = self.common_prefix_len(&current);
        let confirmed_words = current[..common_len].to_vec();
        let remaining = current[common_len..].to_vec();

        let confirmed = self.segment_from_words(&confirmed_words, hypothesis.confidence);
        if let Some(last) = confirmed_words.last() {
            self.last_confirmed_end = Some(last.end);
            self.extend_prompt(&confirmed_words);
        }

        self.previous_unconfirmed = remaining.clone();
        self.preview = self.segment_from_words(&remaining, hypothesis.confidence);

        StreamingUpdate {
            confirmed,
            preview: self.preview.clone(),
        }
    }

    pub fn finish(&mut self, hypothesis: StreamingHypothesis) -> StreamingUpdate {
        let remaining = self.unconfirmed_words(hypothesis.words);
        let confirmed = self.segment_from_words(&remaining, hypothesis.confidence);
        if let Some(last) = remaining.last() {
            self.last_confirmed_end = Some(last.end);
            self.extend_prompt(&remaining);
        }

        self.audio.clear();
        self.audio_start_time = None;
        self.new_samples_since_inference = 0;
        self.previous_unconfirmed.clear();
        self.preview = None;

        StreamingUpdate {
            confirmed,
            preview: None,
        }
    }

    pub fn finish_relative(
        &mut self,
        mut hypothesis: StreamingHypothesis,
        window_start_time: f64,
    ) -> StreamingUpdate {
        for word in &mut hypothesis.words {
            word.start += window_start_time;
            word.end += window_start_time;
        }
        self.finish(hypothesis)
    }

    pub fn current_preview(&self) -> Option<&StreamingSegment> {
        self.preview.as_ref()
    }

    pub fn confirmed_end_time(&self) -> Option<f64> {
        self.last_confirmed_end
    }

    pub fn prompt(&self) -> String {
        self.prompt_for_start(self.audio_start_time.unwrap_or(f64::INFINITY))
    }

    pub fn prompt_for_start(&self, window_start_time: f64) -> String {
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
        self.preview = None;
        self.last_confirmed_end = None;
    }

    fn unconfirmed_words(&self, words: Vec<StreamingWord>) -> Vec<StreamingWord> {
        match self.last_confirmed_end {
            Some(last_end) => words
                .into_iter()
                .filter(|word| word.end > last_end + CONFIRMED_TIME_TOLERANCE_SECONDS)
                .collect(),
            None => words,
        }
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

    fn segment_from_words(
        &self,
        words: &[StreamingWord],
        fallback_confidence: f32,
    ) -> Option<StreamingSegment> {
        let first = words.first()?;
        let last = words.last()?;
        let probability_sum: f32 = words.iter().map(|word| word.probability).sum();
        let confidence = if words.is_empty() {
            fallback_confidence
        } else {
            probability_sum / words.len() as f32
        };

        Some(StreamingSegment {
            text: join_words(words),
            audio_start_time: first.start,
            audio_end_time: last.end,
            confidence,
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

        let attaches_to_previous = value
            .chars()
            .next()
            .is_some_and(|character| !character.is_alphanumeric());
        if !text.is_empty() && !attaches_to_previous {
            text.push(' ');
        }
        text.push_str(value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{
        route_input, InputRoute, StreamingHypothesis, StreamingWord, TranscriptionInput,
        TranscriptionMode, WhisperStreamingSession,
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
        StreamingHypothesis {
            words,
            confidence: 0.9,
        }
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
        assert!(first.confirmed.is_none());
        assert_eq!(first.preview.unwrap().text, "Hello world today");

        let second = session.accept_hypothesis(hypothesis(vec![
            word("Hello", 0.0, 0.4),
            word("world", 0.4, 0.8),
            word("tomorrow", 0.8, 1.3),
        ]));

        assert_eq!(second.confirmed.unwrap().text, "Hello world");
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

        assert_eq!(update.confirmed.unwrap().text, "hello");
        assert_eq!(update.preview.unwrap().text, "there");
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

        assert_eq!(update.confirmed.unwrap().text, "next");
    }

    #[test]
    fn finish_commits_the_remaining_hypothesis_and_clears_preview() {
        let mut session = WhisperStreamingSession::default();
        session.accept_hypothesis(hypothesis(vec![
            word("final", 2.0, 2.4),
            word("words", 2.4, 2.9),
        ]));

        let update = session.finish(hypothesis(vec![
            word("final", 2.0, 2.4),
            word("words", 2.4, 2.9),
        ]));

        assert_eq!(update.confirmed.unwrap().text, "final words");
        assert!(update.preview.is_none());
        assert!(session.current_preview().is_none());
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
    fn inference_waits_for_one_second_of_new_audio() {
        let mut session = WhisperStreamingSession::default();
        session.push_audio(chunk(8_000, 0.0));
        assert!(session.take_inference_window().is_none());

        session.push_audio(chunk(8_000, 0.5));
        assert!(session.take_inference_window().is_some());
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
