use super::{
    C1EventPayload, CorrectionEventPayload, LifecycleEventPayload, VerificationEventPayload,
};

/// Payload union for one canonical match stream.
///
/// Live C1 actions and correction markers have different typed payload sets,
/// but they share one append-only aggregate stream. The union keeps that
/// distinction explicit until a derived effective view selects source events
/// for a typed projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchEventPayload {
    Lifecycle(LifecycleEventPayload),
    C1(C1EventPayload),
    Correction(CorrectionEventPayload),
    Verification(VerificationEventPayload),
}

impl MatchEventPayload {
    pub const fn lifecycle(payload: LifecycleEventPayload) -> Self {
        Self::Lifecycle(payload)
    }

    pub const fn c1(payload: C1EventPayload) -> Self {
        Self::C1(payload)
    }

    pub const fn correction(payload: CorrectionEventPayload) -> Self {
        Self::Correction(payload)
    }

    pub const fn as_c1(&self) -> Option<&C1EventPayload> {
        match self {
            Self::C1(payload) => Some(payload),
            Self::Lifecycle(_) | Self::Correction(_) | Self::Verification(_) => None,
        }
    }

    pub const fn as_correction(&self) -> Option<&CorrectionEventPayload> {
        match self {
            Self::Lifecycle(_) | Self::C1(_) | Self::Verification(_) => None,
            Self::Correction(payload) => Some(payload),
        }
    }

    pub const fn event_type_name(&self) -> &'static str {
        match self {
            Self::Lifecycle(payload) => payload.event_type().as_str(),
            Self::C1(payload) => payload.event_type().as_str(),
            Self::Correction(payload) => payload.event_type().as_str(),
            Self::Verification(payload) => payload.event_type().as_str(),
        }
    }
}

impl From<LifecycleEventPayload> for MatchEventPayload {
    fn from(payload: LifecycleEventPayload) -> Self {
        Self::lifecycle(payload)
    }
}

impl From<C1EventPayload> for MatchEventPayload {
    fn from(payload: C1EventPayload) -> Self {
        Self::c1(payload)
    }
}

impl From<CorrectionEventPayload> for MatchEventPayload {
    fn from(payload: CorrectionEventPayload) -> Self {
        Self::correction(payload)
    }
}

impl From<VerificationEventPayload> for MatchEventPayload {
    fn from(payload: VerificationEventPayload) -> Self {
        Self::Verification(payload)
    }
}
