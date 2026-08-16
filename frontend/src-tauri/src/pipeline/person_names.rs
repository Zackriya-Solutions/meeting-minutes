//! One gate that decides whether a string may become a person's name.
//!
//! Every layer that can write into `speakers.display_name` without the user asking —
//! the local regex pass in [`super::speaker_names`] and the LLM pass in
//! [`super::speaker_naming`] — funnels its candidates through here. Before this
//! existed each layer carried its own ad-hoc blocklist, and the transcript-derived
//! layer accepted anything a capitalised word followed by a comma could produce: a
//! Russian meeting reliably turned «Назови, как тебя зовут» into a speaker named
//! *Назови* and «Бля, проблема в том…» into a speaker named *Бля*.
//!
//! A blocklist cannot win that argument — the space of imperatives, interjections and
//! ASR mangling is open-ended. So the gate is positive where it can afford to be:
//!
//!   * [`is_plausible_person_name`] rejects what is structurally not a name (profanity
//!     including truncations, role words, discourse markers, verb morphology). Every
//!     automatic writer must clear it.
//!   * [`is_known_given_name`] additionally *recognises* a name. Weak evidence — being
//!     addressed once across a turn boundary — may only name a speaker when the word is
//!     a name we know. Grammatically explicit evidence («меня зовут …») does not need
//!     it, so rare and foreign names still land through the path that actually says so.
//!
//! A name that fails the second check is not lost: it stays a pending candidate for
//! review, and the context-aware LLM pass still sees the whole conversation.

use std::collections::HashSet;

/// Common Russian given names and the diminutives a meeting actually uses, normalised
/// the way [`normalized`] normalises: lowercase, `ё` folded to `е`.
///
/// Deliberately not exhaustive. Missing a name costs one unattended rename that the
/// user can still make by hand (or that the LLM pass makes for us); admitting a verb
/// costs a wrong name on a real person, which is the failure users reported.
const GIVEN_NAMES: &[&str] = &[
    // Male
    "александр", "саша", "саня", "шура", "алексей", "лёша", "леша", "лёха", "леха",
    "анатолий", "толя", "андрей", "антон", "тоша", "аркадий", "арсений", "сеня",
    "артём", "артем", "тёма", "тема", "артур", "богдан", "борис", "боря", "вадим",
    "валентин", "валера", "валерий", "василий", "вася", "виктор", "витя", "виталий",
    "виталик", "владимир", "вова", "володя", "владислав", "влад", "вячеслав", "слава",
    "геннадий", "гена", "георгий", "гоша", "жора", "герман", "глеб", "григорий",
    "гриша", "давид", "даниил", "данил", "данила", "даня", "денис", "дмитрий", "дима",
    "митя", "евгений", "женя", "егор", "иван", "ваня", "игорь", "илья", "иннокентий",
    "кирилл", "константин", "костя", "лев", "лёва", "лева", "леонид", "лёня", "леня",
    "макар", "максим", "макс", "марк", "матвей", "михаил", "миша", "никита", "николай",
    "коля", "олег", "пётр", "петр", "петя", "павел", "паша", "платон", "роман", "рома",
    "ростислав", "руслан", "рустам", "савелий", "святослав", "семён", "семен", "сёма",
    "сема", "сергей", "серёжа", "сережа", "серж", "станислав", "стас", "степан",
    "стёпа", "степа", "тарас", "тимофей", "тима", "тимур", "фёдор", "федор", "федя",
    "филипп", "филя", "эдуард", "эдик", "юрий", "юра", "ян", "ярослав",
    // Разговорные формы, которые встреча использует чаще паспортных.
    "андрюха", "серега", "димон", "толян", "санек", "саныч", "ромка", "витек", "леха",
    "юрок", "костян", "макс", "мася",
    "азамат", "айрат", "алан", "альберт", "амир", "арам", "армен", "ашот", "баходир",
    "гурген", "давлат", "дамир", "джамал", "зураб", "ильдар", "ильнур", "искандер",
    "карим", "леван", "марат", "мурат", "нариман", "рамиль", "ренат", "рафаэль",
    "рашид", "самир", "тигран", "фарид", "хасан", "шамиль", "эльдар",
    // Female
    "александра", "алла", "алина", "алиса", "анастасия", "настя", "ангелина", "анна",
    "аня", "анюта", "антонина", "валентина", "валерия", "лера", "варвара", "варя",
    "вера", "вероника", "виктория", "вика", "галина", "галя", "дарья", "даша", "диана",
    "дина", "евгения", "екатерина", "катя", "елена", "лена", "алёна", "алена",
    "елизавета", "лиза", "жанна", "зинаида", "зоя", "инна", "ирина", "ира", "карина",
    "кира", "клавдия", "ксения", "ксюша", "лариса", "лидия", "лида", "любовь", "люба",
    "людмила", "люда", "мила", "маргарита", "рита", "марина", "мария", "маша", "надежда",
    "надя", "наталья", "наталия", "наташа", "нина", "оксана", "олеся", "ольга", "оля",
    "полина", "поля", "раиса", "регина", "римма", "светлана", "света", "снежана",
    "софия", "софья", "соня", "тамара", "татьяна", "таня", "ульяна", "юлия", "юля",
    "яна", "ярослава", "аида", "альбина", "амина", "гузель", "динара", "зарина",
    "лейла", "мадина", "милана", "нелли", "нурия", "тамила", "эльвира",
    // Latin spellings that show up in mixed-language meetings
    "alex", "alexander", "andrew", "andrey", "anna", "anton", "boris", "chris", "daniel",
    "david", "dmitry", "elena", "emma", "eugene", "ivan", "james", "john", "julia",
    "kate", "kirill", "maria", "mark", "martin", "max", "michael", "mike", "nick",
    "nikita", "olga", "oliver", "paul", "peter", "robert", "sergey", "sophie", "thomas",
    "victor", "vladimir",
];

