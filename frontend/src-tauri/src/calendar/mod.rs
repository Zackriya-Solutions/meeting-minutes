pub mod local_outlook;

#[cfg(target_os = "macos")]
mod macos_outlook;

#[cfg(target_os = "macos")]
mod macos_outlook_events;

#[cfg(target_os = "macos")]
mod macos_provider;

#[cfg(target_os = "windows")]
mod windows_outlook;
