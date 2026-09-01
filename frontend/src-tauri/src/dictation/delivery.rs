use std::fmt;

/// Clipboard operations required by the caret-delivery transaction.
///
/// Platform adapters may preserve richer clipboard formats internally. The
/// core only handles text and always asks the adapter to restore its snapshot.
pub trait ClipboardPort {
    type Snapshot;
    type Error: fmt::Display;

    fn snapshot(&mut self) -> Result<Self::Snapshot, Self::Error>;
    fn set_text(&mut self, text: &str) -> Result<(), Self::Error>;
    fn restore(&mut self, snapshot: Self::Snapshot) -> Result<(), Self::Error>;
}

/// Injects the platform paste gesture into the foreground application.
/// Pasting naturally replaces an active selection and otherwise inserts at
/// the current caret, exactly like keyboard input.
pub trait PastePort {
    type Error: fmt::Display;

    fn paste_at_caret(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryError {
    Snapshot(String),
    Stage(String),
    Paste(String),
    RestoreAfterPaste { paste: String, restore: String },
    Restore(String),
}

impl fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Snapshot(message) => write!(f, "could not snapshot clipboard: {message}"),
            Self::Stage(message) => write!(f, "could not stage dictation text: {message}"),
            Self::Paste(message) => write!(f, "could not paste at caret: {message}"),
            Self::RestoreAfterPaste { paste, restore } => write!(
                f,
                "could not paste at caret ({paste}); clipboard restore also failed ({restore})"
            ),
            Self::Restore(message) => {
                write!(f, "text was pasted but clipboard restore failed: {message}")
            }
        }
    }
}

impl std::error::Error for DeliveryError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeliveryReceipt {
    pub pasted: bool,
    pub clipboard_restored: bool,
}

/// Delivers text without leaving the user's clipboard overwritten.
///
/// The caller must save the transcript to history before invoking this
/// function. That makes every failure recoverable even if the target closes.
pub fn deliver_text<C, P>(
    clipboard: &mut C,
    paste: &mut P,
    text: &str,
) -> Result<DeliveryReceipt, DeliveryError>
where
    C: ClipboardPort,
    P: PastePort,
{
    let snapshot = clipboard
        .snapshot()
        .map_err(|error| DeliveryError::Snapshot(error.to_string()))?;

    if let Err(error) = clipboard.set_text(text) {
        let _ = clipboard.restore(snapshot);
        return Err(DeliveryError::Stage(error.to_string()));
    }

    let paste_result = paste.paste_at_caret();
    let restore_result = clipboard.restore(snapshot);

    match (paste_result, restore_result) {
        (Ok(()), Ok(())) => Ok(DeliveryReceipt {
            pasted: true,
            clipboard_restored: true,
        }),
        (Err(paste), Ok(())) => Err(DeliveryError::Paste(paste.to_string())),
        (Err(paste), Err(restore)) => Err(DeliveryError::RestoreAfterPaste {
            paste: paste.to_string(),
            restore: restore.to_string(),
        }),
        (Ok(()), Err(restore)) => Err(DeliveryError::Restore(restore.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeClipboard {
        value: String,
        fail_restore: bool,
    }

    impl ClipboardPort for FakeClipboard {
        type Snapshot = String;
        type Error = &'static str;

        fn snapshot(&mut self) -> Result<Self::Snapshot, Self::Error> {
            Ok(self.value.clone())
        }

        fn set_text(&mut self, text: &str) -> Result<(), Self::Error> {
            self.value = text.to_owned();
            Ok(())
        }

        fn restore(&mut self, snapshot: Self::Snapshot) -> Result<(), Self::Error> {
            if self.fail_restore {
                return Err("clipboard busy");
            }
            self.value = snapshot;
            Ok(())
        }
    }

    struct FakePaste {
        result: Result<(), &'static str>,
    }

    impl PastePort for FakePaste {
        type Error = &'static str;

        fn paste_at_caret(&mut self) -> Result<(), Self::Error> {
            self.result
        }
    }

    #[test]
    fn stages_text_pastes_and_restores_original_clipboard() {
        let mut clipboard = FakeClipboard {
            value: "original".into(),
            ..Default::default()
        };
        let mut paste = FakePaste { result: Ok(()) };

        let receipt = deliver_text(&mut clipboard, &mut paste, "dictated words").unwrap();

        assert!(receipt.pasted);
        assert!(receipt.clipboard_restored);
        assert_eq!(clipboard.value, "original");
    }

    #[test]
    fn paste_failure_still_restores_original_clipboard() {
        let mut clipboard = FakeClipboard {
            value: "original".into(),
            ..Default::default()
        };
        let mut paste = FakePaste {
            result: Err("target closed"),
        };

        let error = deliver_text(&mut clipboard, &mut paste, "dictated words").unwrap_err();

        assert_eq!(error, DeliveryError::Paste("target closed".into()));
        assert_eq!(clipboard.value, "original");
    }

    #[test]
    fn reports_successful_paste_even_when_restore_fails() {
        let mut clipboard = FakeClipboard {
            value: "original".into(),
            fail_restore: true,
        };
        let mut paste = FakePaste { result: Ok(()) };

        let error = deliver_text(&mut clipboard, &mut paste, "dictated words").unwrap_err();

        assert_eq!(error, DeliveryError::Restore("clipboard busy".into()));
        assert_eq!(clipboard.value, "dictated words");
    }
}