/// Words that are never a person even though the surrounding grammar looks like address.
/// Roles, greetings, discourse markers — the vocabulary that fills the same slot a name
/// fills in «Коллеги, давайте начнём».
const BLOCKED_EXACT: &[&str] = &[
    "коллега", "коллеги", "друзья", "ребята", "команда", "разработчик", "аналитик",
    "менеджер", "руководитель", "директор", "заказчик", "клиент", "спикер", "участник",
    "доктор", "девушка", "мужчина", "женщина", "человек", "начальник", "босс", "автор",
    "народ", "мужики", "девчонки", "парни", "господа", "чувак", "чуваки",
    "всем", "кто", "что", "привет", "спасибо", "пожалуйста", "пока", "алло", "ага",
    "угу", "слушай", "слушайте", "смотри", "смотрите", "подожди", "подождите",
    "погоди", "погодите", "давай", "давайте", "поехали", "начнём", "начнем",
    "расскажи", "расскажите", "скажи", "скажите", "подскажи", "подскажите",
    "назови", "назовите", "представь", "представьте", "напомни", "напомните",
    "извини", "извините", "прости", "простите", "знаешь", "знаете", "помнишь",
    "помните", "слышь", "слышишь", "прикинь", "смотрю", "стоп",
    "будет", "может", "просто", "типа", "кстати", "короче", "например", "итак",
    "конечно", "возможно", "видимо", "наверное", "значит", "вообще", "впрочем",
    "однако", "правда", "ладно", "окей", "хорошо", "понятно", "сегодня", "завтра",
    "вчера", "сейчас", "потом", "все", "всё", "результат", "результаты", "новость",
    "ну", "вот", "сказали", "блин", "елки",
    // Короткие служебные слова и междометия, которые ASR ставит ровно в позицию имени.
    "да", "нет", "не", "ни", "так", "то", "тот", "эт", "это", "этот", "эта", "там", "тут",
    "вон", "ой", "эй", "ах", "ох", "уф", "щас", "ща", "прям", "ладно", "точно", "ясно",
    "кажется", "казалось", "можно", "нужно", "надо", "есть", "было", "будем", "главное",
    "вопрос", "идея", "дальше", "интересно", "единственное", "получается", "по-моему",
    "смотрю", "думаю", "вижу", "слышу", "помню", "говорят", "сказал", "сказала",
    "hello", "team", "guys", "manager", "developer", "speaker", "doctor", "client",
    "boss", "please", "thanks", "sorry", "okay", "well", "look", "listen", "wait",
];

