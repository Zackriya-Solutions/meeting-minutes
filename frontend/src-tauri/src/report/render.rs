//! Stage 11: local score computation + self-contained HTML rendering.
//!
//! Rendering is plain string substitution into `template.html` (`<!--SECTION:x-->` for
//! HTML fragments, `/*DATA:x*/` for JS data literals) — no template-engine dependency.
//! ALL user/LLM-derived text is HTML-escaped ([`esc`]); JS data literals are emitted as
//! JSON with `<` neutralised so a transcript can never break out of the `<script>` tag.

use std::collections::HashMap;

use serde_json::json;

use crate::report::dynamics::{Dynamics, TimedSegment};
use crate::report::prompts::{
    AgendaItem, ClarifyAnswer, ClarifyQuestion, Classification, Commitment, Commitments, Concept,
    Decisions, Disagreement, DisagreementsConcepts, Insights, Numbers, Roles, ThreadsRisks, Topics,
};

const TEMPLATE: &str = include_str!("template.html");

// ============================ Score ============================

/// Deterministic meeting score and its five components (all 0..100).
///
/// `Deserialize` is here so the meeting screen can read the score back out of a completed
/// report's artifacts snapshot (see [`crate::report::sections`]).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Score {
    pub total: i64,
    pub coverage_pct: f64,
    pub owners_pct: f64,
    pub deadline_pct: f64,
    pub dod_pct: f64,
    pub qa_pct: f64,
}

fn agenda_coverage(agenda: &[AgendaItem]) -> f64 {
    if agenda.is_empty() {
        return 0.0;
    }
    let sum: f64 = agenda
        .iter()
        .map(|a| match a.status.as_str() {
            "covered" => 1.0,
            "partial" => 0.5,
            _ => 0.0,
        })
        .sum();
    sum / agenda.len() as f64 * 100.0
}

fn pct(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64 * 100.0
    }
}

/// Compute the meeting score from agenda coverage, commitment quality, and open threads.
/// `deadline` is derived from a commitment carrying a non-empty `due`.
pub fn compute_score(
    agenda: &[AgendaItem],
    commitments: &[Commitment],
    open_threads_len: usize,
) -> Score {
    let coverage = agenda_coverage(agenda);
    let owners = pct(
        commitments.iter().filter(|c| c.has_owner).count(),
        commitments.len(),
    );
    let deadline = pct(
        commitments
            .iter()
            .filter(|c| {
                c.due
                    .as_deref()
                    .map(|d| !d.trim().is_empty())
                    .unwrap_or(false)
            })
            .count(),
        commitments.len(),
    );
    let dod = pct(
        commitments.iter().filter(|c| c.has_dod).count(),
        commitments.len(),
    );
    let qa = (100.0 - 12.0 * open_threads_len as f64).clamp(0.0, 100.0);
    let total =
        (0.25 * coverage + 0.2 * owners + 0.15 * deadline + 0.15 * dod + 0.25 * qa).round() as i64;
    Score {
        total,
        coverage_pct: coverage,
        owners_pct: owners,
        deadline_pct: deadline,
        dod_pct: dod,
        qa_pct: qa,
    }
}

// ============================ Small helpers ============================

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn fmt_mmss(secs: f64) -> String {
    let s = secs.max(0.0).round() as i64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn palette_color(index: usize) -> &'static str {
    match index {
        0 => "var(--s1)",
        1 => "var(--s2)",
        2 => "var(--s3)",
        3 => "var(--s4)",
        _ => "var(--muted)",
    }
}

fn meter_color(p: f64) -> &'static str {
    if p >= 65.0 {
        "var(--s1)"
    } else if p >= 40.0 {
        "var(--warn)"
    } else {
        "var(--crit)"
    }
}

/// Provenance link for a segment index; empty string if the index is out of range.
fn pv(seg: i64, seg_times: &[f64]) -> String {
    if seg < 0 || seg as usize >= seg_times.len() {
        return String::new();
    }
    let t = seg_times[seg as usize];
    format!(" <a class=\"pv\" href=\"#t{}\">{}</a>", seg, fmt_mmss(t))
}

fn pv_opt(seg: Option<i64>, seg_times: &[f64]) -> String {
    seg.map(|s| pv(s, seg_times)).unwrap_or_default()
}

fn pv_segs(segs: &[i64], seg_times: &[f64]) -> String {
    segs.iter().map(|s| pv(*s, seg_times)).collect()
}

fn ghost(msg: &str) -> String {
    format!("<div class=\"ghost\">{}</div>", esc(msg))
}

fn preview(s: &str, n: usize) -> String {
    let t: String = s.trim().chars().take(n).collect();
    t
}

// ============================ Render input ============================

pub struct RenderInput<'a> {
    pub meeting_title: &'a str,
    pub date_str: &'a str,
    pub model: &'a str,
    pub generation_secs: f64,
    pub total_stages: i64,
    pub dynamics: &'a Dynamics,
    pub timed: &'a [TimedSegment],
    pub seg_labels: &'a [String],
    pub seg_texts: &'a [String],
    pub classification: Option<&'a Classification>,
    pub topics: Option<&'a Topics>,
    pub decisions: Option<&'a Decisions>,
    pub commitments: Option<&'a Commitments>,
    pub threads_risks: Option<&'a ThreadsRisks>,
    pub disagreements_concepts: Option<&'a DisagreementsConcepts>,
    pub numbers: Option<&'a Numbers>,
    pub roles: Option<&'a Roles>,
    pub insights: Option<&'a Insights>,
    pub score: &'a Score,
    /// Clarify questions shown to the user (empty when the stage produced none).
    pub clarify_questions: &'a [ClarifyQuestion],
    /// The user's answers to those questions (parallel by `question_id`).
    pub clarify_answers: &'a [ClarifyAnswer],
}

