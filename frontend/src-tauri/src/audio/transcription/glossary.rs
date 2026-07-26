// audio/transcription/glossary.rs
//
// Puts names back that the model could not have known.
//
// A speech model only writes words it was trained on. Hand it a surname it has never
// seen and it does not fail - it picks the nearest familiar sounds, and one word comes
// back as two or three: "Baldassarre" as "Baldus are", "Callahan" as "Call and Han".
// That is why these errors cost more than they look: a single unknown name accounts for
// several word errors at once.
//
// The fix is not a better model, it is telling it which names to expect. Nemotron has no
// input for that - unlike Whisper's initial prompt - so the correction happens on the
// text afterwards, matching on how a term is spelled once spaces stop mattering.

/// Terms the transcript should spell the way the user does.
pub struct Glossary {
    terms: Vec<Term>,
}

struct Term {
    /// As the user wrote it. This is what ends up in the transcript.
    display: String,
    /// Lowercased with everything but letters and digits removed, so "Call and Han" and
    /// "Callahan" reduce to strings that differ by two characters rather than by three
    /// words.
    key: String,
    words: usize,
}

/// How much of a term may be wrong and still count as that term.
///
/// A third is wide enough for the run-together mishearings actually observed
/// ("callandhan" is 0.25 away from "callahan", "baldusare" 0.27 from "baldassarre") and
/// narrow enough that ordinary words do not collide.
const MAX_ERROR_RATIO: f64 = 0.34;

/// Below this length a term must match exactly.
///
/// A third of a four-letter term is one character, and one character is the distance
/// between a great many real words. Short terms are also the ones most likely to be
/// acronyms, where a near miss is a different thing entirely, not a misspelling.
const FUZZY_MIN_LEN: usize = 5;

/// The most transcript words any term may be matched against.
///
/// A ceiling on the work done per position, and a hard limit on how much text a single
/// false match could destroy.
const MAX_SPLIT: usize = 5;

/// How many extra words a term may have been broken into beyond its own word count.
///
/// "Callahan" - one word - came back as "Call and Han". Two extra covers the splits
/// actually observed; beyond that a match has stopped being a mishearing and started
/// being a coincidence.
const EXTRA_SPLIT: usize = 2;

impl Glossary {
    /// Build from the user's vocabulary text: terms separated by commas or newlines.
    pub fn parse(text: &str) -> Self {
        let terms = text
            .split(['\n', ','])
            .map(str::trim)
            .filter(|term| !term.is_empty())
            .map(|term| Term {
                display: term.to_string(),
                key: normalise(term),
                words: term.split_whitespace().count(),
            })
            .filter(|term| !term.key.is_empty())
            .collect();

        Self { terms }
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Rewrite any span of `text` that is a mishearing of a known term.
    ///
    /// Scans left to right and never reconsiders a span it has already replaced, so one
    /// term cannot be rewritten into another. Spans are tried longest-first, because
    /// "Call and Han" must win over the single word "Call".
    pub fn correct(&self, text: &str) -> String {
        if self.terms.is_empty() {
            return text.to_string();
        }

        let words: Vec<&str> = text.split_whitespace().collect();
        let mut out: Vec<String> = Vec::with_capacity(words.len());
        let mut at = 0;

        while at < words.len() {
            match self.best_match(&words[at..]) {
                Some((term, span)) => {
                    out.push(carry_punctuation(&words[at..at + span], term));
                    at += span;
                }
                None => {
                    out.push(words[at].to_string());
                    at += 1;
                }
            }
        }

        out.join(" ")
    }

    /// The term this run of words is most likely to be, and how many words it covers.
    fn best_match(&self, words: &[&str]) -> Option<(&str, usize)> {
        let mut best: Option<(&str, usize, f64)> = None;

        for span in 1..=MAX_SPLIT.min(words.len()) {
            let candidate = normalise(&words[..span].join(""));
            if candidate.is_empty() {
                continue;
            }

            for term in &self.terms {
                // A term written as several words cannot have been heard as fewer, and a
                // term of W words split into more than W + 2 has stopped being a
                // mishearing and started being a coincidence.
                if span < term.words || span > term.words + EXTRA_SPLIT {
                    continue;
                }

                let allowed = if term.key.len() < FUZZY_MIN_LEN {
                    0
                } else {
                    (term.key.len() as f64 * MAX_ERROR_RATIO) as usize
                };

                let distance = edit_distance(&candidate, &term.key);
                if distance > allowed {
                    continue;
                }

                // Every span is scored and the closest wins, rather than stopping at the
                // longest that merely fits. Stopping early gets "John Call and" matched
                // as a name and leaves "Han" stranded, when "John Call and Han" is the
                // better reading; scoring everything picks the reading that fits best.
                // Ties go to the longer span, which absorbs the debris.
                let ratio = distance as f64 / term.key.len() as f64;
                let better = match best {
                    None => true,
                    Some((_, best_span, best_ratio)) => {
                        ratio < best_ratio || (ratio == best_ratio && span > best_span)
                    }
                };
                if better {
                    best = Some((term.display.as_str(), span, ratio));
                }
            }
        }

        best.map(|(display, span, _)| (display, span))
    }
}

/// Lowercase, letters and digits only. Spaces go too, which is the whole point: the
/// mishearings being repaired are ones where the model put word boundaries in.
fn normalise(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Keep whatever punctuation trailed the span, so replacing a name does not swallow the
/// full stop that ended the sentence.
fn carry_punctuation(span: &[&str], replacement: &str) -> String {
    let last = span.last().copied().unwrap_or("");
    let trailing: String = last
        .chars()
        .rev()
        .take_while(|c| !c.is_alphanumeric())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    format!("{replacement}{trailing}")
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut next = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            next.push((row[j] + cost).min(row[j + 1] + 1).min(next[j] + 1));
        }
        row = next;
    }

    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two errors measured on the reference recording, both of them a name the model
    /// had never seen being split into words it had.
    #[test]
    fn it_puts_back_names_the_model_split_apart() {
        let glossary = Glossary::parse("Teddy Baldassarre, John Callahan");

        assert_eq!(
            glossary.correct("hi everyone Teddy Baldus are here"),
            "hi everyone Teddy Baldassarre here"
        );
        assert_eq!(
            glossary.correct("We have John Call and Han, our boutique director"),
            "We have John Callahan, our boutique director"
        );
    }

