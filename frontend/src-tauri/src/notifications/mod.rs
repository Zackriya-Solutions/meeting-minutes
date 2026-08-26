// Notification system module
pub mod commands;
pub mod manager;
pub mod settings;
pub mod system;
pub mod types;

// Re-export main types for easy access
pub use manager::NotificationManager;
pub use settings::{get_default_settings, ConsentManager, NotificationSettings};
pub use system::SystemNotificationHandler;
pub use types::{Notification, NotificationPriority, NotificationTimeout, NotificationType};

// Export commands for Tauri
pub use commands::{
    get_notification_settings, get_system_dnd_status, is_dnd_active,
    request_notification_permission, set_notification_settings, show_notification,
    show_test_notification,
};
