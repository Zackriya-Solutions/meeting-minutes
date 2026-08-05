//! Deterministic transcript text normalization.
//!
//! Ported from GigaType's `src/utils/transcriptionFormatting.js`, which cleans dictated
//! speech before it is typed into the focused field. Russian ASR transcribes hesitation
//! sounds literally — «Э-э-э. Продолжаем.», «Я, э-э-э, думаю.», «А-а, продолжаем.» — and
//! the resulting rows read badly in a meeting transcript. This pass removes them, then
//! repairs the punctuation the removal left dangling (a lone «, » before a period, doubled
//! commas, a row that now starts with the punctuation of a deleted filler).
//!
//! No model, no network: the whole pass is a scan plus a handful of rewrite rules, so it
//! runs on every refined row for free.
//!
//! **Not ported:** GigaType's `stripSingleTerminalPeriod`, which drops a row's final
//! period. That is a dictation-insertion nicety (typed text should not end in a period the
//! user did not ask for). Applied to a meeting transcript it would strip sentence-final
//! punctuation from every row, and [`super::refinement::rejoin_sentence_fragments`] reads
//! exactly that punctuation to decide whether two rows are one split sentence — every row
//! would become a merge candidate.
//!
//! **Added:** re-capitalizing a row whose opening filler was removed (see
//! [`capitalize_first`]) — GigaType has no need for it, a transcript row does.
//!
//! The hesitation patterns are Cyrillic-only, so English (Whisper/Parakeet) transcripts
//! pass through untouched.

use once_cell::sync::Lazy;
use regex::Regex;

/// Upper bound on the match candidates explored at one starting position. A hesitation
/// chain is a handful of syllables in practice; the cap keeps a pathological run of
/// «э... э... э...» from making the backtracking search exponential.
const MAX_MATCH_CANDIDATES: usize = 64;

/// Clean one transcript row. Returns the trimmed text, empty when the row was nothing but
/// hesitation («Э-э-э.» → ``) — callers should drop such rows.
pub(crate) fn normalize_transcript_text(text: &str) -> String {
    strip_hesitations(text)
}

/// Remove standalone hesitation sounds and repair the punctuation left behind.
///
/// Mirrors `stripHesitationEs`: the punctuation rules only run when something was actually
/// removed, so untouched text is never rewritten.
fn strip_hesitations(text: &str) -> String {
    let trimmed = text.trim();
    let mut chars: Vec<char> = trimmed.chars().collect();
    let mut changed = false;
    let mut opened_the_row = false;

    if let Some((stripped, at_start)) = remove_matches(&chars, match_e_at) {
        chars = stripped;
        changed = true;
        opened_the_row |= at_start;
    }
    if let Some((stripped, at_start)) = remove_matches(&chars, match_a_at) {
        chars = stripped;
        changed = true;
        opened_the_row |= at_start;
    }
    if !changed {
        return trimmed.to_string();
    }

    let cleaned = tidy_punctuation(&chars.into_iter().collect::<String>());
    if opened_the_row {
        capitalize_first(&cleaned)
    } else {
        cleaned
    }
}

/// Upper-case the first character of a row that lost its opening filler.
///
/// A deviation from GigaType, and a necessary one: dictated text is inserted mid-context
/// where case does not matter, but a transcript row reads as a sentence. «Э-э, ну запиши»
/// would otherwise become «ну запиши» — and a lowercase opener is exactly the signal
/// [`super::refinement::rejoin_sentence_fragments`] treats as "this row continues the
/// previous one", so leaving it would fabricate merges across speakers.
fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_lowercase() => {
            first.to_uppercase().collect::<String>() + chars.as_str()
        }
        _ => text.to_string(),
    }
}

