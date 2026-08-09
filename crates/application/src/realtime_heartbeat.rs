use std::fmt;

pub const DEFAULT_REALTIME_MAX_INBOUND_FRAME_BYTES: usize = 64 * 1024;
pub const DEFAULT_REALTIME_MAX_SUBSCRIPTIONS: usize = 32;
pub const DEFAULT_REALTIME_MAX_PENDING_OUTBOUND_NOTIFICATIONS: usize = 256;
pub const DEFAULT_REALTIME_HEARTBEAT_INTERVAL_MILLIS: u64 = 15_000;
pub const DEFAULT_REALTIME_PONG_TIMEOUT_MILLIS: u64 = 10_000;

/// Adapter-facing safety limits for one realtime connection.
///
/// The values are deliberately transport-independent. An eventual WebSocket
/// adapter applies them before parsing inbound frames, adding subscriptions,
/// or enqueueing outbound notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealtimeConnectionLimits {
    pub max_inbound_frame_bytes: usize,
    pub max_subscriptions: usize,
    pub max_pending_outbound_notifications: usize,
    pub heartbeat_interval_millis: u64,
    pub pong_timeout_millis: u64,
}

impl Default for RealtimeConnectionLimits {
    fn default() -> Self {
        Self {
            max_inbound_frame_bytes: DEFAULT_REALTIME_MAX_INBOUND_FRAME_BYTES,
            max_subscriptions: DEFAULT_REALTIME_MAX_SUBSCRIPTIONS,
            max_pending_outbound_notifications: DEFAULT_REALTIME_MAX_PENDING_OUTBOUND_NOTIFICATIONS,
            heartbeat_interval_millis: DEFAULT_REALTIME_HEARTBEAT_INTERVAL_MILLIS,
            pong_timeout_millis: DEFAULT_REALTIME_PONG_TIMEOUT_MILLIS,
        }
    }
}

impl RealtimeConnectionLimits {
    pub fn validate(self) -> Result<(), RealtimeConnectionLimitsError> {
        if self.max_inbound_frame_bytes == 0 {
            return Err(RealtimeConnectionLimitsError::MaxInboundFrameBytesMustBePositive);
        }
        if self.max_subscriptions == 0 {
            return Err(RealtimeConnectionLimitsError::MaxSubscriptionsMustBePositive);
        }
        if self.max_pending_outbound_notifications == 0 {
            return Err(
                RealtimeConnectionLimitsError::MaxPendingOutboundNotificationsMustBePositive,
            );
        }
        if self.heartbeat_interval_millis == 0 {
            return Err(RealtimeConnectionLimitsError::HeartbeatIntervalMustBePositive);
        }
        if self.pong_timeout_millis == 0 {
            return Err(RealtimeConnectionLimitsError::PongTimeoutMustBePositive);
        }
        if self.pong_timeout_millis > self.heartbeat_interval_millis {
            return Err(RealtimeConnectionLimitsError::PongTimeoutExceedsHeartbeatInterval);
        }
        Ok(())
    }

    pub fn check_inbound_frame_bytes(
        self,
        actual_bytes: usize,
    ) -> Result<(), RealtimeConnectionLimitError> {
        if actual_bytes > self.max_inbound_frame_bytes {
            return Err(RealtimeConnectionLimitError::InboundFrameTooLarge {
                actual_bytes,
                max_bytes: self.max_inbound_frame_bytes,
            });
        }
        Ok(())
    }

    pub fn check_subscription_count(
        self,
        attempted_count: usize,
    ) -> Result<(), RealtimeConnectionLimitError> {
        if attempted_count > self.max_subscriptions {
            return Err(RealtimeConnectionLimitError::SubscriptionLimitExceeded {
                attempted_count,
                max_count: self.max_subscriptions,
            });
        }
        Ok(())
    }