pub fn render_report(input: &RenderInput) -> String {
    let seg_times: Vec<f64> = input.timed.iter().map(|t| t.start).collect();

    // key -> palette index (position in the sorted speakers list)
    let key_to_index: HashMap<&str, usize> = input
        .dynamics
        .speakers
        .iter()
        .map(|s| (s.key.as_str(), s.palette_index))
        .collect();
    // label -> color (for LLM-provided names that match a known speaker)
    let label_to_color: HashMap<String, &'static str> = input
        .dynamics
        .speakers
        .iter()
        .map(|s| (s.label.to_lowercase(), palette_color(s.palette_index)))
        .collect();
    let color_for_name = |name: &str| -> &'static str {
        label_to_color
            .get(&name.to_lowercase())
            .copied()
            .unwrap_or("var(--muted)")
    };

    let title = input
        .classification
        .map(|c| c.title.trim())
        .filter(|t| !t.is_empty())
        .unwrap_or(input.meeting_title);

    let type_badge = match input.classification {
        Some(c) if !c.meeting_type.trim().is_empty() => format!(
            "<b>{}</b> · уверенность {:.2}",
            esc(&c.meeting_type),
            c.confidence
        ),
        _ => format!("<b>{}</b>", esc("встреча")),
    };

    let datedur = format!(
        "{} · {}",
        esc(input.date_str),
        fmt_mmss(input.dynamics.duration_secs)
    );

    // Participants: prefer classification, else fall back to talk-time speakers.
    let participants = build_participants(input, &color_for_name);

    // Score section
    let verdict = input
        .insights
        .map(|i| i.verdict.trim())
        .filter(|v| !v.is_empty())
        .unwrap_or("Автоматическая оценка качества встречи.");
    let score_hero = format!(
        "<div class=\"score-big\">{}<small> / 100</small></div><p class=\"verdict\">{}</p>",
        input.score.total,
        esc(verdict)
    );
    let score_meters = build_meters(input.score);
    let what_hindered = build_what_hindered(input.insights);
    let coverage = build_coverage(input.topics, &seg_times);
    let clarify_section = build_clarify(input.clarify_questions, input.clarify_answers);

    // LLM sections
    let insights_html = build_insights(input.insights, &seg_times);
    let decisions_html = build_decisions(input.decisions, &seg_times);
    let commitments_html = build_commitments(input.commitments, &seg_times);
    let open_html = build_open_threads(input.threads_risks, &seg_times);
    let disagreements_html =
        build_disagreements(input.disagreements_concepts, &seg_times, &color_for_name);
    let concepts_html = build_concepts(input.disagreements_concepts, &seg_times);
    let numbers_html = build_numbers(input.numbers, &seg_times);
    let risks_html = build_risks(input.threads_risks, &seg_times);

    // Dynamics
    let tiles = build_tiles(input);
    let talk_legend = build_talk_legend(input.dynamics);
    let roles_html = build_roles(input.roles, &seg_times, &color_for_name);
    let transcript_html = build_transcript(input, &key_to_index);
    let footer = build_footer(input);

    // JS data
    let (turns_json, spk_json, talk_json) = build_speaker_js(input, &key_to_index);
    let topics_json = build_topics_js(input.topics, input.dynamics.duration_secs);
    let markers_json = build_markers_js(input, &seg_times);
    let dur_json = (input.dynamics.duration_secs.round() as i64).to_string();

    let replacements: [(&str, String); 30] = [
        ("<!--SECTION:TITLE-->", esc(title)),
        ("<!--SECTION:TYPE_BADGE-->", type_badge),
        ("<!--SECTION:DATEDUR-->", datedur),
        ("<!--SECTION:SCORE_BADGE-->", input.score.total.to_string()),
        ("<!--SECTION:PARTICIPANTS-->", participants),
        ("<!--SECTION:SCORE_HERO-->", score_hero),
        ("<!--SECTION:SCORE_METERS-->", score_meters),
        ("<!--SECTION:WHAT_HINDERED-->", what_hindered),
        ("<!--SECTION:COVERAGE-->", coverage),
        ("<!--SECTION:CLARIFY-->", clarify_section),
        ("<!--SECTION:INSIGHTS-->", insights_html),
        ("<!--SECTION:DECISIONS-->", decisions_html),
        ("<!--SECTION:COMMITMENTS-->", commitments_html),
        ("<!--SECTION:OPEN-->", open_html),
        ("<!--SECTION:DISAGREEMENTS-->", disagreements_html),
        ("<!--SECTION:CONCEPTS-->", concepts_html),
        ("<!--SECTION:NUMBERS-->", numbers_html),
        ("<!--SECTION:RISKS-->", risks_html),
        ("<!--SECTION:TILES-->", tiles),
        ("<!--SECTION:TALK_LEGEND-->", talk_legend),
        ("<!--SECTION:ROLES-->", roles_html),
        (
            "<!--SECTION:TRANSCRIPT_COUNT-->",
            input.seg_texts.len().to_string(),
        ),
        ("<!--SECTION:TRANSCRIPT-->", transcript_html),
        ("<!--SECTION:FOOTER-->", footer),
        ("/*DATA:TURNS*/", turns_json),
        ("/*DATA:SPK*/", spk_json),
        ("/*DATA:DUR*/", dur_json),
        ("/*DATA:TOPICS*/", topics_json),
        ("/*DATA:MARKERS*/", markers_json),
        ("/*DATA:TALK*/", talk_json),
    ];

    apply_template(TEMPLATE, &replacements)
}

