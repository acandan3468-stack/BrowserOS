use std::fmt;

/// Browser lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserState {
    /// Browser process is starting up.
    Startup,
    /// Browser process is running and connected.
    Connected,
    /// Browser is fully operational.
    Running,
    /// Browser is in the process of shutting down.
    Closing,
    /// Browser has been closed.
    Closed,
    /// Browser process has crashed.
    Crashed,
    /// Browser is recovering from a crash.
    Recovering,
}

impl BrowserState {
    /// Returns `true` if this state represents an active browser.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Startup | Self::Connected | Self::Running | Self::Recovering
        )
    }

    /// Returns `true` if this state represents a terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Closed)
    }

    /// Returns `true` if this state indicates a failure.
    pub fn is_failure(self) -> bool {
        matches!(self, Self::Crashed)
    }

    /// Validates whether a transition from `self` to `target` is allowed.
    pub fn can_transition_to(self, target: BrowserState) -> bool {
        matches!(
            (self, target),
            (Self::Startup, Self::Connected)
                | (Self::Startup, Self::Crashed)
                | (Self::Connected, Self::Running)
                | (Self::Connected, Self::Closing)
                | (Self::Connected, Self::Crashed)
                | (Self::Running, Self::Closing)
                | (Self::Running, Self::Crashed)
                | (Self::Closing, Self::Closed)
                | (Self::Closing, Self::Crashed)
                | (Self::Crashed, Self::Recovering)
                | (Self::Recovering, Self::Running)
                | (Self::Recovering, Self::Crashed)
                | (Self::Recovering, Self::Closing)
        )
    }

    /// Attempt a transition, returning `Ok(target)` on success or
    /// `Err((self, target))` on invalid transition.
    pub fn transition_to(
        self,
        target: BrowserState,
    ) -> Result<BrowserState, (BrowserState, BrowserState)> {
        if self.can_transition_to(target) {
            Ok(target)
        } else {
            Err((self, target))
        }
    }
}

