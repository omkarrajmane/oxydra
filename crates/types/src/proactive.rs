use crate::GatewayServerFrame;

/// Trait for sending proactive (non-request-driven) messages to a specific
/// channel context, e.g. scheduled task notifications to a Telegram chat.
pub trait ProactiveSender: Send + Sync {
    /// Send a server frame to the specified channel context.
    fn send_proactive(&self, channel_context_id: &str, frame: &GatewayServerFrame);
}

// --- Delivery streak tracking (Phase 3) ---

/// Route-generic outcome of a proactive delivery attempt.
/// Used by `DeliveryStreakUpdater` to track route health.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDeliveryOutcome {
    /// The primary (stored) route accepted the delivery.
    PrimaryRouteSucceeded,
    /// The primary route rejected delivery with a "route not found" error;
    /// fallback delivery to an alternative route succeeded.
    PrimaryRouteNotFoundFallbackSucceeded {
        /// The fallback route that worked (e.g. chat_id without thread).
        fallback_channel_context_id: String,
    },
    /// The primary route rejected with "route not found" and fallback also failed.
    PrimaryRouteNotFoundFallbackFailed,
}

/// Callback for reporting proactive delivery outcomes to durable storage.
///
/// Implementations track consecutive route-not-found failures and trigger
/// a route remap when the threshold is reached.
#[async_trait::async_trait]
pub trait DeliveryStreakUpdater: Send + Sync {
    /// Report the outcome of a proactive delivery for streak tracking.
    ///
    /// `schedule_id`: which schedule this delivery belongs to.
    /// `attempted_channel_context_id`: the route that was attempted — used as a
    ///   guard to prevent stale reports from corrupting a route that has already
    ///   been remapped or edited.
    /// `outcome`: what happened.
    async fn report_outcome(
        &self,
        schedule_id: &str,
        attempted_channel_context_id: &str,
        outcome: &RouteDeliveryOutcome,
    ) -> Result<(), crate::SchedulerError>;
}
