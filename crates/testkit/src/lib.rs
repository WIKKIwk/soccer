#![forbid(unsafe_code)]

//! Deterministic fixtures and test helpers shared by application/integration tests.

use football_domain::match_engine::{MatchLifecycle, MatchState};

#[must_use]
pub fn confirmed_lineups() -> MatchLifecycle {
    let mut lifecycle = MatchLifecycle::new();
    lifecycle
        .transition_to(MatchState::Scheduled)
        .expect("draft matches can be scheduled");
    lifecycle
        .transition_to(MatchState::PreMatch)
        .expect("scheduled matches can enter pre-match");
    lifecycle
        .transition_to(MatchState::LineupsProvisional)
        .expect("pre-match matches can receive provisional lineups");
    lifecycle
        .transition_to(MatchState::LineupsConfirmed)
        .expect("provisional lineups can be confirmed");
    lifecycle
}

#[cfg(test)]
mod tests {
    use super::confirmed_lineups;
    use football_domain::match_engine::MatchState;

    #[test]
    fn fixture_returns_confirmed_lineups() {
        assert_eq!(confirmed_lineups().state(), MatchState::LineupsConfirmed);
    }
}