/// Punctuation repair applied after a removal, in GigaType's order: collapse the comma or
/// colon a deleted filler left in front of a sentence end, de-duplicate the resulting
/// runs, drop punctuation that now leads or trails a line, and squeeze whitespace.
fn tidy_punctuation(text: &str) -> String {
    static RULES: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
        [
            (r"[,;:][ \t]*([.!?])", "${1}"),
            (r"([.!?])[ \t]*[,;:]", "${1}"),
            (r"([.!?])(?:[ \t]+[.!?])+", "${1}"),
            (r"[ \t]+([,.;:!?…])", "${1}"),
            (r"([,;:])(?:[ \t]*[,;:])+", "${1}"),
            (r"(?m)^[ \t]*[,.;:!?…]+[ \t]*", ""),
            (r"(?m)[ \t]*[,;:]+[ \t]*$", ""),
            (r"[ \t]{2,}", " "),
            (r"[ \t]+\n", "\n"),
            (r"\n[ \t]+", "\n"),
        ]
        .into_iter()
        .map(|(pattern, replacement)| {
            (
                Regex::new(pattern).expect("valid punctuation cleanup regex"),
                replacement,
            )
        })
        .collect()
    });

    let mut out = text.to_string();
    for (pattern, replacement) in RULES.iter() {
        out = pattern.replace_all(&out, *replacement).into_owned();
    }
    out.trim().to_string()
}

