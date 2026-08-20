// dictation/atspi_injector.rs (Linux only)
//
// Owns the AT-SPI2 accessibility-bus connection used to inject transcribed
// text into whichever text field currently has OS focus.
//
// API surface verified against docs.rs for `atspi 0.30.0` / `atspi-proxies
// 0.14.0` (the version pinned in Cargo.toml) before writing this file:
//   - `atspi::AccessibilityConnection::new()` / `.event_stream()` / `.register_event::<T>()`
//   - `atspi::events::object::StateChangedEvent { item: ObjectRefOwned, state: State, enabled: bool }`
//   - `atspi::proxy::accessible::ObjectRefExt::as_accessible_proxy(&self, &zbus::Connection)`
//   - `atspi::proxy::proxy_ext::ProxyExt::proxies(&AccessibleProxy) -> Proxies` and
//     `Proxies::text()` / `Proxies::editable_text()` are all `async fn`s in the pinned
//     version actually resolved by Cargo (confirmed by `cargo check`, not just docs.rs)
//   - `EditableTextProxy::insert_text(position: i32, text: &str, length: i32) -> zbus::Result<bool>`
//     where `position`/`length` are AT-SPI *character* offsets, not byte offsets.
//
// IMPORTANT CORRECTION vs the original plan: the plan asserted a
// `State::PasswordText` variant exists on `atspi_common::State`. It does not
// (verified against the published `atspi-common` source: the `State` bitflag
// enum has 44 variants and none of them is password-related). AT-SPI2
// represents password entries via the *role* `Role::PasswordText` (role 40,
// "a text object used for passwords, or other places where the text content
// is not shown visibly to the user") -- this is also how screen readers such
// as Orca detect password fields. Fix 2 below checks `get_role() ==
// Role::PasswordText`, not a nonexistent state.

use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::zbus;
use atspi::{events::object::StateChangedEvent, AccessibilityConnection};
use atspi::{Interface, ObjectRefOwned, Role, State};
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Why a single segment could not be injected. Both variants that indicate
/// "no safe place to type" (`NoFocusedEditableText`, `PasswordField`) are
/// handled identically by the caller: skip injection, fall back to clipboard.
#[derive(Debug, thiserror::Error)]
pub enum InjectError {
    #[error("no focused editable text field")]
    NoFocusedEditableText,
    #[error("focused field is a password field")]
    PasswordField,
    #[error("AT-SPI D-Bus call failed: {0}")]
    Zbus(#[from] zbus::Error),
}

/// Open a new connection to the AT-SPI accessibility bus.
pub async fn connect() -> Result<AccessibilityConnection, zbus::Error> {
    AccessibilityConnection::new()
        .await
        .map_err(|e| zbus::Error::Failure(format!("AT-SPI connection failed: {e}")))
}

/// A liveness cache of "the last object that received AT-SPI focus", kept up
/// to date by [`spawn_focus_tracker`]. Per Fix 3, this is a *liveness signal
/// only* -- `inject_segment` always re-validates it with a fresh query
/// immediately before inserting text, it never trusts the cache blindly.
#[derive(Clone)]
pub struct FocusCache {
    inner: Arc<RwLock<Option<ObjectRefOwned>>>,
}

impl FocusCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    async fn set(&self, item: Option<ObjectRefOwned>) {
        *self.inner.write().await = item;
    }

    /// Snapshot of the currently cached focus target. `pub` so callers
    /// (tests, diagnostics) can observe liveness without going through a
    /// full `inject_segment` call.
    pub async fn get(&self) -> Option<ObjectRefOwned> {
        self.inner.read().await.clone()
    }
}

/// Spawn a background task that subscribes to `object:state-changed:focused`
/// AT-SPI events and keeps `cache` pointed at the most recently focused
/// object. This is the liveness signal consumed (and re-validated) by
/// `inject_segment`.
pub fn spawn_focus_tracker(
    connection: AccessibilityConnection,
    cache: FocusCache,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = connection.register_event::<StateChangedEvent>().await {
            log::error!("Dictation: failed to register AT-SPI focus events: {}", e);
            return;
        }

        let events = connection.event_stream();
        let mut events = std::pin::pin!(events);

        log::info!("Dictation: AT-SPI focus tracker started");

        while let Some(result) = events.next().await {
            let event = match result {
                Ok(event) => event,
                Err(e) => {
                    log::debug!("Dictation: AT-SPI event stream error: {}", e);
                    continue;
                }
            };

            let Ok(state_changed) = StateChangedEvent::try_from(event) else {
                continue;
            };

            if state_changed.state == State::Focused {
                if state_changed.enabled {
                    cache.set(Some(state_changed.item)).await;
                } else if cache.get().await.as_ref() == Some(&state_changed.item) {
                    // The cached object explicitly lost focus and nothing new
                    // has taken it yet; clear so a stale target isn't reused.
                    cache.set(None).await;
                }
            }
        }

        log::info!("Dictation: AT-SPI focus tracker stopped (event stream ended)");
    })
}

/// Attempt to inject `text` at the end of whatever is currently focused.
///
/// Implements Fix 2 (password-field skip) and Fix 3 (fresh re-validation +
/// append-at-current-end, both queried immediately before the D-Bus
/// `InsertText` call, not read from any cache) on every call.
pub async fn inject_segment(
    connection: &AccessibilityConnection,
    cache: &FocusCache,
    text: &str,
) -> Result<(), InjectError> {
    let cached_item = cache.get().await.ok_or(InjectError::NoFocusedEditableText)?;

    let accessible: AccessibleProxy<'_> = cached_item
        .as_accessible_proxy(connection.connection())
        .await
        .map_err(|_| InjectError::NoFocusedEditableText)?;

    // Fix 3(a): re-confirm the cached object is still focused with a fresh,
    // minimal query immediately before acting on it. This narrows, but (as
    // documented in the plan) cannot fully eliminate, the TOCTOU window
    // inherent to an async D-Bus round-trip.
    let state = accessible.get_state().await?;
    if !state.contains(State::Focused) {
        return Err(InjectError::NoFocusedEditableText);
    }

    // Fix 2: never inject into a password field. Checked on every call, not
    // just once at focus-change time.
    let role = accessible.get_role().await?;
    if role == Role::PasswordText {
        return Err(InjectError::PasswordField);
    }

    let interfaces = accessible.get_interfaces().await?;
    if !interfaces.contains(Interface::EditableText) || !interfaces.contains(Interface::Text) {
        return Err(InjectError::NoFocusedEditableText);
    }

    let proxies = accessible
        .proxies()
        .await
        .map_err(|_| InjectError::NoFocusedEditableText)?;
    let text_proxy = proxies
        .text()
        .await
        .map_err(|_| InjectError::NoFocusedEditableText)?;
    let editable_proxy = proxies
        .editable_text()
        .await
        .map_err(|_| InjectError::NoFocusedEditableText)?;

    // Fix 3(b): always insert at the current end of the text content, queried
    // fresh immediately before `insert_text` -- never a remembered/predicted
    // caret offset.
    let end_position = text_proxy.character_count().await?;
    let char_len = text.chars().count() as i32;

    let inserted = editable_proxy
        .insert_text(end_position, text, char_len)
        .await?;
    if !inserted {
        return Err(InjectError::NoFocusedEditableText);
    }

    Ok(())
}