/// Substitute every `(token, value)` pair in one left-to-right pass over the template.
/// Unlike chained whole-string `String::replace`, substituted values are never
/// rescanned, so a value that happens to contain another token's literal text (e.g. a
/// transcript segment where someone read `/*DATA:TURNS*/` aloud) is left intact.
fn apply_template(template: &str, replacements: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len() * 2);
    let mut rest = template;
    loop {
        let hit = replacements
            .iter()
            .filter_map(|(token, value)| rest.find(token).map(|pos| (pos, *token, value)))
            .min_by_key(|(pos, _, _)| *pos);
        match hit {
            Some((pos, token, value)) => {
                out.push_str(&rest[..pos]);
                out.push_str(value);
                rest = &rest[pos + token.len()..];
            }
            None => {
                out.push_str(rest);
                return out;
            }
        }
    }
}

// ============================ Section builders ============================

fn build_participants(
    input: &RenderInput,
    color_for_name: &dyn Fn(&str) -> &'static str,
) -> String {
    if let Some(c) = input.classification {
        if !c.participants.is_empty() {
            return c
                .participants
                .iter()
                .map(|p| {
                    let role = if p.role_hint.trim().is_empty() {
                        String::new()
                    } else {
                        format!("&nbsp;<small>{}</small>", esc(&p.role_hint))
                    };
                    format!(
                        "<span class=\"pers\"><span class=\"dot\" style=\"background:{}\"></span>{}{}</span>",
                        color_for_name(&p.speaker),
                        esc(&p.speaker),
                        role
                    )
                })
                .collect();
        }
    }
    // Fallback: talk-time speakers.
    input
        .dynamics
        .speakers
        .iter()
        .map(|s| {
            format!(
                "<span class=\"pers\"><span class=\"dot\" style=\"background:{}\"></span>{}</span>",
                palette_color(s.palette_index),
                esc(&s.label)
            )
        })
        .collect()
}

fn build_meters(score: &Score) -> String {
    let rows = [
        ("Покрытие повестки", score.coverage_pct),
        ("Задачи с владельцем", score.owners_pct),
        ("Задачи со сроком", score.deadline_pct),
        ("Образ результата", score.dod_pct),
        ("Закрытые вопросы", score.qa_pct),
    ];
    rows.iter()
        .map(|(label, p)| {
            let color = meter_color(*p);
            let v = p.round() as i64;
            format!(
                "<div class=\"meter-row\"><div class=\"meter-lbl\">{}</div>\
                 <div class=\"meter\" style=\"background:color-mix(in srgb,{} 15%,var(--surface))\">\
                 <i style=\"width:{}%;background:{}\"></i></div><div class=\"meter-val\">{}</div></div>",
                label, color, v, color, v
            )
        })
        .collect()
}

fn build_what_hindered(insights: Option<&Insights>) -> String {
    match insights {
        Some(i) if !i.what_hindered.is_empty() => i
            .what_hindered
            .iter()
            .map(|b| format!("<li>{}</li>", esc(b)))
            .collect(),
        _ => "<li class=\"mini\">Нет данных.</li>".to_string(),
    }
}

fn build_coverage(topics: Option<&Topics>, seg_times: &[f64]) -> String {
    let Some(t) = topics else {
        return ghost("Не удалось построить повестку.");
    };
    if t.agenda.is_empty() {
        return ghost("Повестку восстановить не удалось.");
    }
    t.agenda
        .iter()
        .map(|a| {
            let (cls, lbl) = match a.status.as_str() {
                "covered" => ("good", "пройдено"),
                "partial" => ("warn", "частично"),
                _ => ("", "упущено"),
            };
            format!(
                "<div class=\"ci\"><span class=\"st {}\">{}</span><span>{}{}</span></div>",
                cls,
                lbl,
                esc(&a.item),
                pv(a.seg, seg_times)
            )
        })
        .collect()
}