/// Profanity, checked as prefixes so ASR truncations («бля» for «блядь») are caught too.
/// A rejected candidate is never stored, so these strings only ever gate, never persist.
const BLOCKED_STEMS: &[&str] = &[
    "бля", "сука", "суч", "пизд", "хуй", "хуя", "хую", "хуе", "хуё", "хуем", "нахер",
    "нахр", "хрен", "говн", "дерьм", "сран", "ебан", "ёбан", "ебат", "ебал", "долбо",
    "ублюд", "мудак", "мудил", "мраз", "гандон", "пидор", "пидар", "шлюх", "урод",
    "дебил", "идиот", "тупиц", "тупор", "козел", "козёл", "сволоч", "чмо", "лох",
    "падла", "гнида", "хер", "fuck", "shit", "bitch", "asshole", "dick", "cunt",
];

/// Endings that belong to a verb, not to a name. Russian address slots are filled by
/// imperatives far more often than by anything else a regex can confuse with a name
/// («Назови, …», «Смотрите, …», «Подождите, …»), and imperative morphology is regular
/// enough to reject wholesale. Kept narrow on purpose: `-ай`/`-ой` are excluded because
/// Николай and Аркадий live there.
const VERB_ENDINGS: &[&str] = &[
    "йте", "ите", "ешь", "ишь", "ться", "тесь", "аем", "аете", "ают", "ует", "уют",
    "ови", "ажи", "яжи", "оди", "иди", "неси", "беги", "ыть", "ать", "ять", "еть",
    "ается", "аются", "уется", "уются", "ешься", "ишься", "аешь", "аете", "илось", "илась",
];

/// Normalise for comparison: lowercase, `ё` folded to `е`, letters only.
pub fn normalized(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .replace('ё', "е")
        .chars()
        .filter(|character| character.is_alphabetic() || matches!(character, '-' | '\'' | '’'))
        .collect()
}

/// A name we recognise. Hyphenated names count when both halves are known
/// («Анна-Мария»); a single known half is enough for the common «Мария-Анна» shape.
pub fn is_known_given_name(value: &str) -> bool {
    let normalized = normalized(value);
    if normalized.is_empty() {
        return false;
    }
    if GIVEN_NAMES.contains(&normalized.as_str()) {
        return true;
    }
    if normalized
        .split('-')
        .any(|part| GIVEN_NAMES.contains(&part))
    {
        return true;
    }
    // Russian addresses people by a truncated vocative — «Миш!», «Серёж!», «Саш!» — which is
    // both the form a transcript actually contains and a form no common word takes. Fold it
    // back to the name it truncates. Three letters minimum: «Ан» would otherwise reach «Аня»
    // from any stray particle.
    let letters = letter_count(&normalized);
    letters >= 3
        && ["а", "я"]
            .iter()
            .any(|suffix| GIVEN_NAMES.contains(&format!("{normalized}{suffix}").as_str()))
}

fn letter_count(value: &str) -> usize {
    value.chars().filter(|character| character.is_alphabetic()).count()
}