impl fmt::Display for BrowserState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Startup => write!(f, "startup"),
            Self::Connected => write!(f, "connected"),
            Self::Running => write!(f, "running"),
            Self::Closing => write!(f, "closing"),
            Self::Closed => write!(f, "closed"),
            Self::Crashed => write!(f, "crashed"),
            Self::Recovering => write!(f, "recovering"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_to_connected() {
        assert_eq!(
            BrowserState::Startup.transition_to(BrowserState::Connected),
            Ok(BrowserState::Connected)
        );
    }

    #[test]
    fn startup_to_crashed() {
        assert_eq!(
            BrowserState::Startup.transition_to(BrowserState::Crashed),
            Ok(BrowserState::Crashed)
        );
    }

    #[test]
    fn connected_to_running() {
        assert_eq!(
            BrowserState::Connected.transition_to(BrowserState::Running),
            Ok(BrowserState::Running)
        );
    }

    #[test]
    fn running_to_closing() {
        assert_eq!(
            BrowserState::Running.transition_to(BrowserState::Closing),
            Ok(BrowserState::Closing)
        );
    }

    #[test]
    fn closing_to_closed() {
        assert_eq!(
            BrowserState::Closing.transition_to(BrowserState::Closed),
            Ok(BrowserState::Closed)
        );
    }

    #[test]
    fn crashed_to_recovering() {
        assert_eq!(
            BrowserState::Crashed.transition_to(BrowserState::Recovering),
            Ok(BrowserState::Recovering)
        );
    }

    #[test]
    fn recovering_to_running() {
        assert_eq!(
            BrowserState::Recovering.transition_to(BrowserState::Running),
            Ok(BrowserState::Running)
        );
    }

    #[test]
    fn invalid_startup_to_closed() {
        assert!(BrowserState::Startup
            .transition_to(BrowserState::Closed)
            .is_err());
    }

    #[test]
    fn invalid_terminal_transition() {
        assert!(BrowserState::Closed
            .transition_to(BrowserState::Running)
            .is_err());
    }

    #[test]
    fn invalid_closed_to_startup() {
        assert!(BrowserState::Closed
            .transition_to(BrowserState::Startup)
            .is_err());
    }

    #[test]
    fn invalid_running_to_startup() {
        assert!(BrowserState::Running
            .transition_to(BrowserState::Startup)
            .is_err());
    }

    #[test]
    fn is_active_states() {
        assert!(BrowserState::Startup.is_active());
        assert!(BrowserState::Connected.is_active());
        assert!(BrowserState::Running.is_active());
        assert!(BrowserState::Recovering.is_active());
        assert!(!BrowserState::Closing.is_active());
        assert!(!BrowserState::Closed.is_active());
        assert!(!BrowserState::Crashed.is_active());
    }

    #[test]
    fn is_terminal() {
        assert!(BrowserState::Closed.is_terminal());
        assert!(!BrowserState::Running.is_terminal());
    }

    #[test]
    fn is_failure() {
        assert!(BrowserState::Crashed.is_failure());
        assert!(!BrowserState::Running.is_failure());
    }

    #[test]
    fn display() {
        assert_eq!(BrowserState::Startup.to_string(), "startup");
        assert_eq!(BrowserState::Connected.to_string(), "connected");
        assert_eq!(BrowserState::Running.to_string(), "running");
        assert_eq!(BrowserState::Closing.to_string(), "closing");
        assert_eq!(BrowserState::Closed.to_string(), "closed");
        assert_eq!(BrowserState::Crashed.to_string(), "crashed");
        assert_eq!(BrowserState::Recovering.to_string(), "recovering");
    }

    #[test]
    fn all_allowed_transitions() {
        let allowed: &[(BrowserState, BrowserState)] = &[
            (BrowserState::Startup, BrowserState::Connected),
            (BrowserState::Startup, BrowserState::Crashed),
            (BrowserState::Connected, BrowserState::Running),
            (BrowserState::Connected, BrowserState::Closing),
            (BrowserState::Connected, BrowserState::Crashed),
            (BrowserState::Running, BrowserState::Closing),
            (BrowserState::Running, BrowserState::Crashed),
            (BrowserState::Closing, BrowserState::Closed),
            (BrowserState::Closing, BrowserState::Crashed),
            (BrowserState::Crashed, BrowserState::Recovering),
            (BrowserState::Recovering, BrowserState::Running),
            (BrowserState::Recovering, BrowserState::Crashed),
            (BrowserState::Recovering, BrowserState::Closing),
        ];
        for &(from, to) in allowed {
            assert!(
                from.can_transition_to(to),
                "{from} -> {to} should be allowed"
            );
        }
    }

    #[test]
    fn all_disallowed_transitions() {
        let all_states = &[
            BrowserState::Startup,
            BrowserState::Connected,
            BrowserState::Running,
            BrowserState::Closing,
            BrowserState::Closed,
            BrowserState::Crashed,
            BrowserState::Recovering,
        ];
        let allowed: &[(BrowserState, BrowserState)] = &[
            (BrowserState::Startup, BrowserState::Connected),
            (BrowserState::Startup, BrowserState::Crashed),
            (BrowserState::Connected, BrowserState::Running),
            (BrowserState::Connected, BrowserState::Closing),
            (BrowserState::Connected, BrowserState::Crashed),
            (BrowserState::Running, BrowserState::Closing),
            (BrowserState::Running, BrowserState::Crashed),
            (BrowserState::Closing, BrowserState::Closed),
            (BrowserState::Closing, BrowserState::Crashed),
            (BrowserState::Crashed, BrowserState::Recovering),
            (BrowserState::Recovering, BrowserState::Running),
            (BrowserState::Recovering, BrowserState::Crashed),
            (BrowserState::Recovering, BrowserState::Closing),
        ];
        for from in all_states {
            for to in all_states {
                let is_allowed = allowed.contains(&(*from, *to));
                assert_eq!(from.can_transition_to(*to), is_allowed, "{from} -> {to}");
            }
        }
    }
}
