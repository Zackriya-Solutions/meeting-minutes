// Notification system module
pub mod types;
pub mod system;
pub mod settings;
pub mod commands;
pub mod manager;

// Enhanced macOS notifications
#[cfg(target_os = "macos")]
pub mod enhanced_macos;

// Re-export main types for easy access
pub use types::{
    Notification, NotificationType, NotificationPriority, NotificationTimeout
};
pub use settings::{
    NotificationSettings, ConsentManager, get_default_settings
};
pub use manager::NotificationManager;
pub use system::SystemNotificationHandler;

// Export commands for Tauri
pub use commands::{
    get_notification_settings,
    set_notification_settings,
    request_notification_permission,
    show_notification,
    show_test_notification,
    is_dnd_active,
    get_system_dnd_status,
    show_enhanced_recording_confirmation,
    dismiss_all_enhanced_notifications,
    show_enhanced_recording_confirmation_internal,
};

// Export enhanced notification handlers only on macOS
#[cfg(target_os = "macos")]
pub use commands::setup_enhanced_notification_handlers;

// Re-export enhanced macOS notifications
#[cfg(target_os = "macos")]
pub use enhanced_macos::{
    EnhancedNotification, show_recording_confirmation, dismiss_all,
    setup_notification_confirm_handler, setup_notification_dismiss_handler,
};