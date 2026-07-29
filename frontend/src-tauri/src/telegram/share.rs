//! Building the Telegram share URL.
//!
//! # What the share action actually accepts
//!
//! Verified against Telegram Desktop 7.0.5 (`com.tdesktop.Telegram`) on macOS:
//!
//! * The handler table has `^msg_url/?\?(.+)(#|$)` and `^share/url/?\?(.+)$`. There is
//!   **no** bare `msg` handler, so `tg://msg?text=…` is silently ignored.
//! * `url` is mandatory and must be URL-shaped. Omitting it, leaving it empty, or passing
//!   a plain phrase (`url=Test summary line one`) makes the handler bail out: Telegram
//!   comes to the foreground and *nothing else happens*, with no error anywhere.
//! * The composed message is `url` + `\n` + `text` — the URL is not merely a validation
//!   gate, it lands in the message body. Confirmed by sharing to Saved Messages.
//! # Why only a short draft goes through this route
//!
//! The `tg://` handler degrades silently as the link grows. Measured on the same build,
//! with Cyrillic text (which percent-encodes to ~5.4 bytes per character):
//!
//! | summary text | `tg://` URL | result in the draft                     |
//! |--------------|-------------|-----------------------------------------|
//! | 18 chars     | ~130        | correct                                 |
//! | 999 chars    | 5 241       | correct, complete                       |
//! | 2 006 chars  | 10 428      | decoded but **silently truncated**      |
//! | ~3 500 chars | ~18 900     | **not decoded at all** — raw `%D0%9A…`  |
//!
//! macOS is not involved: a capture handler registered for a custom scheme receives the
//! URL byte-identical at up to 108 000 characters, and normalises raw text to exactly one
//! layer of percent-encoding either way. The cliffs are inside Telegram.
//!
//! So only [`DRAFT_TEXT_BUDGET`] characters — enough for a meeting's title and date — are
//! sent through the link. The summary body travels via the clipboard, which has no such
//! limit and cannot be corrupted.
//!
//! A meeting summary is not a URL, so [`SHARE_URL_LINE`] is prepended as the `url` and the
//! summary travels in `text`. That line is visible to the user in the draft and is theirs
//! to delete before sending; there is no parameter arrangement that avoids it. The draft is
//! editable and not sent automatically.

/// Longest single message Telegram accepts, in characters.
pub const TELEGRAM_MESSAGE_LIMIT: usize = 4096;

/// Characters of draft text the `tg://` link may carry.
///
/// 999 characters was verified correct and complete; 2 006 was silently truncated. This
/// sits an order of magnitude below the observed cliff because the real limit appears to be
/// on encoded URL length, which varies with how much of the text is non-ASCII — a budget
/// close to the cliff would hold for Russian and break for something denser.
pub const DRAFT_TEXT_BUDGET: usize = 400;

/// The mandatory `url` parameter, which becomes the first line of the draft.
///
/// Telegram's own domain is used deliberately: the parameter has to be a real URL, and a
/// link the recipient may well click must not point at a third party we do not control.
/// Change this one constant to move that line elsewhere.
pub const SHARE_URL_LINE: &str = "https://t.me";


/// Percent-encode per RFC 3986: everything outside the unreserved set becomes `%XX` for
/// each UTF-8 byte. Deliberately not `form_urlencoded`, which writes a space as `+` —
/// correct for HTML form bodies, but the `tg://` handler is not a form decoder and would
/// leave the pluses in the message text.
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Share URLs for `text`, in the order they should be attempted.
///
/// The `tg://` scheme reaches the installed client's chat picker directly. `t.me/share/url`
/// is the same action via the web, for when no application claims `tg:`.
pub fn share_urls(text: &str) -> [String; 2] {
    let url = percent_encode(SHARE_URL_LINE);
    let encoded = percent_encode(text);
    [
        format!("tg://msg_url?url={url}&text={encoded}"),
        format!("https://t.me/share/url?url={url}&text={encoded}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreserved_characters_pass_through() {
        assert_eq!(percent_encode("abcXYZ0189-._~"), "abcXYZ0189-._~");
    }

    #[test]
    fn spaces_and_delimiters_are_escaped_not_plus_encoded() {
        assert_eq!(percent_encode("a b&c=d#e"), "a%20b%26c%3Dd%23e");
    }

    #[test]
    fn cyrillic_is_encoded_per_utf8_byte() {
        // 'П' is U+041F -> D0 9F.
        assert_eq!(percent_encode("П"), "%D0%9F");
        assert_eq!(percent_encode("Итоги"), "%D0%98%D1%82%D0%BE%D0%B3%D0%B8");
    }

    #[test]
    fn newlines_survive_as_escapes() {
        assert_eq!(percent_encode("a\nb"), "a%0Ab");
    }

    /// The empty/absent `url` that made the picker never appear must not come back.
    #[test]
    fn url_parameter_is_always_a_real_url() {
        for candidate in share_urls("любой текст") {
            assert!(
                candidate.contains("url=https%3A%2F%2Ft.me&"),
                "share URL must carry a URL-shaped url parameter: {candidate}"
            );
            assert!(!candidate.contains("url=&"));
        }
        assert!(SHARE_URL_LINE.starts_with("https://"));
    }

    #[test]
    fn both_urls_carry_the_same_encoded_text() {
        let [scheme, web] = share_urls("Итоги встречи");
        let encoded = percent_encode("Итоги встречи");
        assert!(scheme.starts_with("tg://msg_url?url="));
        assert!(web.starts_with("https://t.me/share/url?url="));
        assert!(scheme.ends_with(&format!("&text={encoded}")));
        assert!(web.ends_with(&format!("&text={encoded}")));
        // No raw delimiter can leak out of the text and split the query string.
        assert!(!encoded.contains('&'));
        assert!(!encoded.contains('#'));
    }

    /// The draft budget must stay far below the 999-character length verified intact, and
    /// well below the 2 006 that silently truncated.
    #[test]
    fn draft_budget_stays_in_the_verified_safe_range() {
        assert!(DRAFT_TEXT_BUDGET <= 999);
        assert!(SHARE_URL_LINE.len() + 1 + DRAFT_TEXT_BUDGET <= TELEGRAM_MESSAGE_LIMIT);
    }
}
