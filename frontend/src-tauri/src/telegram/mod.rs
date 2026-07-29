//! Sharing a meeting summary to Telegram.
//!
//! This uses Telegram's *share deep link* (`tg://msg_url`, falling back to
//! `https://t.me/share/url`), not the Bot API: the app hands a short draft to the Telegram
//! client, which opens its own chat picker, and the user chooses the chat and presses send.
//! Nothing is transmitted by this process, and no bot token is stored.
//!
//! The link carries **only** the meeting's title and date. The summary body reaches the
//! chat through the clipboard, because the `tg://` handler silently truncates longer links
//! and then stops decoding them altogether — see [`share`] for the measured thresholds.
//!
//! Four consequences are structural, not oversights:
//!
//! * **The body is pasted, not prefilled.** Only a small draft can survive the link, so the
//!   user pastes the summary into the chat they picked.
//! * **No attachments.** A deep link carries text in a query parameter and has no file
//!   channel. Summaries past Telegram's per-message limit are also written to a `.md` file and
//!   revealed in the file manager so the user can drag them in ([`commands::save_summary_markdown_file`]).
//! * **No unattended send.** The chat picker always requires a human. The "auto-share"
//!   preference therefore opens the picker when a summary finishes rather than sending.
//! * **A URL on the first line.** The share action takes a URL plus an optional comment,
//!   and refuses to open at all without a URL-shaped one — so the draft always begins with
//!   [`share::SHARE_URL_LINE`], which the user deletes if they do not want it.
//!
//! All four would be lifted by adding a Bot API path (bot token + chat id); the text
//! formatting in [`share`] and the whole UI would carry over unchanged.

pub mod commands;
pub mod share;