    pub fn check_pending_outbound_count(
        self,
        attempted_count: usize,
    ) -> Result<(), RealtimeConnectionLimitError> {
        if attempted_count > self.max_pending_outbound_notifications {
            return Err(RealtimeConnectionLimitError::PendingOutboundLimitExceeded {
                attempted_count,
                max_count: self.max_pending_outbound_notifications,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeConnectionLimitsError {
    MaxInboundFrameBytesMustBePositive,
    MaxSubscriptionsMustBePositive,
    MaxPendingOutboundNotificationsMustBePositive,
    HeartbeatIntervalMustBePositive,
    PongTimeoutMustBePositive,
    PongTimeoutExceedsHeartbeatInterval,
}

impl fmt::Display for RealtimeConnectionLimitsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MaxInboundFrameBytesMustBePositive => "max_inbound_frame_bytes must be positive",
            Self::MaxSubscriptionsMustBePositive => "max_subscriptions must be positive",
            Self::MaxPendingOutboundNotificationsMustBePositive => {
                "max_pending_outbound_notifications must be positive"
            }
            Self::HeartbeatIntervalMustBePositive => "heartbeat_interval_millis must be positive",
            Self::PongTimeoutMustBePositive => "pong_timeout_millis must be positive",
            Self::PongTimeoutExceedsHeartbeatInterval => {
                "pong_timeout_millis cannot exceed heartbeat_interval_millis"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for RealtimeConnectionLimitsError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeConnectionLimitError {
    InboundFrameTooLarge {
        actual_bytes: usize,
        max_bytes: usize,
    },
    SubscriptionLimitExceeded {
        attempted_count: usize,
        max_count: usize,
    },
    PendingOutboundLimitExceeded {
        attempted_count: usize,
        max_count: usize,
    },
}

impl fmt::Display for RealtimeConnectionLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InboundFrameTooLarge {
                actual_bytes,
                max_bytes,
            } => write!(
                formatter,
                "realtime inbound frame is {actual_bytes} bytes, maximum is {max_bytes}"
            ),
            Self::SubscriptionLimitExceeded {
                attempted_count,
                max_count,
            } => write!(
                formatter,
                "realtime subscription count {attempted_count} exceeds maximum {max_count}"
            ),
            Self::PendingOutboundLimitExceeded {
                attempted_count,
                max_count,
            } => write!(
                formatter,
                "realtime pending outbound count {attempted_count} exceeds maximum {max_count}"
            ),
        }
    }
}

impl std::error::Error for RealtimeConnectionLimitError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeHeartbeatAction {
    Noop,
    SendPing,
    CloseUnresponsive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimePongDisposition {
    Accepted,
    Ignored,
    Late,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeHeartbeatError {
    InvalidLimits(RealtimeConnectionLimitsError),
    ClockMovedBackwards {
        previous_millis: u64,
        now_millis: u64,
    },
}

impl fmt::Display for RealtimeHeartbeatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits(error) => {
                write!(formatter, "invalid realtime heartbeat limits: {error}")
            }
            Self::ClockMovedBackwards {
                previous_millis,
                now_millis,
            } => write!(
                formatter,
                "realtime heartbeat clock moved backwards from {previous_millis} to {now_millis} milliseconds"
            ),
        }
    }
}

impl std::error::Error for RealtimeHeartbeatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidLimits(error) => Some(error),
            Self::ClockMovedBackwards { .. } => None,
        }
    }
}

/// Monotonic heartbeat state for one connection.
///
/// `now_millis` values come from the adapter's monotonic clock, not wall-clock
/// UTC timestamps. The state machine uses native WebSocket Ping/Pong control
/// frames; heartbeat messages are not JSON subscription frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealtimeHeartbeatState {
    started_at_millis: u64,
    last_observed_at_millis: u64,
    last_ping_at_millis: Option<u64>,
    awaiting_pong_since_millis: Option<u64>,
    closed: bool,
}

impl RealtimeHeartbeatState {
    pub const fn new(now_millis: u64) -> Self {
        Self {
            started_at_millis: now_millis,
            last_observed_at_millis: now_millis,
            last_ping_at_millis: None,
            awaiting_pong_since_millis: None,
            closed: false,
        }
    }

    pub const fn is_awaiting_pong(self) -> bool {
        self.awaiting_pong_since_millis.is_some()
    }

    pub const fn is_closed(self) -> bool {
        self.closed
    }

