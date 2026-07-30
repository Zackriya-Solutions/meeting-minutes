//! Typed artifacts for each LLM stage + the prompt builders that produce them.
//!
//! Every stage asks DeepSeek (JSON mode) for a single JSON object matching one of the
//! structs below. All text values are Russian; every `seg` is a 0-based transcript
//! segment index that maps to the `#t{seg}` anchor in the rendered HTML. Structs use
//! `#[serde(default)]` liberally so a slightly-off model response still parses instead
//! of failing the whole stage.

use serde::{Deserialize, Serialize};

use crate::report::dynamics::{Dynamics, TimedSegment};

/// Rough character budget for the transcript inside a prompt before defensive truncation.
const TRANSCRIPT_BUDGET: usize = 60_000;
const TRANSCRIPT_HEAD: usize = 45_000;
const TRANSCRIPT_TAIL: usize = 10_000;

// ============================ Stage artifacts ============================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Participant {
    #[serde(default)]
    pub speaker: String,
    #[serde(default)]
    pub role_hint: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Classification {
    #[serde(default)]
    pub meeting_type: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub participants: Vec<Participant>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClarifyQuestion {
    #[serde(default)]
    pub id: String,
    /// "ambiguity" | "context"
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub quote: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub affects: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Clarify {
    #[serde(default)]
    pub questions: Vec<ClarifyQuestion>,
}

/// One user answer to a clarify question. Sent by the frontend as
/// `{ "question_id": ..., "answer": ... }`; `answer` is null/omitted when skipped.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClarifyAnswer {
    #[serde(default)]
    pub question_id: String,
    #[serde(default)]
    pub answer: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Topic {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub start_s: f32,
    #[serde(default)]
    pub end_s: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgendaItem {
    #[serde(default)]
    pub item: String,
    /// "covered" | "partial" | "missed"
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub seg: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Topics {
    #[serde(default)]
    pub topics: Vec<Topic>,
    #[serde(default)]
    pub agenda: Vec<AgendaItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Decision {
    #[serde(default)]
    pub statement: String,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub quality_badges: Vec<String>,
    #[serde(default)]
    pub seg: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Decisions {
    #[serde(default)]
    pub decisions: Vec<Decision>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Commitment {
    #[serde(default)]
    pub who: String,
    #[serde(default)]
    pub what: String,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub seg: i64,
    /// "firm" | "hedged" | "vague"
    #[serde(default)]
    pub hedge: String,
    #[serde(default)]
    pub has_owner: bool,
    #[serde(default)]
    pub has_dod: bool,
    #[serde(default)]
    pub dod_note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Commitments {
    #[serde(default)]
    pub commitments: Vec<Commitment>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenThread {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub seg: i64,
    /// "info" | "warn" | "crit"
    #[serde(default)]
    pub severity: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Risk {
    #[serde(default)]
    pub text: String,
    /// "note" | "warn" | "serious" | "crit"
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub seg: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadsRisks {
    #[serde(default)]
    pub open_threads: Vec<OpenThread>,
    #[serde(default)]
    pub risks: Vec<Risk>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Position {
    #[serde(default)]
    pub who: String,
    #[serde(default)]
    pub stance: String,
    #[serde(default)]
    pub seg: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Disagreement {
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub positions: Vec<Position>,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub resolved: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProCon {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub seg: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConceptOption {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub pros: Vec<ProCon>,
    #[serde(default)]
    pub cons: Vec<ProCon>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Concept {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub options: Vec<ConceptOption>,
    #[serde(default)]
    pub resolution: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DisagreementsConcepts {
    #[serde(default)]
    pub disagreements: Vec<Disagreement>,
    #[serde(default)]
    pub concepts: Vec<Concept>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NumberItem {
    #[serde(default)]
    pub metric: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub seg: i64,
    #[serde(default)]
    pub check: String,
    /// "ok" | "warn" | "info"
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Numbers {
    #[serde(default)]
    pub numbers: Vec<NumberItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Role {
    #[serde(default)]
    pub speaker: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub seg: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Roles {
    #[serde(default)]
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Insight {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// "info" | "warn" | "serious" | "crit"
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub segs: Vec<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Insights {
    #[serde(default)]
    pub insights: Vec<Insight>,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub what_hindered: Vec<String>,
}

// ============================ Transcript formatting ============================

fn fmt_mmss(secs: f64) -> String {
    let s = secs.max(0.0).round() as i64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// Build the numbered transcript passed to extraction stages:
/// `[{index}|{mm:ss}] {speaker}: {text}`. `timed` and `segments` are parallel.
pub fn format_transcript(timed: &[TimedSegment], labels: &[String], texts: &[String]) -> String {
    let mut out = String::new();
    for (i, text) in texts.iter().enumerate() {
        let ts = timed.get(i).map(|t| t.start).unwrap_or(0.0);
        let speaker = labels.get(i).map(String::as_str).unwrap_or("Спикер");
        out.push_str(&format!(
            "[{}|{}] {}: {}\n",
            i,
            fmt_mmss(ts),
            speaker,
            text.trim()
        ));
    }
    out
}

/// Defensive truncation: if the transcript is very long, keep the head and tail with a
/// marker so segment indices in the head/tail remain valid anchors.
pub fn truncate_transcript(transcript: &str) -> String {
    if transcript.len() <= TRANSCRIPT_BUDGET {
        return transcript.to_string();
    }
    // Cut on char boundaries near the byte targets.
    let head_end = floor_char_boundary(transcript, TRANSCRIPT_HEAD);
    let tail_start =
        ceil_char_boundary(transcript, transcript.len().saturating_sub(TRANSCRIPT_TAIL));
    let mut out = String::with_capacity(TRANSCRIPT_HEAD + TRANSCRIPT_TAIL + 64);
    out.push_str(&transcript[..head_end]);
    out.push_str("\n[… середина встречи пропущена для экономии контекста …]\n");
    out.push_str(&transcript[tail_start..]);
    out
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

// ============================ Prompt builders ============================

/// Shared system preamble. Enforces language, JSON-only output, and the seg-anchor rule.
pub const SYSTEM_BASE: &str = "Ты — аналитик деловых встреч. Ты работаешь со стенограммой встречи на русском языке. \
Отвечай СТРОГО одним JSON-объектом по запрошенной схеме: без markdown, без пояснений, без текста вне JSON. \
Все текстовые значения — на русском языке. \
Каждое поле `seg` — это НОМЕР сегмента стенограммы (число в квадратных скобках вида [N|mm:ss] в начале реплики), к которому относится утверждение. \
Опирайся только на то, что реально сказано в стенограмме; ничего не выдумывай.";

fn stage_system(schema: &str) -> String {
    format!("{SYSTEM_BASE}\n\nСхема ответа (верни объект ровно с этими ключами):\n{schema}")
}

/// Append a stricter instruction for the single retry after a parse failure.
pub fn retry_suffix(user: &str) -> String {
    format!(
        "{user}\n\nВАЖНО: предыдущий ответ не удалось разобрать как JSON. \
Верни СТРОГО валидный JSON-объект по схеме, начни ответ с {{ и заверши }}. \
Без markdown-ограждений, без комментариев, без текста до или после JSON."
    )
}

pub fn classify(transcript: &str) -> (String, String) {
    let schema = "{ \"meeting_type\": строка (тип встречи по-русски, напр. \"статус/планёрка\", \"1:1\", \"продуктовая\"), \
\"confidence\": число 0..1, \
\"title\": строка (краткий осмысленный заголовок встречи по-русски), \
\"participants\": [ { \"speaker\": имя/метка спикера, \"role_hint\": краткая роль или зона ответственности } ] }";
    let user = format!(
        "Классифицируй встречу и восстанови участников по стенограмме.\n\nСтенограмма:\n{transcript}"
    );
    (stage_system(schema), user)
}

pub fn clarify(transcript: &str, classification_json: &str) -> (String, String) {
    let schema = "{ \"questions\": [ { \"id\": \"q1\", \"kind\": \"ambiguity\"|\"context\", \
\"text\": текст вопроса, \"quote\": спорная цитата из стенограммы или null, \
\"options\": [2–4 коротких варианта ответа], \"affects\": на что повлияет ответ или null } ] }";
    let user = format!(
        "Сформулируй ДО 5 уточняющих вопросов, ответы на которые РЕАЛЬНО изменят результат разбора. \
Если уточнять нечего — верни пустой список questions.\n\n\
Приоритет и правила:\n\
1) Неоднозначности стенограммы (kind = \"ambiguity\"): неясные ссылки на сущности, обязательства без владельца, \
решения без явного исхода, неатрибутированные имена. Для КАЖДОГО такого вопроса поле quote ОБЯЗАТЕЛЬНО — приведи спорную цитату.\n\
2) Контекст (kind = \"context\"): цель встречи или аудитория итоговых материалов — НЕ БОЛЕЕ ОДНОГО такого вопроса.\n\
У каждого вопроса 2–4 коротких варианта ответа (для тапа) плюс обязательно добавь буквальный вариант \"другое\".\n\n\
Классификация встречи (JSON):\n{classification_json}\n\nСтенограмма:\n{transcript}"
    );
    (stage_system(schema), user)
}

/// Build the "user clarifications (treat as established facts)" block appended to every
/// downstream stage prompt. Only non-empty answers are included; returns "" if none.
pub fn build_answers_block(questions: &[ClarifyQuestion], answers: &[ClarifyAnswer]) -> String {
    use std::collections::HashMap;
    let qmap: HashMap<&str, &str> = questions
        .iter()
        .map(|q| (q.id.as_str(), q.text.as_str()))
        .collect();
    let lines: Vec<String> = answers
        .iter()
        .filter_map(|a| {
            let ans = a
                .answer
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())?;
            let qtext = qmap.get(a.question_id.as_str()).copied().unwrap_or("");
            Some(format!("— {} → {}", qtext.trim(), ans))
        })
        .collect();
    if lines.is_empty() {
        String::new()
    } else {
        format!(
            "Уточнения пользователя (считать установленными фактами):\n{}",
            lines.join("\n")
        )
    }
}

/// Append a context block (e.g. user clarifications) to a stage's user prompt. No-op when
/// the block is empty.
pub fn with_context(user: &str, block: &str) -> String {
    if block.trim().is_empty() {
        user.to_string()
    } else {
        format!("{user}\n\n{block}")
    }
}

pub fn topics(transcript: &str, meeting_type: &str) -> (String, String) {
    let schema = "{ \"topics\": [ { \"name\": тема, \"start_s\": секунда начала, \"end_s\": секунда конца } ], \
\"agenda\": [ { \"item\": пункт повестки, \"status\": \"covered\"|\"partial\"|\"missed\", \"seg\": номер сегмента } ] }";
    let user = format!(
        "Раздели встречу на тематические блоки (topics) с приблизительными секундами по меткам времени. \
Затем восстанови подразумеваемую повестку (agenda) — что по типу встречи «{meeting_type}» и по её началу должно было быть обсуждено — \
и для каждого пункта укажи, был ли он раскрыт (covered), частично (partial) или упущен (missed).\n\nСтенограмма:\n{transcript}"
    );
    (stage_system(schema), user)
}

pub fn decisions(transcript: &str) -> (String, String) {
    let schema = "{ \"decisions\": [ { \"statement\": формулировка решения, \
\"rationale\": обоснование или null, \
\"quality_badges\": [короткие метки качества решения, напр. \"альтернатива рассмотрена\", \"обоснование есть\", \"владелец не назначен\"], \
\"seg\": номер сегмента } ] }";
    let user = format!("Извлеки принятые на встрече решения.\n\nСтенограмма:\n{transcript}");
    (stage_system(schema), user)
}

pub fn commitments(transcript: &str) -> (String, String) {
    let schema = "{ \"commitments\": [ { \"who\": кто взял обязательство, \"what\": что именно, \
\"due\": срок или null, \"quote\": короткая дословная цитата из стенограммы, \"seg\": номер сегмента, \
\"hedge\": \"firm\"|\"hedged\"|\"vague\" (насколько твёрдо звучит формулировка), \
\"has_owner\": true/false (назван ли конкретный исполнитель), \
\"has_dod\": true/false (понятен ли образ результата / критерий готовности), \
\"dod_note\": пояснение к образу результата или null } ] }";
    let user = format!(
        "Извлеки обязательства (кто что обещал сделать). \
`hedge` оценивает ТВЁРДОСТЬ ФОРМУЛИРОВКИ (глагол, срок, оговорки), а не человека.\n\nСтенограмма:\n{transcript}"
    );
    (stage_system(schema), user)
}

pub fn threads_risks(transcript: &str) -> (String, String) {
    let schema = "{ \"open_threads\": [ { \"text\": незакрытый вопрос/тема, \"seg\": номер сегмента, \"severity\": \"info\"|\"warn\"|\"crit\" } ], \
\"risks\": [ { \"text\": риск, \"severity\": \"note\"|\"warn\"|\"serious\"|\"crit\", \"seg\": номер сегмента } ] }";
    let user = format!(
        "Найди незакрытые вопросы (open_threads) — то, что повисло в воздухе без решения — и риски (risks), \
упомянутые вскользь или не закрытые.\n\nСтенограмма:\n{transcript}"
    );
    (stage_system(schema), user)
}

pub fn disagreements_concepts(transcript: &str) -> (String, String) {
    let schema = "{ \"disagreements\": [ { \"topic\": предмет спора, \
\"positions\": [ { \"who\": сторона, \"stance\": позиция, \"seg\": номер сегмента } ], \
\"resolution\": чем закончилось, \"resolved\": true/false } ], \
\"concepts\": [ { \"title\": обсуждавшийся вариант/концепция, \"outcome\": короткий итог, \
\"options\": [ { \"name\": название варианта, \
\"pros\": [ { \"text\": довод за, \"seg\": номер сегмента или null } ], \
\"cons\": [ { \"text\": довод против, \"seg\": номер сегмента или null } ] } ], \
\"resolution\": итоговое решение по концепции } ] }";
    let user = format!(
        "Извлеки разногласия (позиции сторон) и карточки концепций (варианты с плюсами и минусами, которые обсуждались).\n\nСтенограмма:\n{transcript}"
    );
    (stage_system(schema), user)
}

pub fn numbers(transcript: &str) -> (String, String) {
    let schema = "{ \"numbers\": [ { \"metric\": показатель, \"value\": значение (строкой, с единицами), \
\"seg\": номер сегмента, \"check\": краткий комментарий/проверка, \"status\": \"ok\"|\"warn\"|\"info\" } ] }";
    let user = format!(
        "Извлеки все числа и количественные утверждения встречи (метрики, объёмы, сроки, ресурсы).\n\nСтенограмма:\n{transcript}"
    );
    (stage_system(schema), user)
}

pub fn roles(transcript: &str) -> (String, String) {
    let schema = "{ \"roles\": [ { \"speaker\": спикер, \
\"role\": поведенческая роль по-русски (напр. фасилитатор, генератор решений, критик, навигатор бэклога, исполнитель-докладчик), \
\"evidence\": обоснование из стенограммы, \"seg\": номер сегмента } ] }";
    let user = format!(
        "Опиши поведенческие роли участников ИМЕННО НА ЭТОЙ ВСТРЕЧЕ (как они себя вели: кто вёл, кто предлагал, кто критиковал). \
Это описание поведения на встрече, а НЕ типология личности; роли не переносятся на человека.\n\nСтенограмма:\n{transcript}"
    );
    (stage_system(schema), user)
}

/// Insights: fed the compact artifact JSON + fast facts, NOT the raw transcript.
pub fn insights(artifacts_json: &str, fast_facts: &str) -> (String, String) {
    let schema =
        "{ \"insights\": [ { \"title\": заголовок наблюдения, \"body\": развёрнутое объяснение, \
\"severity\": \"info\"|\"warn\"|\"serious\"|\"crit\", \"category\": короткая категория, \
\"segs\": [номера сегментов-подтверждений] } ], \
\"verdict\": строка (одно предложение — вердикт по встрече), \
\"what_hindered\": [ровно 3 строки — что помешало встрече] }";
    let user = format!(
        "На основе уже извлечённых артефактов и фактов встречи выдай 3–6 НЕОЧЕВИДНЫХ наблюдений в поле insights. \
Фильтр новизны: каждое наблюдение ОБЯЗАНО быть таким, которое НЕЛЬЗЯ получить из простого пересказа встречи — \
это связки между репликами, системные паттерны, противоречия, повторы, инверсии. \
Отбрось всё, что и так очевидно из резюме. \
Также верни `verdict` (одна строка) и `what_hindered` (ровно 3 пункта).\n\n\
Факты встречи:\n{fast_facts}\n\nАртефакты (JSON):\n{artifacts_json}"
    );
    (stage_system(schema), user)
}

/// Compact human-readable fast facts for the insights stage (deterministic, no LLM text).
pub fn fast_facts(dynamics: &Dynamics) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "Длительность: {} · плотность речи: {}% · реплик-очередей: {} · вопросов всего: {} · пауз>10с: {}\n",
        fmt_mmss(dynamics.duration_secs),
        (dynamics.speech_density * 100.0).round() as i64,
        dynamics.turn_count,
        dynamics.total_questions,
        dynamics.pauses_over_10s,
    ));
    s.push_str("Доли речи: ");
    let parts: Vec<String> = dynamics
        .speakers
        .iter()
        .map(|sp| {
            format!(
                "{} — {}% ({} вопр.)",
                sp.label,
                (sp.talk_share * 100.0).round() as i64,
                sp.questions
            )
        })
        .collect();
    s.push_str(&parts.join(", "));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_is_numbered_with_timestamps() {
        let timed = vec![
            TimedSegment {
                start: 5.0,
                end: 7.0,
                speaker_key: "a".into(),
            },
            TimedSegment {
                start: 65.0,
                end: 66.0,
                speaker_key: "b".into(),
            },
        ];
        let labels = vec!["Аня".to_string(), "Боря".to_string()];
        let texts = vec!["Привет".to_string(), "Здравствуй".to_string()];
        let t = format_transcript(&timed, &labels, &texts);
        assert!(t.contains("[0|00:05] Аня: Привет"));
        assert!(t.contains("[1|01:05] Боря: Здравствуй"));
    }

    #[test]
    fn truncation_keeps_head_and_tail_on_char_boundaries() {
        let long = "я".repeat(50_000); // 2 bytes/char -> 100_000 bytes, over budget
        let out = truncate_transcript(&long);
        assert!(out.len() < long.len());
        assert!(out.contains("середина встречи пропущена"));
        // Must remain valid UTF-8 / not panic on slicing (implicitly проверено выше).
    }

    #[test]
    fn short_transcript_is_untouched() {
        let s = "[0|00:00] Аня: коротко\n";
        assert_eq!(truncate_transcript(s), s);
    }

    #[test]
    fn classify_prompt_contains_transcript_and_schema() {
        let (system, user) = classify("[0|00:00] Аня: тест\n");
        assert!(system.contains("meeting_type"));
        assert!(user.contains("[0|00:00] Аня: тест"));
    }

    #[test]
    fn answers_block_includes_only_non_empty_answers() {
        let questions = vec![
            ClarifyQuestion {
                id: "q1".into(),
                text: "Кто такой Андрей?".into(),
                ..Default::default()
            },
            ClarifyQuestion {
                id: "q2".into(),
                text: "Пропущенный вопрос".into(),
                ..Default::default()
            },
        ];
        let answers = vec![
            ClarifyAnswer {
                question_id: "q1".into(),
                answer: Some("Ведущий".into()),
            },
            ClarifyAnswer {
                question_id: "q2".into(),
                answer: None,
            },
        ];
        let block = build_answers_block(&questions, &answers);
        assert!(block.contains("установленными фактами"));
        assert!(block.contains("Кто такой Андрей? → Ведущий"));
        assert!(!block.contains("Пропущенный вопрос"));
    }

    #[test]
    fn answers_block_empty_when_all_skipped() {
        let questions = vec![ClarifyQuestion {
            id: "q1".into(),
            text: "?".into(),
            ..Default::default()
        }];
        let answers = vec![ClarifyAnswer {
            question_id: "q1".into(),
            answer: Some("   ".into()),
        }];
        assert!(build_answers_block(&questions, &answers).is_empty());
        assert_eq!(with_context("prompt", ""), "prompt");
    }

    #[test]
    fn clarify_parses_with_defaults_for_missing_fields() {
        // Missing quote/affects/options should default rather than fail.
        let raw = r#"{"questions":[{"id":"q1","kind":"ambiguity","text":"Кто?"}]}"#;
        let c: Clarify = serde_json::from_str(raw).unwrap();
        assert_eq!(c.questions.len(), 1);
        assert_eq!(c.questions[0].id, "q1");
        assert!(c.questions[0].quote.is_none());
        assert!(c.questions[0].options.is_empty());
    }
}
