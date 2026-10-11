//! Speaker names from the conversation: the prompt for the summary model, reading its JSON
//! answer, and the checks that keep only names grounded in a quote from the transcript.
use crate::database::repositories::person::{clean_person_name, name_key};
use crate::summary::chunk_text;
use crate::summary::processor::{clean_llm_markdown_detailed, rough_token_count};
use serde::Deserialize;
use serde_json::Value;
use crate::database::repositories::setting::SettingsRepository;
use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::service::{is_local_model, resolve_llm, ResolvedLlm};
use sqlx::SqlitePool;
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

/// Longest name accepted from the model, in characters.
pub const MAX_NAME_CHARS: usize = 40;
/// Tokens kept free for the instructions, the summary and the answer when the transcript is cut
/// to the model's context.
pub const PROMPT_OVERHEAD_TOKENS: usize = 1000;
/// Smallest transcript chunk, in tokens, whatever the model's context.
const MIN_CHUNK_TOKENS: usize = 500;
const CHUNK_OVERLAP_TOKENS: usize = 100;
const UNREADABLE: &str = "The model's answer could not be read";

/// One labelled transcript row.
#[derive(Debug, Clone, PartialEq)]
pub struct NamingLine {
    pub start_s: f64,
    pub speaker: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamingSpeaker {
    pub key: String,
    pub display_name: Option<String>,
    /// A weak voice match is already proposed; it is kept over a conversation suggestion.
    pub has_voice_suggestion: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KnownPerson {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct NamingInput {
    pub lines: Vec<NamingLine>,
    pub summary: Option<String>,
    pub speakers: Vec<NamingSpeaker>,
    pub people: Vec<KnownPerson>,
    /// (speaker key, normalised name) pairs the user rejected in this meeting.
    pub rejected_names: HashSet<(String, String)>,
}

/// How the conversation names a speaker, weakest first: the order decides between proposals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    Mentioned,
    Addressed,
    SelfIntro,
}

impl ProposalKind {
    /// How the reason shown to the user puts it ("introduced as Noah at 00:05").
    fn verb(self) -> &'static str {
        match self {
            ProposalKind::SelfIntro => "introduced",
            ProposalKind::Addressed => "addressed",
            ProposalKind::Mentioned => "mentioned",
        }
    }
}

/// One entry of the model's answer.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Proposal {
    pub key: String,
    pub name: String,
    pub evidence: String,
    pub kind: ProposalKind,
    #[serde(default)]
    pub confidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionKind {
    /// Shown as the speaker's name, marked auto.
    Apply,
    /// Shown as "Speaker 2 · Noah?" until confirmed or rejected.
    Suggest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamingDecision {
    pub key: String,
    pub name: String,
    /// Set when the name is a known person's.
    pub person_id: Option<String>,
    pub kind: DecisionKind,
    pub reason: String,
}

/// The model that reads the transcript: the user's summary model in the app, a fake in tests.
#[async_trait::async_trait]
pub trait NamingModel: Send + Sync {
    async fn complete(&self, system: &str, user: &str) -> Result<String, String>;
    /// Usable context of the model, in tokens.
    fn context_tokens(&self) -> usize;
}

const SYSTEM_PROMPT: &str = r#"You find the real names of the speakers in a meeting transcript.
Each transcript line looks like "[MM:SS] spk_N: text", where spk_N is the speaker key.

Name a speaker only when a transcript line shows their name:
- "self_intro": the speaker says their own name ("I'm Noah", "this is Noah speaking").
- "addressed": another speaker talks to them by name ("Noah, where are you?") and they answer.
- "mentioned": the name is only talked about, or you are unsure who it belongs to.

For every speaker you can name, give:
- "key": the speaker key exactly as written, for example "spk_2"
- "name": the name as written in the transcript or the summary
- "evidence": one exact quote, copied from a single transcript line, that contains the name
- "kind": "self_intro", "addressed" or "mentioned"
- "confidence": "high" only when the quote leaves no doubt, otherwise "low"

Never guess or invent names. Leave out speakers you cannot name.
Answer with JSON only, in exactly this shape:
{"speakers": [{"key": "spk_2", "name": "Noah", "evidence": "Noah, where are you?", "kind": "addressed", "confidence": "high"}]}
If you cannot name anyone, answer {"speakers": []}."#;

fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0).floor() as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

/// One "[MM:SS] spk_N: text" line per labelled row.
pub fn transcript_text(lines: &[NamingLine]) -> String {
    lines
        .iter()
        .map(|l| format!("[{}] {}: {}", clock(l.start_s), l.speaker, l.text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// (system, user) prompts for one transcript chunk.
pub fn prompts(transcript: &str, summary: Option<&str>, people: &[KnownPerson]) -> (String, String) {
    let mut user = String::new();
    if !people.is_empty() {
        let names: Vec<&str> = people.iter().map(|p| p.name.as_str()).collect();
        user.push_str(&format!("People named in earlier meetings: {}\n\n", names.join(", ")));
    }
    if let Some(summary) = summary.map(str::trim).filter(|s| !s.is_empty()) {
        user.push_str(&format!("Meeting summary:\n{}\n\n", summary));
    }
    user.push_str(&format!("Transcript:\n{}", transcript));
    (SYSTEM_PROMPT.to_string(), user)
}

/// The first complete JSON object or array in `text`, whichever starts first.
fn outer_json(text: &str) -> Option<Value> {
    let mut starts: Vec<(usize, char)> = [('{', '}'), ('[', ']')]
        .iter()
        .filter_map(|&(open, close)| text.find(open).map(|i| (i, close)))
        .collect();
    starts.sort();
    starts.into_iter().find_map(|(start, close)| {
        let end = text.rfind(close)?;
        if end <= start {
            return None;
        }
        serde_json::from_str(&text[start..=end]).ok()
    })
}

/// Proposals in the model's answer. Reasoning (`<think>` blocks, or everything before a lone
/// `</think>`), code fences and surrounding prose are ignored; entries that do not fit the
/// expected shape are skipped.
pub fn parse_proposals(raw: &str) -> Result<Vec<Proposal>, String> {
    let cleaned = clean_llm_markdown_detailed(raw).markdown;
    // ASCII lowercasing keeps byte offsets, so the index is valid in `cleaned`.
    let visible = match cleaned.to_ascii_lowercase().rfind("</think>") {
        Some(i) => &cleaned[i + "</think>".len()..],
        None => cleaned.as_str(),
    };
    let items = match outer_json(visible) {
        Some(Value::Object(object)) => object.get("speakers").and_then(Value::as_array).cloned().unwrap_or_default(),
        Some(Value::Array(items)) => items,
        _ => return Err("the answer holds no JSON".to_string()),
    };
    Ok(items.into_iter().filter_map(|item| serde_json::from_value(item).ok()).collect())
}

/// `needle` occurs in `haystack` (both normalised) with no letter or digit right before or after.
fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    haystack.match_indices(needle).any(|(i, m)| {
        let before = haystack[..i].chars().next_back();
        let after = haystack[i + m.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// A name as stored for a person; None when empty or longer than MAX_NAME_CHARS.
fn clean_name(name: &str) -> Option<String> {
    clean_person_name(name).filter(|n| (1..=MAX_NAME_CHARS).contains(&n.chars().count()))
}

/// A proposal that passed every check.
struct Verified {
    key: String,
    name: String,
    person_id: Option<String>,
    kind: ProposalKind,
    apply: bool,
    line: usize,
    reason: String,
}

fn verify(input: &NamingInput, lines: &[String], summary: Option<&str>, p: &Proposal) -> Option<Verified> {
    let speaker = input.speakers.iter().find(|s| s.key == p.key)?;
    if speaker.display_name.is_some() {
        return None;
    }
    let name = clean_name(&p.name)?;
    let name_norm = name_key(&name);
    if !lines.iter().any(|l| contains_word(l, &name_norm)) && !summary.is_some_and(|s| contains_word(s, &name_norm)) {
        return None;
    }
    let quote_marks: &[char] = &['"', '\'', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}'];
    let evidence = name_key(p.evidence.trim().trim_matches(quote_marks));
    if evidence.is_empty() {
        return None;
    }
    // A short quote can occur more than once ("Noah?"): every line holding it is a candidate.
    let found: Vec<usize> =
        lines.iter().enumerate().filter(|(_, l)| contains_word(l, &evidence)).map(|(i, _)| i).collect();
    let first = *found.first()?;
    // The quote has to name the person: at least one word of the name is in it.
    if !name_norm.split(' ').any(|word| contains_word(&evidence, word)) {
        return None;
    }
    if input.rejected_names.contains(&(p.key.clone(), name_norm.clone())) {
        return None;
    }
    let person = input.people.iter().find(|k| name_key(&k.name) == name_norm);
    let name = person.map(|k| k.name.clone()).unwrap_or(name);
    // The kind's rule for the proposed speaker, at transcript line `i`.
    let fits = |i: usize| {
        let said = &input.lines[i];
        match p.kind {
            ProposalKind::SelfIntro => said.speaker == p.key,
            ProposalKind::Addressed => {
                said.speaker != p.key && input.lines.iter().skip(i + 1).take(2).any(|l| l.speaker == p.key)
            }
            ProposalKind::Mentioned => false,
        }
    };
    // The first occurrence that fits decides (and gives the time); else the first occurrence.
    let line = found.iter().copied().find(|&i| fits(i)).unwrap_or(first);
    let said = &input.lines[line];
    let high = p.confidence.trim().eq_ignore_ascii_case("high");
    let apply = high && fits(line);
    Some(Verified {
        key: p.key.clone(),
        reason: format!("{} as {name} at {}", p.kind.verb(), clock(said.start_s)),
        person_id: person.map(|k| k.id.clone()),
        name,
        kind: p.kind,
        apply,
        line,
    })
}

/// All proposals for one (speaker, name) pair.
struct Merged {
    key: String,
    name_key: String,
    best: Verified,
    quote_lines: BTreeSet<usize>,
}

impl Merged {
    fn add(&mut self, v: Verified) {
        self.quote_lines.insert(v.line);
        let better = v.kind > self.best.kind || (v.kind == self.best.kind && v.apply && !self.best.apply);
        if better {
            self.best = v;
        }
    }
}

/// Checks every proposal (spec §5.3) and picks one name per speaker and one speaker per name:
/// `self_intro` over `addressed` over `mentioned`, then the number of verified quotes.
pub fn decide(input: &NamingInput, proposals: &[Proposal]) -> Vec<NamingDecision> {
    let lines: Vec<String> = input.lines.iter().map(|l| name_key(&l.text)).collect();
    let summary = input.summary.as_deref().map(name_key);
    let mut merged: Vec<Merged> = Vec::new();
    for v in proposals.iter().filter_map(|p| verify(input, &lines, summary.as_deref(), p)) {
        let name_key = name_key(&v.name);
        match merged.iter_mut().find(|m| m.key == v.key && m.name_key == name_key) {
            Some(m) => m.add(v),
            None => merged.push(Merged {
                key: v.key.clone(),
                name_key,
                quote_lines: BTreeSet::from([v.line]),
                best: v,
            }),
        }
    }
    merged.sort_by(|a, b| {
        b.best.kind
            .cmp(&a.best.kind)
            .then(b.quote_lines.len().cmp(&a.quote_lines.len()))
            .then(b.best.apply.cmp(&a.best.apply))
            .then(a.quote_lines.first().cmp(&b.quote_lines.first()))
    });
    let mut keys = HashSet::new();
    // Names already on speakers of the meeting are taken: no second speaker gets them.
    let mut names: HashSet<String> =
        input.speakers.iter().filter_map(|s| s.display_name.as_deref()).map(name_key).collect();
    let mut decisions = Vec::new();
    for m in merged {
        if keys.contains(&m.key) || names.contains(&m.name_key) {
            continue;
        }
        let kind = if m.best.apply { DecisionKind::Apply } else { DecisionKind::Suggest };
        let voice_suggested = input.speakers.iter().any(|s| s.key == m.key && s.has_voice_suggestion);
        // A dropped suggestion reserves neither its speaker nor its name.
        if kind == DecisionKind::Suggest && voice_suggested {
            continue;
        }
        keys.insert(m.key.clone());
        names.insert(m.name_key.clone());
        decisions.push(NamingDecision {
            key: m.key,
            name: m.best.name,
            person_id: m.best.person_id,
            kind,
            reason: m.best.reason,
        });
    }
    decisions
}

/// Asks `model` about each transcript chunk and decides on the merged answers. A chunk whose
/// answer cannot be read is skipped; Err when every chunk failed or the model call failed.
pub async fn propose_names(model: &dyn NamingModel, input: &NamingInput) -> Result<Vec<NamingDecision>, String> {
    let transcript = transcript_text(&input.lines);
    if transcript.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Every chunk's prompt also carries the summary and the people list.
    let people_text: String = input.people.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
    let reserved = PROMPT_OVERHEAD_TOKENS
        + rough_token_count(input.summary.as_deref().unwrap_or_default())
        + rough_token_count(&people_text);
    let chunk_tokens = model.context_tokens().saturating_sub(reserved).max(MIN_CHUNK_TOKENS);
    let chunks = chunk_text(&transcript, chunk_tokens, CHUNK_OVERLAP_TOKENS);
    let mut proposals = Vec::new();
    let mut readable = 0usize;
    for chunk in &chunks {
        let (system, user) = prompts(chunk, input.summary.as_deref(), &input.people);
        let raw = model.complete(&system, &user).await?;
        match parse_proposals(&raw) {
            Ok(mut found) => {
                readable += 1;
                proposals.append(&mut found);
            }
            Err(e) => log::warn!("Skipping a naming answer that could not be read ({} chars): {}", raw.len(), e),
        }
    }
    if readable == 0 {
        return Err(UNREADABLE.to_string());
    }
    Ok(decide(input, &proposals))
}

const NO_MODEL: &str = "No summary model is configured";

/// The user's summary model, asked for the names said in a meeting.
pub struct SummaryModel {
    llm: ResolvedLlm,
    client: reqwest::Client,
}

impl SummaryModel {
    /// The transcript stays on this machine (see `is_local_model`).
    pub fn is_local(&self) -> bool {
        self.llm.is_local()
    }
}

#[async_trait::async_trait]
impl NamingModel for SummaryModel {
    async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let llm = &self.llm;
        // The shared client sends sampling settings only to custom OpenAI-compatible endpoints:
        // there, temperature 0 makes answers repeatable unless the user set a temperature. Other
        // providers answer at their default temperature.
        let temperature = llm.temperature.or(Some(0.0));
        generate_summary(
            &self.client,
            &llm.provider,
            &llm.model,
            &llm.api_key,
            system,
            user,
            llm.ollama_endpoint.as_deref(),
            llm.custom_openai_endpoint.as_deref(),
            llm.max_tokens,
            temperature,
            llm.top_p,
            llm.app_data_dir.as_ref(),
            None,
        )
        .await
        .map(|completion| completion.content)
    }

    fn context_tokens(&self) -> usize {
        self.llm.context_tokens
    }
}

/// The summary provider and model from the settings table. A custom OpenAI endpoint uses its
/// configured model, falling back to the settings model (as the frontend does).
pub async fn summary_model_from_settings(pool: &SqlitePool, app_data_dir: Option<PathBuf>) -> Result<SummaryModel, String> {
    let setting = SettingsRepository::get_model_config(pool)
        .await
        .map_err(|e| format!("Failed to read the summary model settings: {e}"))?
        .ok_or_else(|| NO_MODEL.to_string())?;
    let provider = setting.provider.trim().to_string();
    let mut model = setting.model.trim().to_string();
    if provider == "custom-openai" {
        match SettingsRepository::get_custom_openai_config(pool).await {
            Ok(Some(config)) if !config.model.trim().is_empty() => model = config.model.trim().to_string(),
            Ok(_) => {}
            Err(e) => return Err(format!("Failed to read the custom OpenAI settings: {e}")),
        }
    }
    if provider.is_empty() || model.is_empty() {
        return Err(NO_MODEL.to_string());
    }
    let llm = resolve_llm(pool, &provider, &model, app_data_dir).await?;
    Ok(SummaryModel { llm, client: reqwest::Client::new() })
}

/// Whether the saved summary model is local, from the same settings `resolve_llm` reads, without
/// resolving the model. False when no model is saved or the settings cannot be read.
pub async fn saved_model_is_local(pool: &SqlitePool) -> bool {
    let setting = match SettingsRepository::get_model_config(pool).await {
        Ok(Some(setting)) => setting,
        Ok(None) => return false,
        Err(e) => {
            log::warn!("Failed to read the summary model settings: {}", e);
            return false;
        }
    };
    let Ok(provider) = LLMProvider::from_str(setting.provider.trim()) else { return false };
    let custom_endpoint = if provider == LLMProvider::CustomOpenAI {
        match SettingsRepository::get_custom_openai_config(pool).await {
            Ok(config) => config.map(|c| c.endpoint),
            Err(e) => {
                log::warn!("Failed to read the custom OpenAI settings: {}", e);
                return false;
            }
        }
    } else {
        None
    };
    is_local_model(&provider, setting.ollama_endpoint.as_deref(), custom_endpoint.as_deref())
}
#[cfg(test)]
pub(crate) mod test_support {
    use super::NamingModel;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// Answers with the queued replies in order (then an empty list) and records every prompt.
    pub(crate) struct FakeNamingModel {
        answers: Mutex<VecDeque<Result<String, String>>>,
        prompts: Mutex<Vec<(String, String)>>,
        context: usize,
    }

    impl FakeNamingModel {
        pub(crate) fn new(context: usize, answers: Vec<Result<String, String>>) -> Self {
            Self { answers: Mutex::new(answers.into()), prompts: Mutex::new(Vec::new()), context }
        }

        /// (system, user) prompt of every call, in order.
        pub(crate) fn prompts(&self) -> Vec<(String, String)> {
            self.prompts.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl NamingModel for FakeNamingModel {
        async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
            self.prompts.lock().unwrap().push((system.to_string(), user.to_string()));
            self.answers.lock().unwrap().pop_front().unwrap_or_else(|| Ok(r#"{"speakers": []}"#.to_string()))
        }

        fn context_tokens(&self) -> usize {
            self.context
        }
    }

    /// A model answer with one entry per (key, name, evidence, kind, confidence).
    pub(crate) fn answer(entries: &[(&str, &str, &str, &str, &str)]) -> String {
        let speakers: Vec<serde_json::Value> = entries
            .iter()
            .map(|(key, name, evidence, kind, confidence)| {
                serde_json::json!({ "key": key, "name": name, "evidence": evidence, "kind": kind, "confidence": confidence })
            })
            .collect();
        serde_json::json!({ "speakers": speakers }).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{answer, FakeNamingModel};
    use super::ProposalKind::{Addressed, Mentioned, SelfIntro};
    use super::*;

    fn line(start_s: f64, speaker: &str, text: &str) -> NamingLine {
        NamingLine { start_s, speaker: speaker.into(), text: text.into() }
    }

    /// Noah introduces himself, Ana is asked and answers, Bea is asked but never answers and is
    /// then talked about, José and O'Brien are only talked about.
    fn conversation() -> Vec<NamingLine> {
        vec![
            line(5.0, "spk_0", "Hi everyone, I'm Noah and I run the platform team."),
            line(12.0, "spk_1", "Thanks Noah. Ana, can you start with the numbers?"),
            line(20.0, "spk_2", "Sure, the annual numbers look good."),
            line(31.0, "spk_1", "Great. Bea, are you there?"),
            line(35.0, "spk_0", "I think Bea had to step out."),
            line(41.0, "spk_2", "Has anyone heard from José or O\u{2019}Brien?"),
            line(50.0, "spk_3", "Sorry, I was on mute."),
        ]
    }

    fn base_input() -> NamingInput {
        NamingInput {
            lines: conversation(),
            summary: Some("Attendees: Noah Passalacqua, Ana Lima".into()),
            speakers: ["spk_0", "spk_1", "spk_2", "spk_3"]
                .iter()
                .map(|k| NamingSpeaker { key: k.to_string(), display_name: None, has_voice_suggestion: false })
                .collect(),
            ..Default::default()
        }
    }

    /// `base_input` plus enough rows without names that the transcript needs several chunks.
    fn with_filler(mut input: NamingInput) -> NamingInput {
        for i in 0..60 {
            input.lines.push(line(60.0 + i as f64, "spk_1", "We went through the remaining agenda items one by one."));
        }
        input
    }

    fn proposal(key: &str, name: &str, evidence: &str, kind: ProposalKind, confidence: &str) -> Proposal {
        Proposal { key: key.into(), name: name.into(), evidence: evidence.into(), kind, confidence: confidence.into() }
    }

    fn decision<'a>(decisions: &'a [NamingDecision], key: &str) -> Option<&'a NamingDecision> {
        decisions.iter().find(|d| d.key == key)
    }

    fn decided(key: &str, name: &str, kind: DecisionKind, reason: &str) -> NamingDecision {
        NamingDecision { key: key.into(), name: name.into(), person_id: None, kind, reason: reason.into() }
    }

    #[test]
    fn transcript_lines_use_minutes_seconds_and_keys() {
        let lines = vec![line(5.4, "spk_0", "  hello "), line(72.0, "spk_1", "hi"), line(3725.9, "spk_2", "late")];
        assert_eq!(transcript_text(&lines), "[00:05] spk_0: hello\n[01:12] spk_1: hi\n[62:05] spk_2: late");

        let people = [KnownPerson { id: "person-1".into(), name: "Ana".into() }];
        let (system, user) = prompts("[00:05] spk_0: hello", Some("Attendees: Noah"), &people);
        assert!(system.contains("JSON only"));
        assert!(system.contains("self_intro") && system.contains("addressed") && system.contains("mentioned"));
        assert!(user.contains("[00:05] spk_0: hello"));
        assert!(user.contains("Attendees: Noah"));
        assert!(user.contains("Ana"));
        let (_, bare) = prompts("[00:05] spk_0: hello", None, &[]);
        assert!(!bare.contains("Meeting summary"));
        assert!(!bare.contains("earlier meetings"));
    }

    #[test]
    fn parses_json_in_fences_prose_and_think_tags() {
        let entry = r#"{"key": "spk_0", "name": "Noah", "evidence": "I'm Noah", "kind": "self_intro", "confidence": "high"}"#;
        let expected = vec![proposal("spk_0", "Noah", "I'm Noah", SelfIntro, "high")];
        for raw in [
            format!("```json\n{{\"speakers\": [{entry}]}}\n```"),
            format!("Sure! Here are the speakers:\n{{\"speakers\": [{entry}]}}\nLet me know if you need more."),
            format!("<think>The user wants {{names}}. Noah introduces himself.</think>\n{{\"speakers\": [{entry}]}}"),
            format!("Noah introduces himself {{maybe}}.</think>{{\"speakers\": [{entry}]}}"),
            format!("[{entry}]"),
        ] {
            assert_eq!(parse_proposals(&raw).unwrap(), expected, "{raw}");
        }
        // An entry the app cannot use (unknown kind) is skipped; the rest is kept.
        let mixed = format!(r#"{{"speakers": [{entry}, {{"key": "spk_1", "name": "Ana", "evidence": "Ana", "kind": "guess"}}]}}"#);
        assert_eq!(parse_proposals(&mixed).unwrap(), expected);
        // A missing confidence reads as empty (never "high").
        let no_confidence = r#"{"speakers": [{"key": "spk_0", "name": "Noah", "evidence": "I'm Noah", "kind": "self_intro"}]}"#;
        assert_eq!(parse_proposals(no_confidence).unwrap()[0].confidence, "");
        assert_eq!(parse_proposals(r#"{"speakers": []}"#).unwrap(), vec![]);
        assert!(parse_proposals("I could not find any names.").is_err());
        assert!(parse_proposals("{\"speakers\": [").is_err());
    }

    #[test]
    fn quote_must_appear_in_one_line() {
        let input = base_input();
        // Spacing and case differ from the transcript: still the same quote.
        let ok = decide(&input, &[proposal("spk_0", "Noah", "i'm   NOAH and I RUN", SelfIntro, "high")]);
        assert_eq!(decision(&ok, "spk_0").unwrap().kind, DecisionKind::Apply);
        // Across two lines, not in the transcript, or not naming the person: dropped.
        for evidence in ["platform team. Thanks Noah", "Noah said hello to everyone", "I run the platform team", ""] {
            assert!(decide(&input, &[proposal("spk_0", "Noah", evidence, SelfIntro, "high")]).is_empty(), "{evidence:?}");
        }
    }

    #[test]
    fn repeated_quote_is_judged_on_the_occurrence_that_fits() {
        let mut input = base_input();
        input.lines = vec![
            line(5.0, "spk_1", "Noah?"),
            line(8.0, "spk_2", "He might be late."),
            line(10.0, "spk_2", "Let's start without him."),
            line(70.0, "spk_1", "Noah?"),
            line(72.0, "spk_0", "Yes, sorry, I'm here."),
        ];
        // Only the second "Noah?" is answered by spk_0.
        let ds = decide(&input, &[proposal("spk_0", "Noah", "Noah?", Addressed, "high")]);
        assert_eq!(ds, vec![decided("spk_0", "Noah", DecisionKind::Apply, "addressed as Noah at 01:10")]);
        // No occurrence fits spk_3, who never answers: a suggestion timed at the first one.
        let ds = decide(&input, &[proposal("spk_3", "Noah", "Noah?", Addressed, "high")]);
        assert_eq!(ds, vec![decided("spk_3", "Noah", DecisionKind::Suggest, "addressed as Noah at 00:05")]);
    }

    #[test]
    fn name_must_appear_as_whole_word() {
        let input = base_input();
        // "Ann" only occurs inside "annual".
        assert!(decide(&input, &[proposal("spk_2", "Ann", "the annual numbers look good", SelfIntro, "high")]).is_empty());
        // Apostrophes and accents: the transcript writes O’Brien with a typographic apostrophe.
        let ds = decide(
            &input,
            &[
                proposal("spk_2", "O'Brien", "heard from José or O'Brien", Mentioned, "high"),
                proposal("spk_3", "José", "Has anyone heard from José", Mentioned, "high"),
            ],
        );
        assert_eq!(decision(&ds, "spk_2").unwrap().name, "O'Brien");
        assert_eq!(decision(&ds, "spk_3").unwrap().name, "José");
        // A full name from the summary, with a quote that names the person.
        let ds = decide(&input, &[proposal("spk_0", "Noah Passalacqua", "I'm Noah", SelfIntro, "high")]);
        assert_eq!(decision(&ds, "spk_0").unwrap().name, "Noah Passalacqua");
        // A name that is not in the transcript or the summary.
        assert!(decide(&input, &[proposal("spk_0", "Noam", "I'm Noah", SelfIntro, "high")]).is_empty());
    }

    #[test]
    fn name_longer_than_40_chars_is_dropped() {
        let mut input = base_input();
        input.lines.push(line(60.0, "spk_3", "I'm Maximiliana Wolfeschlegelsteinhausenberger."));
        let long = "Maximiliana Wolfeschlegelsteinhausenberger";
        assert!(long.chars().count() > MAX_NAME_CHARS);
        assert!(decide(&input, &[proposal("spk_3", long, &format!("I'm {long}"), SelfIntro, "high")]).is_empty());
        assert!(decide(&input, &[proposal("spk_3", "   ", "I'm Maximiliana", SelfIntro, "high")]).is_empty());
        let ds = decide(&input, &[proposal("spk_3", "Maximiliana", "I'm Maximiliana", SelfIntro, "high")]);
        assert_eq!(decision(&ds, "spk_3").unwrap().kind, DecisionKind::Apply);
    }

    #[test]
    fn self_intro_by_the_speaker_is_applied() {
        let ds = decide(&base_input(), &[proposal("spk_0", "Noah", "I'm Noah", SelfIntro, "high")]);
        assert_eq!(ds, vec![decided("spk_0", "Noah", DecisionKind::Apply, "introduced as Noah at 00:05")]);
        // The same quote said by someone else only makes a suggestion.
        let ds = decide(&base_input(), &[proposal("spk_1", "Noah", "I'm Noah", SelfIntro, "high")]);
        assert_eq!(decision(&ds, "spk_1").unwrap().kind, DecisionKind::Suggest);
    }

    #[test]
    fn addressed_by_another_speaker_followed_by_reply_is_applied() {
        let ds = decide(&base_input(), &[proposal("spk_2", "Ana", "Ana, can you start with the numbers?", Addressed, "high")]);
        assert_eq!(ds, vec![decided("spk_2", "Ana", DecisionKind::Apply, "addressed as Ana at 00:12")]);
    }

    #[test]
    fn addressed_without_reply_within_two_lines_is_a_suggestion() {
        // Bea is asked at 00:31; spk_3 first speaks three lines later.
        let ds = decide(&base_input(), &[proposal("spk_3", "Bea", "Bea, are you there?", Addressed, "high")]);
        assert_eq!(ds, vec![decided("spk_3", "Bea", DecisionKind::Suggest, "addressed as Bea at 00:31")]);
        // A speaker cannot address themselves.
        let ds = decide(&base_input(), &[proposal("spk_1", "Bea", "Bea, are you there?", Addressed, "high")]);
        assert_eq!(decision(&ds, "spk_1").unwrap().kind, DecisionKind::Suggest);
    }

    #[test]
    fn mentioned_is_never_applied() {
        let ds = decide(&base_input(), &[proposal("spk_3", "Bea", "I think Bea had to step out.", Mentioned, "high")]);
        assert_eq!(ds, vec![decided("spk_3", "Bea", DecisionKind::Suggest, "mentioned as Bea at 00:35")]);
    }

    #[test]
    fn low_confidence_is_a_suggestion() {
        for confidence in ["low", "medium", ""] {
            let ds = decide(&base_input(), &[proposal("spk_0", "Noah", "I'm Noah", SelfIntro, confidence)]);
            assert_eq!(decision(&ds, "spk_0").unwrap().kind, DecisionKind::Suggest, "{confidence:?}");
        }
        let ds = decide(&base_input(), &[proposal("spk_0", "Noah", "I'm Noah", SelfIntro, "HIGH")]);
        assert_eq!(decision(&ds, "spk_0").unwrap().kind, DecisionKind::Apply);
    }

    #[test]
    fn named_speakers_and_rejected_pairs_are_skipped() {
        let mut input = base_input();
        input.speakers[0].display_name = Some("Noah".into());
        input.people = vec![KnownPerson { id: "person-ana".into(), name: "Ana".into() }];
        input.rejected_names.insert(("spk_2".into(), "ana".into()));
        let ds = decide(
            &input,
            &[
                proposal("spk_0", "Noah", "I'm Noah", SelfIntro, "high"),
                proposal("spk_2", "Ana", "Ana, can you start", Addressed, "high"),
                proposal("spk_9", "Bea", "Bea, are you there?", Addressed, "high"),
            ],
        );
        assert!(ds.is_empty(), "{ds:?}");
        // A name already on a speaker of the meeting is not given to another speaker.
        let ds = decide(&input, &[proposal("spk_2", "Noah", "Thanks Noah", Addressed, "high")]);
        assert!(ds.is_empty(), "{ds:?}");

        // A voice suggestion is kept over a conversation suggestion, but an applied name wins.
        let mut input = base_input();
        input.speakers[2].has_voice_suggestion = true;
        input.speakers[3].has_voice_suggestion = true;
        let ds = decide(
            &input,
            &[
                proposal("spk_3", "Bea", "Bea, are you there?", Addressed, "high"),
                proposal("spk_2", "Ana", "Ana, can you start", Addressed, "high"),
            ],
        );
        assert_eq!(ds, vec![decided("spk_2", "Ana", DecisionKind::Apply, "addressed as Ana at 00:12")]);
    }

    #[test]
    fn rejected_name_is_skipped_even_for_an_unknown_person() {
        let mut input = base_input();
        assert!(input.people.is_empty());
        input.rejected_names.insert(("spk_2".into(), "ana".into()));
        let ds = decide(&input, &[proposal("spk_2", "Ana", "Ana, can you start", Addressed, "high")]);
        assert!(ds.is_empty(), "{ds:?}");
        // Another speaker can still get that name.
        let ds = decide(&input, &[proposal("spk_1", "Ana", "Ana, can you start", Mentioned, "high")]);
        assert_eq!(decision(&ds, "spk_1").unwrap().name, "Ana");
    }

    #[test]
    fn quote_matches_whole_words_only() {
        let mut input = base_input();
        input.lines = vec![
            line(1.0, "spk_1", "Ana is out today."),
            line(2.0, "spk_0", "Okay."),
            line(3.0, "spk_1", "Our manager wants the report."),
            line(4.0, "spk_2", "Sure, I'll send it."),
        ];
        // "Ana" only fits the first line, which spk_2 does not answer: never applied via "manager".
        let ds = decide(&input, &[proposal("spk_2", "Ana", "Ana", Addressed, "high")]);
        assert_eq!(ds, vec![decided("spk_2", "Ana", DecisionKind::Suggest, "addressed as Ana at 00:01")]);
        input.lines = vec![line(1.0, "spk_0", "Let's look at the analytics.")];
        assert!(decide(&input, &[proposal("spk_0", "Ana", "Ana", SelfIntro, "high")]).is_empty());
    }

    #[test]
    fn one_name_per_speaker_and_one_speaker_per_name() {
        let ds = decide(
            &base_input(),
            &[
                proposal("spk_0", "Ana", "Ana, can you start", Mentioned, "high"),
                proposal("spk_0", "Noah", "I'm Noah", SelfIntro, "high"),
                proposal("spk_1", "Noah", "Thanks Noah", Addressed, "high"),
                proposal("spk_2", "Ana", "Ana, can you start", Addressed, "high"),
            ],
        );
        assert_eq!(ds.len(), 2, "{ds:?}");
        assert_eq!(decision(&ds, "spk_0").unwrap().name, "Noah");
        assert_eq!(decision(&ds, "spk_2").unwrap().name, "Ana");
        assert!(decision(&ds, "spk_1").is_none());
    }

    #[tokio::test]
    async fn chunks_merge_by_kind_then_quote_count() {
        let input = with_filler(base_input());
        let context = PROMPT_OVERHEAD_TOKENS + 500;
        let chunks = chunk_text(&transcript_text(&input.lines), 500, 100);
        assert!(chunks.len() >= 3, "the transcript must need several chunks");
        // First chunk: Noah only mentioned for spk_1; Bea mentioned for spk_2 and spk_3 (one quote each).
        let mut answers = vec![Ok(answer(&[
            ("spk_1", "Noah", "Thanks Noah", "mentioned", "high"),
            ("spk_2", "Bea", "Bea, are you there?", "mentioned", "high"),
            ("spk_3", "Bea", "Bea, are you there?", "mentioned", "high"),
        ]))];
        answers.extend((2..chunks.len()).map(|_| Ok(answer(&[]))));
        // Last chunk: Noah introduces himself as spk_0; a second quote for Bea as spk_3.
        answers.push(Ok(answer(&[
            ("spk_0", "Noah", "I'm Noah", "self_intro", "high"),
            ("spk_3", "Bea", "I think Bea had to step out.", "mentioned", "high"),
        ])));
        let model = FakeNamingModel::new(context, answers);
        let mut ds = propose_names(&model, &input).await.unwrap();
        ds.sort_by(|a, b| a.key.cmp(&b.key));
        assert_eq!(model.prompts().len(), chunks.len());
        assert_eq!(
            ds,
            vec![
                decided("spk_0", "Noah", DecisionKind::Apply, "introduced as Noah at 00:05"),
                decided("spk_3", "Bea", DecisionKind::Suggest, "mentioned as Bea at 00:31"),
            ]
        );
    }

    #[test]
    fn known_person_name_links_that_person() {
        let mut input = base_input();
        input.people = vec![KnownPerson { id: "person-noah".into(), name: "Noah".into() }];
        let ds = decide(&input, &[proposal("spk_0", "noah", "I'm Noah", SelfIntro, "high")]);
        assert_eq!(
            ds,
            vec![NamingDecision {
                key: "spk_0".into(),
                name: "Noah".into(),
                person_id: Some("person-noah".into()),
                kind: DecisionKind::Apply,
                reason: "introduced as Noah at 00:05".into(),
            }]
        );
    }

    #[tokio::test]
    async fn a_long_summary_and_people_list_shrink_the_transcript_chunks() {
        let mut input = with_filler(base_input());
        let context = PROMPT_OVERHEAD_TOKENS + 1500;
        let plain = FakeNamingModel::new(context, vec![]);
        propose_names(&plain, &input).await.unwrap();
        input.summary = Some("A long summary sentence. ".repeat(80));
        input.people = (0..40).map(|i| KnownPerson { id: format!("p{i}"), name: format!("Person Number {i}") }).collect();
        let heavy = FakeNamingModel::new(context, vec![]);
        propose_names(&heavy, &input).await.unwrap();
        assert!(
            heavy.prompts().len() > plain.prompts().len(),
            "{} chunks with a long summary, {} without",
            heavy.prompts().len(),
            plain.prompts().len()
        );
    }

    #[tokio::test]
    async fn long_transcripts_are_chunked_to_the_model_context() {
        let input = with_filler(base_input());
        let transcript = transcript_text(&input.lines);
        let chunks = chunk_text(&transcript, 600, 100);
        assert!(chunks.len() > 1);
        let reserved = crate::summary::processor::rough_token_count(input.summary.as_deref().unwrap());
        let model = FakeNamingModel::new(PROMPT_OVERHEAD_TOKENS + 600 + reserved, vec![]);
        assert_eq!(propose_names(&model, &input).await.unwrap(), vec![]);
        let prompts = model.prompts();
        assert_eq!(prompts.len(), chunks.len());
        for ((_, user), chunk) in prompts.iter().zip(&chunks) {
            assert!(user.contains(chunk.as_str()));
            assert!(user.contains("Attendees: Noah Passalacqua"));
        }
        // A tiny context still sends chunks of at least 500 tokens.
        let small = FakeNamingModel::new(10, vec![]);
        propose_names(&small, &input).await.unwrap();
        assert_eq!(small.prompts().len(), chunk_text(&transcript, 500, 100).len());
        // Nothing to read: the model is not called.
        let none = FakeNamingModel::new(8000, vec![]);
        assert_eq!(propose_names(&none, &NamingInput::default()).await.unwrap(), vec![]);
        assert!(none.prompts().is_empty());
    }

    #[tokio::test]
    async fn model_error_is_returned() {
        let model = FakeNamingModel::new(8000, vec![Err("Failed to send request to LLM: connection refused".into())]);
        assert_eq!(
            propose_names(&model, &base_input()).await,
            Err("Failed to send request to LLM: connection refused".to_string())
        );
    }

    #[tokio::test]
    async fn all_unreadable_chunks_is_an_error() {
        let model = FakeNamingModel::new(8000, vec![Ok("I could not find any names.".into())]);
        assert_eq!(propose_names(&model, &base_input()).await, Err("The model's answer could not be read".to_string()));
        // One readable chunk is enough: the unreadable ones are skipped.
        let input = with_filler(base_input());
        let n = chunk_text(&transcript_text(&input.lines), 500, 100).len();
        let mut answers: Vec<Result<String, String>> = vec![Ok("not json".into()); n - 1];
        answers.push(Ok(answer(&[("spk_0", "Noah", "I'm Noah", "self_intro", "high")])));
        let model = FakeNamingModel::new(PROMPT_OVERHEAD_TOKENS + 500, answers);
        let ds = propose_names(&model, &input).await.unwrap();
        assert_eq!(ds, vec![decided("spk_0", "Noah", DecisionKind::Apply, "introduced as Noah at 00:05")]);
    }

    use crate::database::repositories::setting::SettingsRepository;
    use crate::database::test_support::migrated_pool;
    use crate::summary::CustomOpenAIConfig;

    /// Reads one HTTP request; returns (head, body).
    async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, String) {
        use tokio::io::AsyncReadExt;
        let mut data = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let n = stream.read(&mut buffer).await.unwrap();
            assert_ne!(n, 0, "connection closed before the request was complete");
            data.extend_from_slice(&buffer[..n]);
            let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
            let head = String::from_utf8_lossy(&data[..end]).to_string();
            let length = head
                .lines()
                .find_map(|l| {
                    let (name, value) = l.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if data.len() >= end + 4 + length {
                return (head, String::from_utf8_lossy(&data[end + 4..end + 4 + length]).to_string());
            }
        }
    }

    #[tokio::test]
    async fn summary_model_requires_a_configured_model() {
        let pool = migrated_pool().await;
        assert_eq!(summary_model_from_settings(&pool, None).await.err(), Some("No summary model is configured".to_string()));
        SettingsRepository::save_model_config(&pool, "ollama", "  ", "large-v3", None).await.unwrap();
        assert_eq!(summary_model_from_settings(&pool, None).await.err(), Some("No summary model is configured".to_string()));
    }

    #[tokio::test]
    async fn summary_model_calls_the_configured_endpoint() {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            let body = r#"{"choices":[{"message":{"content":"{\"speakers\": []}"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            request
        });

        let pool = migrated_pool().await;
        SettingsRepository::save_custom_openai_config(
            &pool,
            &CustomOpenAIConfig {
                endpoint: format!("http://{address}"),
                api_key: Some("local-key".into()),
                model: "naming-model".into(),
                max_tokens: None,
                temperature: None,
                top_p: None,
            },
        )
        .await
        .unwrap();
        let model = summary_model_from_settings(&pool, None).await.ok().expect("the custom model resolves");
        assert_eq!(model.context_tokens(), 100_000);
        assert_eq!(model.complete("system text", "user text").await.unwrap(), r#"{"speakers": []}"#);

        let (head, body) = server.await.unwrap();
        assert!(head.starts_with("POST /chat/completions "), "{head}");
        assert!(head.to_ascii_lowercase().contains("authorization: bearer local-key"));
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["model"], "naming-model");
        assert_eq!(body["temperature"].as_f64(), Some(0.0));
        assert_eq!(body["messages"][0]["content"], "system text");
        assert_eq!(body["messages"][1]["content"], "user text");
    }
}