/// The «Уточнения» section: the questions asked during the build and the user's answers.
/// Renders nothing (empty string) when no questions were asked.
fn build_clarify(questions: &[ClarifyQuestion], answers: &[ClarifyAnswer]) -> String {
    if questions.is_empty() {
        return String::new();
    }
    let answer_of: HashMap<&str, &str> = answers
        .iter()
        .filter_map(|a| {
            a.answer
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|ans| (a.question_id.as_str(), ans))
        })
        .collect();

    let items: String = questions
        .iter()
        .map(|q| {
            let ans = answer_of
                .get(q.id.as_str())
                .map(|a| esc(a))
                .unwrap_or_else(|| "<span class=\"mini\">пропущено</span>".to_string());
            let quote = q
                .quote
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(|s| format!(" <span class=\"mini\">«{}»</span>", esc(s)))
                .unwrap_or_default();
            format!("<li><b>{}</b>{} — {}</li>", esc(&q.text), quote, ans)
        })
        .collect();

    format!(
        "<section class=\"card\" id=\"clarify\">\
         <div class=\"sec-head\"><div><span class=\"kicker\">ответы пользователя учтены в разборе</span>\
         <h2>Уточнения</h2></div></div>\
         <ul class=\"clean\">{items}</ul></section>"
    )
}

fn severity_class(sev: &str) -> &'static str {
    match sev {
        "crit" => "crit",
        "serious" => "serious",
        "warn" => "warn",
        _ => "",
    }
}

fn build_insights(insights: Option<&Insights>, seg_times: &[f64]) -> String {
    let note = "<p class=\"note novelty\">Неочевидные наблюдения — то, чего нет в обычном резюме. \
        Клик по времени открывает момент в записи.</p>";
    let Some(i) = insights else {
        return format!("{note}{}", ghost("Не удалось построить раздел."));
    };
    if i.insights.is_empty() {
        return format!("{note}{}", ghost("Наблюдений не выделено."));
    }
    let blocks: String = i
        .insights
        .iter()
        .enumerate()
        .map(|(n, ins)| {
            let cat = if ins.category.trim().is_empty() {
                "наблюдение".to_string()
            } else {
                esc(&ins.category)
            };
            format!(
                "<div class=\"insight\"><span class=\"n\">{:02}</span><div>\
                 <span class=\"st {}\">{}</span><h3>{}</h3><p>{}{}</p></div></div>",
                n + 1,
                severity_class(&ins.severity),
                cat,
                esc(&ins.title),
                esc(&ins.body),
                pv_segs(&ins.segs, seg_times)
            )
        })
        .collect();
    format!("{note}{blocks}")
}

fn build_decisions(decisions: Option<&Decisions>, seg_times: &[f64]) -> String {
    let Some(d) = decisions else {
        return ghost("Не удалось построить раздел.");
    };
    if d.decisions.is_empty() {
        return ghost("Явных решений не зафиксировано.");
    }
    d.decisions
        .iter()
        .enumerate()
        .map(|(n, dec)| {
            let rationale = dec
                .rationale
                .as_deref()
                .filter(|r| !r.trim().is_empty())
                .map(|r| format!("<p>{}</p>", esc(r)))
                .unwrap_or_default();
            let badges: String = dec
                .quality_badges
                .iter()
                .map(|b| format!("<span class=\"qb\">{}</span>", esc(b)))
                .collect();
            let badges_wrap = if badges.is_empty() {
                String::new()
            } else {
                format!("<div class=\"badges\">{badges}</div>")
            };
            format!(
                "<div class=\"dec\"><h3>{}. {}{}</h3>{}{}</div>",
                n + 1,
                esc(&dec.statement),
                pv(dec.seg, seg_times),
                rationale,
                badges_wrap
            )
        })
        .collect()
}

fn hedge_tag(hedge: &str) -> (&'static str, &'static str) {
    match hedge {
        "firm" => ("h1", "твёрдое"),
        "hedged" => ("h2", "с оговоркой"),
        "vague" => ("h3", "расплывчато"),
        _ => ("h2", "—"),
    }
}

fn build_commitments(commitments: Option<&Commitments>, seg_times: &[f64]) -> String {
    let Some(c) = commitments else {
        return ghost("Не удалось построить раздел.");
    };
    if c.commitments.is_empty() {
        return ghost("Обязательств не зафиксировано.");
    }
    let rows: String = c
        .commitments
        .iter()
        .map(|cm| {
            let due = cm
                .due
                .as_deref()
                .filter(|d| !d.trim().is_empty())
                .map(esc)
                .unwrap_or_else(|| "—".to_string());
            let (hcls, hlbl) = hedge_tag(&cm.hedge);
            let (dcls, dlbl) = if cm.has_dod {
                ("good", "есть")
            } else {
                ("warn", "нет")
            };
            let dod_note = cm
                .dod_note
                .as_deref()
                .filter(|d| !d.trim().is_empty())
                .map(|d| format!(" {}", esc(d)))
                .unwrap_or_default();
            format!(
                "<tr><td><b>{}</b></td><td>{}</td><td class=\"num\">{}</td>\
                 <td>«{}»{}</td><td><span class=\"hedge {}\">{}</span></td>\
                 <td><span class=\"st {}\">{}</span>{}</td></tr>",
                esc(&cm.who),
                esc(&cm.what),
                due,
                esc(&cm.quote),
                pv(cm.seg, seg_times),
                hcls,
                hlbl,
                dcls,
                dlbl,
                dod_note
            )
        })
        .collect();
    format!(
        "<div class=\"tbl-scroll\"><table>\
         <tr><th>Кто</th><th>Что</th><th>Срок</th><th>Формулировка</th><th>Твёрдость</th><th>Образ результата</th></tr>\
         {rows}</table></div>"
    )
}