    /// The risk that matters. A glossary is loaded for a whole meeting, so it sees far
    /// more text that has nothing to do with it than text that does, and quietly
    /// rewriting correct words would be worse than the errors it repairs.
    #[test]
    fn it_leaves_text_that_has_nothing_to_do_with_it_alone() {
        let glossary = Glossary::parse("Baldassarre, Callahan, Speedmaster");
        let untouched = "I just took out a link in fifteen seconds and it is incredible \
                         so we have a couple different kind of looks today";

        assert_eq!(glossary.correct(untouched), untouched);
    }

    /// Short terms are usually acronyms, where a near miss is a different thing rather
    /// than a misspelling. A third of three letters is one, and one letter separates a
    /// great many real words.
    #[test]
    fn short_terms_must_match_exactly() {
        let glossary = Glossary::parse("DVA");

        assert_eq!(glossary.correct("the dva report"), "the DVA report");
        assert_eq!(glossary.correct("the diva sang"), "the diva sang");
        assert_eq!(glossary.correct("we saw her"), "we saw her");
    }

    /// Replacing a name must not eat the sentence that ended with it.
    #[test]
    fn punctuation_after_a_name_survives() {
        let glossary = Glossary::parse("Callahan");

        assert_eq!(glossary.correct("thanks, Call and Han."), "thanks, Callahan.");
    }

    /// A word that is already exactly a glossary term is left where it is, rather than
    /// being absorbed into a longer, fuzzier match of a different term.
    ///
    /// This is the tie-break that matters, and it falls out of scoring by how wrong a
    /// match is: an exact match is zero wrong, and nothing beats it. The alternative -
    /// always preferring the longer span - would let "Callahan" swallow a correctly
    /// transcribed "Call" standing next to it.
    #[test]
    fn a_word_that_is_already_right_is_not_absorbed() {
        let glossary = Glossary::parse("Callahan, Call");

        assert_eq!(
            glossary.correct("John Call and Han spoke"),
            "John Call and Han spoke"
        );
    }

    /// With no competing exact match, a name broken into several words is put back whole
    /// rather than half-repaired with the remainder left as debris.
    #[test]
    fn a_split_name_is_rejoined_completely() {
        let glossary = Glossary::parse("John Callahan");

        assert_eq!(
            glossary.correct("we have John Call and Han here"),
            "we have John Callahan here"
        );
    }

    /// An empty glossary is the normal case for most users and must cost nothing.
    #[test]
    fn an_empty_glossary_changes_nothing() {
        let glossary = Glossary::parse("  \n , ");

        assert!(glossary.is_empty());
        assert_eq!(glossary.correct("anything at all"), "anything at all");
    }
}