/// Scan `chars` left to right, dropping every match `matcher` reports. Returns `None` when
/// nothing matched, so callers can tell "unchanged" from "everything was filler"; the flag
/// says whether a match sat at the very beginning of the row.
///
/// A match may only start where the preceding character is not a letter or digit — the
/// check reads the original text, so «поэзию» is never mistaken for a hesitation.
fn remove_matches(
    chars: &[char],
    matcher: fn(&[char], usize) -> Option<usize>,
) -> Option<(Vec<char>, bool)> {
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    let mut changed = false;
    let mut at_start = false;
    while i < chars.len() {
        if i == 0 || !is_word(chars[i - 1]) {
            if let Some(end) = matcher(chars, i) {
                if end > i {
                    changed = true;
                    at_start |= i == 0;
                    i = end;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    changed.then_some((out, at_start))
}

/// `э+ (?: [ \t]*[-–—][ \t]*э+ | [ \t]*(?:\.[ \t]*\.[ \t]*\.|…)[ \t]*э* )*`
///
/// A single «э» counts — unlike «а» it is never a word on its own.
fn match_e_at(chars: &[char], start: usize) -> Option<usize> {
    let after_run = skip_run(chars, start, E);
    if after_run == start {
        return None;
    }
    let mut candidates = Vec::new();
    collect_e_star_ends(chars, after_run, &mut candidates);
    candidates.into_iter().find(|&end| boundary_ok(chars, end))
}

/// `а+ (?:[ \t]*[-–—][ \t]*а+)+ (?:[ \t]*(?:\.[ \t]*\.[ \t]*\.|…))?`
///
/// At least one repetition is required: a bare «а» is a conjunction («А потом продолжим»),
/// only the stutter «а-а-а» is filler.
fn match_a_at(chars: &[char], start: usize) -> Option<usize> {
    let after_run = skip_run(chars, start, A);
    if after_run == start {
        return None;
    }

    let mut repetition_ends = Vec::new();
    let mut pos = after_run;
    while let Some(end) = match_dash_repetition(chars, pos, A) {
        repetition_ends.push(end);
        pos = end;
    }

    // Greedy: prefer the longest chain, and the trailing ellipsis over omitting it.
    for &end in repetition_ends.iter().rev() {
        if let Some(after_dots) = match_ellipsis(chars, skip_hspace(chars, end)) {
            if boundary_ok(chars, after_dots) {
                return Some(after_dots);
            }
        }
        if boundary_ok(chars, end) {
            return Some(end);
        }
    }
    None
}

/// Enumerate the end positions the star loop can produce, in the order a backtracking
/// engine would try them (most iterations first, then progressively fewer).
fn collect_e_star_ends(chars: &[char], pos: usize, out: &mut Vec<usize>) {
    if out.len() >= MAX_MATCH_CANDIDATES {
        return;
    }
    for end in e_iteration_ends(chars, pos) {
        if end > pos {
            collect_e_star_ends(chars, end, out);
        }
    }
    out.push(pos);
}

/// End positions of one iteration of the «э» star loop, greediest first.
fn e_iteration_ends(chars: &[char], pos: usize) -> Vec<usize> {
    if let Some(end) = match_dash_repetition(chars, pos, E) {
        return vec![end];
    }

    let Some(after_dots) = match_ellipsis(chars, skip_hspace(chars, pos)) else {
        return Vec::new();
    };
    // `[ \t]*э*` — greedy, but the engine gives the tail back when the trailing spaces or
    // «э»s would put the match right up against a word: «э... абв» matches only «э...».
    let after_spaces = skip_hspace(chars, after_dots);
    let after_trailing_run = skip_run(chars, after_spaces, E);
    let mut ends = Vec::new();
    if after_trailing_run > after_spaces {
        ends.push(after_trailing_run);
    }
    if after_spaces > after_dots {
        ends.push(after_spaces);
    }
    ends.push(after_dots);
    ends
}

/// `[ \t]*[-–—][ \t]*<letter>+` — one link of a stutter chain.
fn match_dash_repetition(chars: &[char], pos: usize, letter: [char; 2]) -> Option<usize> {
    let dash = skip_hspace(chars, pos);
    if !chars.get(dash).is_some_and(|&c| is_dash(c)) {
        return None;
    }
    let after_dash = skip_hspace(chars, dash + 1);
    let end = skip_run(chars, after_dash, letter);
    (end > after_dash).then_some(end)
}

/// `\.[ \t]*\.[ \t]*\.|…` at `pos`, returning the position after it.
fn match_ellipsis(chars: &[char], pos: usize) -> Option<usize> {
    if chars.get(pos) == Some(&'…') {
        return Some(pos + 1);
    }
    let mut i = pos;
    for dot in 0..3 {
        if dot > 0 {
            i = skip_hspace(chars, i);
        }
        if chars.get(i) != Some(&'.') {
            return None;
        }
        i += 1;
    }
    Some(i)
}

/// The trailing guard `(?![\p{L}\p{N}]|[ \t]*[-–—][ \t]*[\p{L}\p{N}])`.
///
/// Rejecting a dash followed by a word is what keeps «А-а-абсолютно» intact: without it the
/// match would stop after «А-а» and leave a stray «-абсолютно».
fn boundary_ok(chars: &[char], pos: usize) -> bool {
    if chars.get(pos).is_some_and(|&c| is_word(c)) {
        return false;
    }
    let dash = skip_hspace(chars, pos);
    if chars.get(dash).is_some_and(|&c| is_dash(c)) {
        let after_dash = skip_hspace(chars, dash + 1);
        if chars.get(after_dash).is_some_and(|&c| is_word(c)) {
            return false;
        }
    }
    true
}

const E: [char; 2] = ['э', 'Э'];
const A: [char; 2] = ['а', 'А'];

fn skip_run(chars: &[char], mut i: usize, letter: [char; 2]) -> usize {
    while chars
        .get(i)
        .is_some_and(|&c| c == letter[0] || c == letter[1])
    {
        i += 1;
    }
    i
}

fn skip_hspace(chars: &[char], mut i: usize) -> usize {
    while chars.get(i).is_some_and(|&c| c == ' ' || c == '\t') {
        i += 1;
    }
    i
}

fn is_dash(c: char) -> bool {
    matches!(c, '-' | '–' | '—')
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The cases below are the GigaType suite (test/helpers/transcriptText.test.js). The
    // port agrees with the original on all of them except where a row-opening filler was
    // removed, where meetily re-capitalizes (see `capitalize_first`).

    #[test]
    fn removes_standalone_hesitation_variants() {
        assert_eq!(normalize_transcript_text("Я э думаю."), "Я думаю.");
        assert_eq!(normalize_transcript_text("Я, э-э-э, думаю."), "Я, думаю.");
        assert_eq!(
            normalize_transcript_text("Э-э-э. Продолжаем."),
            "Продолжаем."
        );
        assert_eq!(
            normalize_transcript_text("Э-э-э... Продолжаем."),
            "Продолжаем."
        );
        assert_eq!(normalize_transcript_text("э ээ эээ э-э э-э-э э... э…"), "");
        assert_eq!(
            normalize_transcript_text("А-а... Продолжаем."),
            "Продолжаем."
        );
        assert_eq!(
            normalize_transcript_text("А-а-а... Продолжаем."),
            "Продолжаем."
        );
        // GigaType leaves these lowercase; a transcript row is a sentence.
        assert_eq!(
            normalize_transcript_text("Э — Э — Э, продолжаем."),
            "Продолжаем."
        );
        assert_eq!(normalize_transcript_text("А-а, продолжаем."), "Продолжаем.");
        assert_eq!(
            normalize_transcript_text("А-а-а, продолжаем."),
            "Продолжаем."
        );
    }

    #[test]
    fn recapitalizes_only_when_the_row_lost_its_opening_filler() {
        // Real rows from the reference meetings.
        assert_eq!(
            normalize_transcript_text("Э-э, ну запиши тогда что-нибудь. Да."),
            "Ну запиши тогда что-нибудь. Да."
        );
        assert_eq!(
            normalize_transcript_text("А-а, ну можно спуллить вот последнюю версию."),
            "Ну можно спуллить вот последнюю версию."
        );
        // Filler in the middle leaves the row's own opening word alone.
        assert_eq!(
            normalize_transcript_text("потом, э-э-э, посмотрим."),
            "потом, посмотрим."
        );
    }

    #[test]
    fn preserves_words_containing_the_letter_e() {
        assert_eq!(
            normalize_transcript_text("Эмма изучает поэзию и эмпатию."),
            "Эмма изучает поэзию и эмпатию."
        );
    }

    #[test]
    fn preserves_meaningful_words_and_standalone_a() {
        assert_eq!(normalize_transcript_text("А"), "А");
        assert_eq!(normalize_transcript_text("а"), "а");
        assert_eq!(
            normalize_transcript_text("А потом продолжим."),
            "А потом продолжим."
        );
        assert_eq!(
            normalize_transcript_text("Я, а, думаю иначе."),
            "Я, а, думаю иначе."
        );
        assert_eq!(
            normalize_transcript_text("А-аудио уже готово."),
            "А-аудио уже готово."
        );
    }

    #[test]
    fn leaves_hyphenated_words_intact_instead_of_clipping_them() {
        // A partial match here used to leave a stray leading hyphen ("-абсолютно точно").
        assert_eq!(
            normalize_transcript_text("А-а-абсолютно точно"),
            "А-а-абсолютно точно"
        );
        assert_eq!(
            normalize_transcript_text("Э-э-этого мало"),
            "Э-э-этого мало"
        );
    }

    // Meetily-specific expectations.

    #[test]
    fn keeps_sentence_final_punctuation_the_fragment_rejoin_reads() {
        // GigaType also drops the trailing period for dictation; a transcript row must
        // keep it, or every row looks like an unfinished sentence to rejoin.
        assert_eq!(normalize_transcript_text("Готово."), "Готово.");
        assert_eq!(
            normalize_transcript_text("Ну, э-э-э, готово."),
            "Ну, готово."
        );
        assert_eq!(normalize_transcript_text("  Готово.  "), "Готово.");
    }

    #[test]
    fn hesitation_only_rows_collapse_to_empty() {
        assert_eq!(normalize_transcript_text("Э-э-э."), "");
        assert_eq!(normalize_transcript_text("А-а-а..."), "");
        assert_eq!(normalize_transcript_text(""), "");
        assert_eq!(normalize_transcript_text("   "), "");
    }

    #[test]
    fn leaves_latin_transcripts_untouched() {
        assert_eq!(
            normalize_transcript_text("The a-a-agenda, uh, is ready."),
            "The a-a-agenda, uh, is ready."
        );
    }

    #[test]
    fn ellipsis_before_a_word_does_not_swallow_the_word() {
        assert_eq!(normalize_transcript_text("э... абв"), "Абв");
        assert_eq!(
            normalize_transcript_text("Итак, э… дальше."),
            "Итак, дальше."
        );
    }

    #[test]
    fn multiline_rows_keep_their_line_structure() {
        assert_eq!(
            normalize_transcript_text("Первое, э-э-э, готово.\nВторое, э, тоже."),
            "Первое, готово.\nВторое, тоже."
        );
    }
}