fn build_open_threads(tr: Option<&ThreadsRisks>, seg_times: &[f64]) -> String {
    let Some(t) = tr else {
        return ghost("Не удалось построить раздел.");
    };
    if t.open_threads.is_empty() {
        return ghost("Незакрытых вопросов не найдено.");
    }
    let items: String = t
        .open_threads
        .iter()
        .map(|ot| {
            let tag = match ot.severity.as_str() {
                "crit" => " <span class=\"st crit\">критично</span>",
                "warn" => " <span class=\"st warn\">внимание</span>",
                _ => "",
            };
            format!("<li>{}{}{}</li>", esc(&ot.text), pv(ot.seg, seg_times), tag)
        })
        .collect();
    format!("<ul class=\"clean\">{items}</ul>")
}

fn build_disagreements(
    dc: Option<&DisagreementsConcepts>,
    seg_times: &[f64],
    color_for_name: &dyn Fn(&str) -> &'static str,
) -> String {
    let Some(d) = dc else {
        return ghost("Не удалось построить раздел.");
    };
    if d.disagreements.is_empty() {
        return ghost("Явных разногласий не зафиксировано.");
    }
    d.disagreements
        .iter()
        .map(|dis: &Disagreement| {
            let (rcls, rlbl) = if dis.resolved {
                ("good", "решено")
            } else {
                ("warn", "не решено")
            };
            let positions: String = dis
                .positions
                .iter()
                .map(|p| {
                    format!(
                        "<span class=\"who\"><span class=\"dot\" style=\"background:{}\"></span>{}</span>\
                         <span>{}{}</span>",
                        color_for_name(&p.who),
                        esc(&p.who),
                        esc(&p.stance),
                        pv(p.seg, seg_times)
                    )
                })
                .collect();
            let resolution = if dis.resolution.trim().is_empty() {
                String::new()
            } else {
                format!("<p class=\"resolution\">Итог: {}</p>", esc(&dis.resolution))
            };
            format!(
                "<div class=\"disagree\"><h3>{} <span class=\"st {}\">{}</span></h3>\
                 <div class=\"pos\">{}</div>{}</div>",
                esc(&dis.topic),
                rcls,
                rlbl,
                positions,
                resolution
            )
        })
        .collect()
}

fn build_concepts(dc: Option<&DisagreementsConcepts>, seg_times: &[f64]) -> String {
    let Some(d) = dc else {
        return ghost("Не удалось построить раздел.");
    };
    if d.concepts.is_empty() {
        return ghost("Развёрнутых концепций не обсуждалось.");
    }
    d.concepts
        .iter()
        .map(|c: &Concept| {
            let outcome = if c.outcome.trim().is_empty() {
                String::new()
            } else {
                format!(" <span class=\"st\">{}</span>", esc(&c.outcome))
            };
            let options: String = c
                .options
                .iter()
                .map(|opt| {
                    let pros: String = opt
                        .pros
                        .iter()
                        .map(|p| format!("<li>{}{}</li>", esc(&p.text), pv_opt(p.seg, seg_times)))
                        .collect();
                    let cons: String = opt
                        .cons
                        .iter()
                        .map(|p| format!("<li>{}{}</li>", esc(&p.text), pv_opt(p.seg, seg_times)))
                        .collect();
                    format!(
                        "<div><h4>{}</h4><ul>{}</ul><h4>против</h4><ul>{}</ul></div>",
                        esc(&opt.name),
                        pros,
                        cons
                    )
                })
                .collect();
            let resolution = if c.resolution.trim().is_empty() {
                String::new()
            } else {
                format!("<p class=\"resolution\">Итог: {}</p>", esc(&c.resolution))
            };
            format!(
                "<div class=\"concept\"><h3>{}{}</h3><div class=\"pc\">{}</div>{}</div>",
                esc(&c.title),
                outcome,
                options,
                resolution
            )
        })
        .collect()
}

fn build_numbers(numbers: Option<&Numbers>, seg_times: &[f64]) -> String {
    let Some(n) = numbers else {
        return ghost("Не удалось построить раздел.");
    };
    if n.numbers.is_empty() {
        return ghost("Числовых утверждений не найдено.");
    }
    let rows: String = n
        .numbers
        .iter()
        .map(|num| {
            let status = match num.status.as_str() {
                "ok" => "good",
                "warn" => "warn",
                _ => "",
            };
            let check = if num.check.trim().is_empty() {
                String::new()
            } else {
                format!("<span class=\"st {}\">{}</span>", status, esc(&num.check))
            };
            format!(
                "<tr><td>{}</td><td class=\"num\">{}</td><td>{}</td><td>{}</td></tr>",
                esc(&num.metric),
                esc(&num.value),
                pv(num.seg, seg_times),
                check
            )
        })
        .collect();
    format!(
        "<div class=\"tbl-scroll\"><table>\
         <tr><th>Показатель</th><th>Значение</th><th>Момент</th><th>Комментарий</th></tr>{rows}</table></div>"
    )
}

