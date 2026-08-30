// Minimal ES/EN string table for the handful of user-visible Rust surfaces
// (tray menu + OS notifications). Not a general i18n framework — see
// frontend/src/lib/app-i18n.ts for the much larger frontend TS equivalent,
// which this mirrors only in tone (informal "vos" register).

use crate::get_ui_language_internal;

fn is_en() -> bool {
    get_ui_language_internal().as_deref() == Some("en")
}

// ---- Tray menu ----

pub fn tray_downloading_model() -> &'static str {
    if is_en() { "⏳ Downloading transcription model..." } else { "⏳ Descargando modelo de transcripción..." }
}

pub fn tray_start_recording() -> &'static str {
    if is_en() { "Start Recording" } else { "Iniciar grabación" }
}

pub fn tray_starting_recording() -> &'static str {
    if is_en() { "🔄 Starting Recording..." } else { "🔄 Iniciando grabación..." }
}

pub fn tray_pause_recording() -> &'static str {
    if is_en() { "⏸ Pause Recording" } else { "⏸ Pausar grabación" }
}

pub fn tray_stop_recording() -> &'static str {
    if is_en() { "⏹ Stop Recording" } else { "⏹ Detener grabación" }
}

pub fn tray_pausing() -> &'static str {
    if is_en() { "⏸ Pausing..." } else { "⏸ Pausando..." }
}

pub fn tray_resume_recording() -> &'static str {
    if is_en() { "▶ Resume Recording" } else { "▶ Reanudar grabación" }
}

pub fn tray_resuming() -> &'static str {
    if is_en() { "▶ Resuming..." } else { "▶ Reanudando..." }
}

pub fn tray_stopping() -> &'static str {
    if is_en() { "⏹ Stopping..." } else { "⏹ Deteniendo..." }
}

pub fn tray_open_main_window() -> &'static str {
    if is_en() { "Open Main Window" } else { "Abrir ventana principal" }
}

pub fn tray_settings() -> &'static str {
    if is_en() { "Settings" } else { "Configuración" }
}

pub fn tray_check_updates() -> &'static str {
    if is_en() { "Check for Updates" } else { "Buscar actualizaciones" }
}

pub fn tray_quit() -> &'static str {
    if is_en() { "Quit" } else { "Salir" }
}

// ---- OS notifications ----

pub fn notif_recording_started(meeting_name: Option<&str>) -> String {
    match (meeting_name, is_en()) {
        (Some(name), true) => format!("Recording started for meeting: {}", name),
        (Some(name), false) => format!("Empezó la grabación de la reunión: {}", name),
        (None, true) => {
            "Recording has started. Please inform others in the meeting that you are recording."
                .to_string()
        }
        (None, false) => {
            "Empezó la grabación. Avisale a los demás que estás grabando la reunión.".to_string()
        }
    }
}

pub fn notif_recording_stopped() -> &'static str {
    if is_en() { "Recording has been stopped and saved" } else { "La grabación se detuvo y se guardó" }
}

/// Shorter fallback variant used by notifications/commands.rs direct-Tauri fallback path.
pub fn notif_recording_stopped_fallback() -> &'static str {
    if is_en() { "Recording has stopped" } else { "La grabación se detuvo" }
}

pub fn notif_recording_paused() -> &'static str {
    if is_en() { "Recording has been paused" } else { "La grabación se pausó" }
}

pub fn notif_recording_resumed() -> &'static str {
    if is_en() { "Recording has been resumed" } else { "La grabación se reanudó" }
}

pub fn notif_transcription_complete(file_path: Option<&str>) -> String {
    match (file_path, is_en()) {
        (Some(path), true) => format!("Transcription completed and saved to: {}", path),
        (Some(path), false) => format!("Se completó la transcripción y se guardó en: {}", path),
        (None, true) => "Transcription has been completed".to_string(),
        (None, false) => "Se completó la transcripción".to_string(),
    }
}

pub fn notif_meeting_reminder(minutes_until: u64, meeting_title: Option<&str>) -> String {
    match (meeting_title, is_en()) {
        (Some(title), true) => format!("Meeting '{}' starts in {} minutes", title, minutes_until),
        (Some(title), false) => format!("La reunión '{}' empieza en {} minutos", title, minutes_until),
        (None, true) => format!("Meeting starts in {} minutes", minutes_until),
        (None, false) => format!("La reunión empieza en {} minutos", minutes_until),
    }
}

pub fn notif_error_title() -> &'static str {
    if is_en() { "Meet4Specs Error" } else { "Error de Meet4Specs" }
}

pub fn notif_test() -> &'static str {
    if is_en() {
        "This is a test notification to verify the system is working correctly"
    } else {
        "Esta es una notificación de prueba para verificar que el sistema funciona correctamente"
    }
}