    pub fn on_tick(
        &mut self,
        now_millis: u64,
        limits: RealtimeConnectionLimits,
    ) -> Result<RealtimeHeartbeatAction, RealtimeHeartbeatError> {
        limits
            .validate()
            .map_err(RealtimeHeartbeatError::InvalidLimits)?;
        self.observe_time(now_millis)?;

        if self.closed {
            return Ok(RealtimeHeartbeatAction::CloseUnresponsive);
        }

        if let Some(ping_sent_at) = self.awaiting_pong_since_millis {
            if now_millis.saturating_sub(ping_sent_at) >= limits.pong_timeout_millis {
                self.closed = true;
                return Ok(RealtimeHeartbeatAction::CloseUnresponsive);
            }
            return Ok(RealtimeHeartbeatAction::Noop);
        }

        let last_probe_at = self.last_ping_at_millis.unwrap_or(self.started_at_millis);
        if now_millis.saturating_sub(last_probe_at) < limits.heartbeat_interval_millis {
            return Ok(RealtimeHeartbeatAction::Noop);
        }

        self.last_ping_at_millis = Some(now_millis);
        self.awaiting_pong_since_millis = Some(now_millis);
        Ok(RealtimeHeartbeatAction::SendPing)
    }

    pub fn on_pong(
        &mut self,
        now_millis: u64,
        limits: RealtimeConnectionLimits,
    ) -> Result<RealtimePongDisposition, RealtimeHeartbeatError> {
        limits
            .validate()
            .map_err(RealtimeHeartbeatError::InvalidLimits)?;
        self.observe_time(now_millis)?;

        let Some(ping_sent_at) = self.awaiting_pong_since_millis else {
            return Ok(RealtimePongDisposition::Ignored);
        };
        if self.closed || now_millis.saturating_sub(ping_sent_at) >= limits.pong_timeout_millis {
            self.closed = true;
            return Ok(RealtimePongDisposition::Late);
        }

        self.awaiting_pong_since_millis = None;
        Ok(RealtimePongDisposition::Accepted)
    }

    fn observe_time(&mut self, now_millis: u64) -> Result<(), RealtimeHeartbeatError> {
        if now_millis < self.last_observed_at_millis {
            return Err(RealtimeHeartbeatError::ClockMovedBackwards {
                previous_millis: self.last_observed_at_millis,
                now_millis,
            });
        }
        self.last_observed_at_millis = now_millis;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_REALTIME_HEARTBEAT_INTERVAL_MILLIS, DEFAULT_REALTIME_MAX_INBOUND_FRAME_BYTES,
        DEFAULT_REALTIME_MAX_PENDING_OUTBOUND_NOTIFICATIONS, DEFAULT_REALTIME_MAX_SUBSCRIPTIONS,
        DEFAULT_REALTIME_PONG_TIMEOUT_MILLIS, RealtimeConnectionLimitError,
        RealtimeConnectionLimits, RealtimeConnectionLimitsError, RealtimeHeartbeatAction,
        RealtimeHeartbeatError, RealtimeHeartbeatState, RealtimePongDisposition,
    };

    #[test]
    fn default_limits_are_positive_and_ordered_for_heartbeat() {
        let limits = RealtimeConnectionLimits::default();
        assert_eq!(
            limits.max_inbound_frame_bytes,
            DEFAULT_REALTIME_MAX_INBOUND_FRAME_BYTES
        );
        assert_eq!(limits.max_subscriptions, DEFAULT_REALTIME_MAX_SUBSCRIPTIONS);
        assert_eq!(
            limits.max_pending_outbound_notifications,
            DEFAULT_REALTIME_MAX_PENDING_OUTBOUND_NOTIFICATIONS
        );
        assert_eq!(
            limits.heartbeat_interval_millis,
            DEFAULT_REALTIME_HEARTBEAT_INTERVAL_MILLIS
        );
        assert_eq!(
            limits.pong_timeout_millis,
            DEFAULT_REALTIME_PONG_TIMEOUT_MILLIS
        );
        assert!(limits.validate().is_ok());
    }

    #[test]
    fn rejects_zero_or_misordered_limits() {
        let zero_frame_limits = RealtimeConnectionLimits {
            max_inbound_frame_bytes: 0,
            ..RealtimeConnectionLimits::default()
        };
        assert_eq!(
            zero_frame_limits.validate(),
            Err(RealtimeConnectionLimitsError::MaxInboundFrameBytesMustBePositive)
        );
        assert_eq!(
            RealtimeHeartbeatState::new(0).on_tick(0, zero_frame_limits),
            Err(RealtimeHeartbeatError::InvalidLimits(
                RealtimeConnectionLimitsError::MaxInboundFrameBytesMustBePositive
            ))
        );
        assert_eq!(
            RealtimeConnectionLimits {
                heartbeat_interval_millis: 10,
                pong_timeout_millis: 11,
                ..RealtimeConnectionLimits::default()
            }
            .validate(),
            Err(RealtimeConnectionLimitsError::PongTimeoutExceedsHeartbeatInterval)
        );
    }