fn build_risks(tr: Option<&ThreadsRisks>, seg_times: &[f64]) -> String {
    let Some(t) = tr else {
        return ghost("Не удалось построить раздел.");
    };
    if t.risks.is_empty() {
        return ghost("Явных рисков не выделено.");
    }
    let items: String = t
        .risks
        .iter()
        .map(|r| {
            let (cls, lbl) = match r.severity.as_str() {
                "crit" => ("crit", "критично"),
                "serious" => ("serious", "серьёзно"),
                "warn" => ("warn", "внимание"),
                _ => ("", "заметка"),
            };
            format!(
                "<li><span class=\"st {}\">{}</span> {}{}</li>",
                cls,
                lbl,
                esc(&r.text),
                pv(r.seg, seg_times)
            )
        })
        .collect();
    format!("<ul class=\"clean\">{items}</ul>")
}

fn build_tiles(input: &RenderInput) -> String {
    let d = input.dynamics;
    let decisions_n = input
        .decisions
        .map(|x| x.decisions.len().to_string())
        .unwrap_or_else(|| "—".to_string());
    let commitments_n = input
        .commitments
        .map(|x| x.commitments.len().to_string())
        .unwrap_or_else(|| "—".to_string());
    let density = (d.speech_density * 100.0).round() as i64;
    format!(
        "<div class=\"tile\"><div class=\"lbl\">Длительность</div><div class=\"val\">{}</div><div class=\"sub\">плотность речи {}%</div></div>\
         <div class=\"tile\"><div class=\"lbl\">Решений</div><div class=\"val\">{}</div></div>\
         <div class=\"tile\"><div class=\"lbl\">Обязательств</div><div class=\"val\">{}</div></div>\
         <div class=\"tile\"><div class=\"lbl\">Вопросов</div><div class=\"val\">{}</div></div>\
         <div class=\"tile\"><div class=\"lbl\">Реплик-очередей</div><div class=\"val\">{}</div></div>\
         <div class=\"tile\"><div class=\"lbl\">Пауз &gt; 10 с</div><div class=\"val\">{}</div><div class=\"sub\">&gt; 3 с: {}</div></div>",
        fmt_mmss(d.duration_secs),
        density,
        decisions_n,
        commitments_n,
        d.total_questions,
        d.turn_count,
        d.pauses_over_10s,
        d.pauses_over_3s
    )
}

fn build_talk_legend(d: &Dynamics) -> String {
    if d.speakers.is_empty() {
        return "Нет данных о речи.".to_string();
    }
    let parts: Vec<String> = d
        .speakers
        .iter()
        .map(|s| format!("{} — {}", esc(&s.label), s.questions))
        .collect();
    format!("Вопросы по спикерам: {}.", parts.join(", "))
}

fn build_roles(
    roles: Option<&Roles>,
    seg_times: &[f64],
    color_for_name: &dyn Fn(&str) -> &'static str,
) -> String {
    let Some(r) = roles else {
        return ghost("Не удалось построить раздел.");
    };
    if r.roles.is_empty() {
        return ghost("Роли определить не удалось.");
    }
    r.roles
        .iter()
        .map(|role| {
            format!(
                "<div class=\"role\"><div class=\"rname\"><span class=\"dot\" style=\"background:{}\"></span>{}</div>\
                 <div class=\"rtag\">{}</div><p>{}{}</p></div>",
                color_for_name(&role.speaker),
                esc(&role.speaker),
                esc(&role.role),
                esc(&role.evidence),
                pv(role.seg, seg_times)
            )
        })
        .collect()
}

fn build_transcript(input: &RenderInput, key_to_index: &HashMap<&str, usize>) -> String {
    let mut out = String::new();
    for (i, text) in input.seg_texts.iter().enumerate() {
        let ts = input.timed.get(i).map(|t| t.start).unwrap_or(0.0);
        let color = input
            .timed
            .get(i)
            .and_then(|t| key_to_index.get(t.speaker_key.as_str()))
            .map(|idx| palette_color(*idx))
            .unwrap_or("var(--muted)");
        let label = input
            .seg_labels
            .get(i)
            .map(String::as_str)
            .unwrap_or("Спикер");
        out.push_str(&format!(
            "<div class=\"turn\" id=\"t{}\"><span class=\"ts\">{}</span>\
             <span class=\"sn\"><span class=\"dot\" style=\"background:{}\"></span>{}</span>\
             <p>{}</p></div>",
            i,
            fmt_mmss(ts),
            color,
            esc(label),
            esc(text)
        ));
    }
    out
}

fn build_footer(input: &RenderInput) -> String {
    format!(
        "<div class=\"mono\">deep_report_v1 · этапов: {} · собрано за {}</div>\
         <div>LLM: DeepSeek ({}) · аудио и транскрипт хранятся локально</div>\
         <div>Провенанс: утверждения со ссылкой на момент записи — клик по времени открывает транскрипт.</div>",
        input.total_stages,
        fmt_mmss(input.generation_secs),
        esc(input.model)
    )
}

// ============================ JS data builders ============================

