//! Handle for triggering reload operations.

use tokio::sync::watch;

use super::types::ReloadSignal;

/// Handle for triggering reload operations.
///
/// This handle can be cloned and shared across tasks to trigger reloads
/// from anywhere in the application.
#[derive(Clone)]
pub struct ReloadHandle {
    sender: watch::Sender<Option<ReloadSignal>>,
}

impl ReloadHandle {
    /// Create a new reload handle and receiver pair.
    ///
    /// The handle is used to trigger reloads, while the receiver is used
    /// by the load balancer to listen for reload signals.
    pub fn new() -> (Self, watch::Receiver<Option<ReloadSignal>>) {
        let (sender, receiver) = watch::channel(None);
        (Self { sender }, receiver)
    }

    /// Trigger a reload with the given backend addresses.
    ///
    /// This sends a signal to update the backend list. The load balancer
    /// will apply the changes gracefully, draining removed backends before
    /// removing them.
    #[allow(dead_code)]
    pub fn trigger_reload(&self, backends: Vec<String>) -> bool {
        let signal = ReloadSignal::new(backends);
        self.sender.send(Some(signal)).is_ok()
    }

    /// Check if there are any active receivers.
    #[allow(dead_code)]
    pub fn has_receivers(&self) -> bool {
        self.sender.receiver_count() > 0
    }
}

impl Default for ReloadHandle {
    fn default() -> Self {
        Self::new().0
    }
}