/// Could this string be somebody's name at all?
///
/// Rejects profanity (including truncations), role and discourse words, and verb
/// morphology. This is the floor every automatic writer must clear — including the LLM,
/// which is perfectly capable of echoing back the loudest word in a transcript.
pub fn is_plausible_person_name(value: &str) -> bool {
    let normalized = normalized(value);
    // A known name outranks every heuristic below: «Ян» must not lose to a length floor and
    // «Никита» must not lose to a verb ending.
    if is_known_given_name(&normalized) {
        return true;
    }
    let letters = normalized
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect::<String>();
    // Two letters is «да», «не», «то», «ой» — the words a transcript puts in the address
    // slot all day. A two-letter name has to be one we recognise, handled just above.
    if !(3..=32).contains(&letters.chars().count()) {
        return false;
    }
    // «э-э», «а-а-а», «ммм»: hesitation, not a person.
    if letters
        .chars()
        .collect::<HashSet<char>>()
        .len()
        <= 2
    {
        return false;
    }
    if BLOCKED_STEMS
        .iter()
        .any(|stem| letters.starts_with(stem))
    {
        return false;
    }
    if BLOCKED_EXACT.contains(&normalized.as_str()) || BLOCKED_EXACT.contains(&letters.as_str())
    {
        return false;
    }
    if VERB_ENDINGS
        .iter()
        .any(|ending| letters.len() > ending.len() && letters.ends_with(ending))
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_that_became_speakers_are_rejected() {
        // Both come from a real meeting: «Назови, как тебя зовут» and «Бля, проблема…».
        assert!(!is_plausible_person_name("Назови"));
        assert!(!is_plausible_person_name("Бля"));
        assert!(!is_plausible_person_name("Блядь"));
        assert!(!is_plausible_person_name("Блин"));
        assert!(!is_plausible_person_name("Слышь"));
        assert!(!is_plausible_person_name("Подождите"));
        assert!(!is_plausible_person_name("Спикер"));
    }

    #[test]
    fn real_names_survive_the_gate() {
        for name in ["Миша", "Андрей", "Анна", "Мария-Анна", "Гурген", "O'Neill", "Ким"] {
            assert!(is_plausible_person_name(name), "rejected {name}");
        }
    }

    #[test]
    fn recognition_is_stricter_than_plausibility() {
        assert!(is_known_given_name("Миша"));
        assert!(is_known_given_name("МИША"));
        assert!(is_known_given_name("Мария-Анна"));
        assert!(!is_known_given_name("Назови"));
        // Plausible but unknown: allowed to be reviewed, not to be applied on weak evidence.
        assert!(is_plausible_person_name("Аюна"));
        assert!(!is_known_given_name("Аюна"));
    }

    /// Measured against this archive's 802 collected candidates: the address slot fills up
    /// with two-letter particles, hesitation noise and ordinary verbs far more often than
    /// with names, and no blocklist finishes that job. Structure does.
    #[test]
    fn particles_hesitation_and_verbs_are_not_names() {
        for word in [
            "да", "нет", "не", "то", "ой", "так",
            "э-э", "а-а-а", "ммм",
            "о'кей", "окей",
            "кажется", "получается", "выяснилось", "говорят", "главное", "интересно",
        ] {
            assert!(!is_plausible_person_name(word), "accepted {word}");
        }
        // Two letters are fine when we recognise the name.
        assert!(is_plausible_person_name("Ян"));
    }

    /// «Миш!», «Серёж!», «Саш!» — the truncated vocative is how a transcript actually
    /// contains a name, and no common word takes that form.
    #[test]
    fn the_truncated_vocative_is_recognised_as_its_name() {
        for word in ["Миш", "Серёж", "Саш", "Ром", "Андрюх"] {
            assert!(is_known_given_name(word), "missed {word}");
        }
        // Two letters would reach a name from any stray particle: «Ан» → «Аня».
        assert!(!is_known_given_name("Ан"));
    }

    #[test]
    fn yo_folding_makes_both_spellings_the_same_name() {
        assert!(is_known_given_name("Алёна"));
        assert!(is_known_given_name("Алена"));
        assert!(is_known_given_name("Пётр"));
    }
}