/// Encode a serde_json Value as a JS literal safe to embed inside `<script>`:
/// neutralise `<` (so `</script>` can't appear) and the JS line separators.
///
/// This is script-boundary escaping only — `<` decodes back to a literal
/// `<` once the script runs, so the values are NOT html-safe. Every innerHTML /
/// template-literal sink in `template.html` must wrap these strings in `escH()`
/// (textContent sinks need no escaping, which is why we don't escape here).
fn js_data(v: &serde_json::Value) -> String {
    serde_json::to_string(v)
        .unwrap_or_else(|_| "null".to_string())
        .replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Build TURNS, SPK, and TALK JS literals from the timeline + speaker rollup.
fn build_speaker_js(
    input: &RenderInput,
    key_to_index: &HashMap<&str, usize>,
) -> (String, String, String) {
    let turns: Vec<serde_json::Value> = input
        .timed
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let idx = key_to_index
                .get(t.speaker_key.as_str())
                .copied()
                .unwrap_or(0) as i64;
            let txt = input
                .seg_texts
                .get(i)
                .map(|s| preview(s, 90))
                .unwrap_or_default();
            json!([
                (t.start * 10.0).round() / 10.0,
                (t.end * 10.0).round() / 10.0,
                idx,
                txt
            ])
        })
        .collect();

    let spk: Vec<serde_json::Value> = input
        .dynamics
        .speakers
        .iter()
        .map(|s| {
            json!({
                "name": s.label,
                "side": "",
                "c": palette_color(s.palette_index),
            })
        })
        .collect();

    let talk: Vec<serde_json::Value> = input
        .dynamics
        .speakers
        .iter()
        .map(|s| {
            json!([
                s.palette_index as i64,
                (s.talk_secs * 10.0).round() / 10.0,
                (s.talk_share * 100.0 * 10.0).round() / 10.0
            ])
        })
        .collect();

    (
        js_data(&serde_json::Value::Array(turns)),
        js_data(&serde_json::Value::Array(spk)),
        js_data(&serde_json::Value::Array(talk)),
    )
}

fn build_topics_js(topics: Option<&Topics>, duration: f64) -> String {
    let arr: Vec<serde_json::Value> = topics
        .map(|t| {
            t.topics
                .iter()
                .map(|tp| {
                    let a = (tp.start_s as f64).clamp(0.0, duration.max(0.0));
                    let b = (tp.end_s as f64).clamp(a, duration.max(a));
                    json!([a, b, tp.name])
                })
                .collect()
        })
        .unwrap_or_default();
    js_data(&serde_json::Value::Array(arr))
}

