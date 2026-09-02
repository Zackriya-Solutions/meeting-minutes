use super::ActivationEvent;
use tokio::sync::broadcast;

/// Connects the desktop shortcut adapter to the dictation coordinator without
/// coupling either side to Tauri or Win32 types.
#[derive(Clone)]
pub struct ActivationBus {
    sender: broadcast::Sender<ActivationEvent>,
}

impl ActivationBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(16);
        Self { sender }
    }

    pub fn publish(&self, event: ActivationEvent) {
        // It is valid to publish before the coordinator subscribes during app
        // startup. No recording exists yet, so there is nothing to recover.
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ActivationEvent> {
        self.sender.subscribe()
    }
}

impl Default for ActivationBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn forwards_activation_to_subscribers() {
        let bus = ActivationBus::new();
        let mut receiver = bus.subscribe();

        bus.publish(ActivationEvent::Started);
        bus.publish(ActivationEvent::Stopped);

        assert_eq!(receiver.recv().await.unwrap(), ActivationEvent::Started);
        assert_eq!(receiver.recv().await.unwrap(), ActivationEvent::Stopped);
    }
}