    #[test]
    fn enforces_frame_subscription_and_pending_queue_boundaries() {
        let limits = RealtimeConnectionLimits {
            max_inbound_frame_bytes: 10,
            max_subscriptions: 2,
            max_pending_outbound_notifications: 3,
            ..RealtimeConnectionLimits::default()
        };
        assert_eq!(limits.check_inbound_frame_bytes(10), Ok(()));
        assert_eq!(
            limits.check_inbound_frame_bytes(11),
            Err(RealtimeConnectionLimitError::InboundFrameTooLarge {
                actual_bytes: 11,
                max_bytes: 10
            })
        );
        assert_eq!(limits.check_subscription_count(2), Ok(()));
        assert!(matches!(
            limits.check_subscription_count(3),
            Err(RealtimeConnectionLimitError::SubscriptionLimitExceeded { .. })
        ));
        assert_eq!(limits.check_pending_outbound_count(3), Ok(()));
        assert!(matches!(
            limits.check_pending_outbound_count(4),
            Err(RealtimeConnectionLimitError::PendingOutboundLimitExceeded { .. })
        ));
    }

    #[test]
    fn sends_ping_only_at_interval_and_accepts_a_timely_pong() {
        let limits = RealtimeConnectionLimits {
            heartbeat_interval_millis: 100,
            pong_timeout_millis: 50,
            ..RealtimeConnectionLimits::default()
        };
        let mut heartbeat = RealtimeHeartbeatState::new(0);
        assert_eq!(
            heartbeat.on_tick(99, limits),
            Ok(RealtimeHeartbeatAction::Noop)
        );
        assert_eq!(
            heartbeat.on_tick(100, limits),
            Ok(RealtimeHeartbeatAction::SendPing)
        );
        assert!(heartbeat.is_awaiting_pong());
        assert_eq!(
            heartbeat.on_tick(149, limits),
            Ok(RealtimeHeartbeatAction::Noop)
        );
        assert_eq!(
            heartbeat.on_pong(149, limits),
            Ok(RealtimePongDisposition::Accepted)
        );
        assert!(!heartbeat.is_awaiting_pong());
        assert_eq!(
            heartbeat.on_tick(199, limits),
            Ok(RealtimeHeartbeatAction::Noop)
        );
        assert_eq!(
            heartbeat.on_tick(200, limits),
            Ok(RealtimeHeartbeatAction::SendPing)
        );
    }

    #[test]
    fn closes_when_pong_is_missing_and_rejects_late_pong() {
        let limits = RealtimeConnectionLimits {
            heartbeat_interval_millis: 100,
            pong_timeout_millis: 50,
            ..RealtimeConnectionLimits::default()
        };
        let mut heartbeat = RealtimeHeartbeatState::new(0);
        assert_eq!(
            heartbeat.on_tick(100, limits),
            Ok(RealtimeHeartbeatAction::SendPing)
        );
        assert_eq!(
            heartbeat.on_pong(150, limits),
            Ok(RealtimePongDisposition::Late)
        );
        assert!(heartbeat.is_closed());
        assert_eq!(
            heartbeat.on_tick(151, limits),
            Ok(RealtimeHeartbeatAction::CloseUnresponsive)
        );
    }

    #[test]
    fn ignores_unsolicited_pong_and_rejects_backwards_time() {
        let limits = RealtimeConnectionLimits::default();
        let mut heartbeat = RealtimeHeartbeatState::new(100);
        assert_eq!(
            heartbeat.on_pong(100, limits),
            Ok(RealtimePongDisposition::Ignored)
        );
        assert_eq!(
            heartbeat.on_tick(99, limits),
            Err(RealtimeHeartbeatError::ClockMovedBackwards {
                previous_millis: 100,
                now_millis: 99
            })
        );
    }
}