fn build_markers_js(input: &RenderInput, seg_times: &[f64]) -> String {
    let mut markers: Vec<serde_json::Value> = Vec::new();
    let time_of = |seg: i64| -> Option<f64> {
        if seg >= 0 && (seg as usize) < seg_times.len() {
            Some(seg_times[seg as usize])
        } else {
            None
        }
    };

    if let Some(d) = input.decisions {
        for dec in &d.decisions {
            if let Some(t) = time_of(dec.seg) {
                markers.push(json!([
                    t,
                    "d",
                    format!("Решение: {}", preview(&dec.statement, 60))
                ]));
            }
        }
    }
    if let Some(dc) = input.disagreements_concepts {
        for dis in &dc.disagreements {
            if let Some(seg) = dis.positions.first().map(|p| p.seg) {
                if let Some(t) = time_of(seg) {
                    markers.push(json!([
                        t,
                        "x",
                        format!("Разногласие: {}", preview(&dis.topic, 60))
                    ]));
                }
            }
        }
    }
    if let Some(c) = input.commitments {
        for cm in &c.commitments {
            if let Some(t) = time_of(cm.seg) {
                markers.push(json!([
                    t,
                    "c",
                    format!(
                        "Обязательство: {} — {}",
                        preview(&cm.who, 24),
                        preview(&cm.what, 40)
                    )
                ]));
            }
        }
    }
    js_data(&serde_json::Value::Array(markers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::prompts::{AgendaItem, Commitment};

    fn agenda(status: &str) -> AgendaItem {
        AgendaItem {
            item: "пункт".into(),
            status: status.into(),
            seg: 0,
        }
    }

    fn commitment(has_owner: bool, due: Option<&str>, has_dod: bool) -> Commitment {
        Commitment {
            who: "Кто".into(),
            what: "Что".into(),
            due: due.map(|s| s.to_string()),
            quote: "цитата".into(),
            seg: 0,
            hedge: "firm".into(),
            has_owner,
            has_dod,
            dod_note: None,
        }
    }

    #[test]
    fn score_matches_manual_weighting() {
        // coverage: covered + partial -> (1 + 0.5)/2 * 100 = 75
        // owners: 1 of 2 -> 50; deadline: 1 of 2 -> 50; dod: 2 of 2 -> 100
        // qa: 100 - 12*2 = 76
        // total = 0.25*75 + 0.2*50 + 0.15*50 + 0.15*100 + 0.25*76 = 70.25 -> 70
        let ag = vec![agenda("covered"), agenda("partial")];
        let cm = vec![
            commitment(true, Some("завтра"), true),
            commitment(false, None, true),
        ];
        let s = compute_score(&ag, &cm, 2);
        assert_eq!(s.coverage_pct.round() as i64, 75);
        assert_eq!(s.owners_pct.round() as i64, 50);
        assert_eq!(s.deadline_pct.round() as i64, 50);
        assert_eq!(s.dod_pct.round() as i64, 100);
        assert_eq!(s.qa_pct.round() as i64, 76);
        assert_eq!(s.total, 70);
    }

    #[test]
    fn score_qa_clamped_at_zero_for_many_open_threads() {
        let s = compute_score(&[], &[], 20);
        assert_eq!(s.qa_pct, 0.0);
        assert_eq!(s.total, 0);
    }

    #[test]
    fn empty_inputs_do_not_panic_and_score_zero() {
        let s = compute_score(&[], &[], 0);
        assert_eq!(s.coverage_pct, 0.0);
        assert_eq!(s.owners_pct, 0.0);
        // qa = 100 -> total = 25
        assert_eq!(s.total, 25);
    }

    #[test]
    fn esc_neutralises_html() {
        assert_eq!(esc("<b>&\"'"), "&lt;b&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn js_data_neutralises_script_break() {
        let v = json!(["</script>"]);
        let out = js_data(&v);
        assert!(!out.contains("</script>"));
        assert!(out.contains("\\u003c/script>"));
    }

    #[test]
    fn apply_template_does_not_rescan_inserted_values() {
        let replacements = [
            (
                "<!--SECTION:A-->",
                "transcript quoting /*DATA:B*/ literally".to_string(),
            ),
            ("/*DATA:B*/", "[1,2]".to_string()),
        ];
        let out = apply_template("x <!--SECTION:A--> y /*DATA:B*/ z", &replacements);
        assert_eq!(out, "x transcript quoting /*DATA:B*/ literally y [1,2] z");
    }

    /// js_data values are not html-safe (see its doc), so every string the
    /// template's script interpolates into innerHTML / tooltip HTML must go
    /// through escH. Guards against reintroducing the raw interpolations.
    #[test]
    fn template_escapes_data_strings_at_html_sinks() {
        assert!(TEMPLATE.contains("const escH="));
        for raw in [
            "${p.name}",
            "${p.side||\"\"}",
            "${name}",
            "${label}",
            "${txt}",
            "${SPK[s].name}",
        ] {
            assert!(
                !TEMPLATE.contains(raw),
                "unescaped data interpolation `{raw}` in template.html — wrap it in escH()"
            );
        }
    }

    #[test]
    fn clarify_section_renders_answers_and_hides_when_empty() {
        use crate::report::prompts::ClarifyQuestion;
        assert_eq!(build_clarify(&[], &[]), "");

        let questions = vec![
            ClarifyQuestion {
                id: "q1".into(),
                text: "Кто такой Андрей?".into(),
                quote: Some("...Андрей загрузит...".into()),
                ..Default::default()
            },
            ClarifyQuestion {
                id: "q2".into(),
                text: "Второй вопрос".into(),
                ..Default::default()
            },
        ];
        let answers = vec![ClarifyAnswer {
            question_id: "q1".into(),
            answer: Some("Ведущий".into()),
        }];
        let html = build_clarify(&questions, &answers);
        assert!(html.contains("id=\"clarify\""));
        assert!(html.contains("Кто такой Андрей?"));
        assert!(html.contains("Ведущий"));
        // Unanswered question falls back to "пропущено".
        assert!(html.contains("пропущено"));
    }

    #[test]
    fn render_replaces_every_template_token_even_with_all_stages_failed() {
        use crate::report::dynamics::{Dynamics, SpeakerDyn, TimedSegment};

        let dyn_metrics = Dynamics {
            duration_secs: 65.0,
            speech_density: 0.5,
            turn_count: 2,
            total_questions: 1,
            pauses_over_3s: 1,
            pauses_over_10s: 0,
            speakers: vec![SpeakerDyn {
                key: "ch:mic".into(),
                label: "Аня".into(),
                talk_secs: 40.0,
                talk_share: 1.0,
                questions: 1,
                turns: 2,
                palette_index: 0,
            }],
        };
        let timed = vec![
            TimedSegment {
                start: 0.0,
                end: 3.0,
                speaker_key: "ch:mic".into(),
            },
            TimedSegment {
                start: 5.0,
                end: 7.0,
                speaker_key: "ch:mic".into(),
            },
        ];
        let seg_labels = vec!["Аня".to_string(), "Аня".to_string()];
        // Include a hostile string to prove escaping in transcript output.
        let seg_texts = vec!["привет <script>".to_string(), "как дела?".to_string()];
        let score = compute_score(&[], &[], 0);

        let input = RenderInput {
            meeting_title: "Планёрка",
            date_str: "23 июля 2026",
            model: "deepseek-v4-pro",
            generation_secs: 12.0,
            total_stages: 11,
            dynamics: &dyn_metrics,
            timed: &timed,
            seg_labels: &seg_labels,
            seg_texts: &seg_texts,
            classification: None,
            topics: None,
            decisions: None,
            commitments: None,
            threads_risks: None,
            disagreements_concepts: None,
            numbers: None,
            roles: None,
            insights: None,
            score: &score,
            clarify_questions: &[],
            clarify_answers: &[],
        };
        let html = render_report(&input);

        assert!(
            !html.contains("<!--SECTION:"),
            "an HTML section token was left unreplaced"
        );
        assert!(
            !html.contains("/*DATA:"),
            "a JS data token was left unreplaced"
        );
        // Transcript text must be escaped, never a live tag.
        assert!(html.contains("привет &lt;script&gt;"));
        assert!(html.contains("id=\"t0\""));
        // Failed stages fall back to the ghost placeholder, not a crash.
        assert!(html.contains("Не удалось построить"));
    }
}
