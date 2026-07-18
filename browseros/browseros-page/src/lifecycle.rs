use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageState {
    Creating,
    Loading,
    Interactive,
    Complete,
    Closing,
    Closed,
    Crashed,
}

impl PageState {
    pub fn can_transition_to(&self, next: PageState) -> bool {
        matches!(
            (self, next),
            (PageState::Creating, PageState::Loading)
                | (PageState::Loading, PageState::Interactive)
                | (PageState::Loading, PageState::Complete)
                | (PageState::Loading, PageState::Crashed)
                | (PageState::Interactive, PageState::Complete)
                | (PageState::Interactive, PageState::Crashed)
                | (PageState::Complete, PageState::Loading)
                | (PageState::Complete, PageState::Crashed)
                | (PageState::Complete, PageState::Closing)
                | (PageState::Closing, PageState::Closed)
                | (PageState::Closing, PageState::Crashed)
                | (PageState::Closed, PageState::Creating)
                | (PageState::Crashed, PageState::Creating)
        )
    }

    pub fn transition_to(&self, next: PageState) -> Result<PageState, PageStateError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(PageStateError::InvalidTransition {
                from: *self,
                to: next,
            })
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            PageState::Creating | PageState::Loading | PageState::Interactive | PageState::Complete
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, PageState::Closed | PageState::Crashed)
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, PageState::Crashed)
    }
}

impl fmt::Display for PageState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageState::Creating => write!(f, "creating"),
            PageState::Loading => write!(f, "loading"),
            PageState::Interactive => write!(f, "interactive"),
            PageState::Complete => write!(f, "complete"),
            PageState::Closing => write!(f, "closing"),
            PageState::Closed => write!(f, "closed"),
            PageState::Crashed => write!(f, "crashed"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PageStateError {
    InvalidTransition { from: PageState, to: PageState },
}

impl fmt::Display for PageStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageStateError::InvalidTransition { from, to } => {
                write!(f, "invalid page state transition: {:?} -> {:?}", from, to)
            }
        }
    }
}

impl std::error::Error for PageStateError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creating_to_loading() {
        assert!(PageState::Creating.can_transition_to(PageState::Loading));
    }

    #[test]
    fn test_loading_to_interactive() {
        assert!(PageState::Loading.can_transition_to(PageState::Interactive));
    }

    #[test]
    fn test_loading_to_complete() {
        assert!(PageState::Loading.can_transition_to(PageState::Complete));
    }

    #[test]
    fn test_loading_to_crashed() {
        assert!(PageState::Loading.can_transition_to(PageState::Crashed));
    }

    #[test]
    fn test_interactive_to_complete() {
        assert!(PageState::Interactive.can_transition_to(PageState::Complete));
    }

    #[test]
    fn test_interactive_to_crashed() {
        assert!(PageState::Interactive.can_transition_to(PageState::Crashed));
    }

    #[test]
    fn test_complete_to_loading() {
        assert!(PageState::Complete.can_transition_to(PageState::Loading));
    }

    #[test]
    fn test_complete_to_crashed() {
        assert!(PageState::Complete.can_transition_to(PageState::Crashed));
    }

    #[test]
    fn test_complete_to_closing() {
        assert!(PageState::Complete.can_transition_to(PageState::Closing));
    }

    #[test]
    fn test_closing_to_closed() {
        assert!(PageState::Closing.can_transition_to(PageState::Closed));
    }

    #[test]
    fn test_closing_to_crashed() {
        assert!(PageState::Closing.can_transition_to(PageState::Crashed));
    }

    #[test]
    fn test_closed_to_creating() {
        assert!(PageState::Closed.can_transition_to(PageState::Creating));
    }

    #[test]
    fn test_crashed_to_creating() {
        assert!(PageState::Crashed.can_transition_to(PageState::Creating));
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(!PageState::Creating.can_transition_to(PageState::Complete));
        assert!(!PageState::Creating.can_transition_to(PageState::Closed));
        assert!(!PageState::Creating.can_transition_to(PageState::Crashed));
        assert!(!PageState::Loading.can_transition_to(PageState::Closing));
        assert!(!PageState::Loading.can_transition_to(PageState::Closed));
        assert!(!PageState::Interactive.can_transition_to(PageState::Closed));
        assert!(!PageState::Interactive.can_transition_to(PageState::Closing));
        assert!(!PageState::Interactive.can_transition_to(PageState::Loading));
        assert!(!PageState::Complete.can_transition_to(PageState::Interactive));
        assert!(!PageState::Complete.can_transition_to(PageState::Closed));
        assert!(!PageState::Closed.can_transition_to(PageState::Loading));
        assert!(!PageState::Closed.can_transition_to(PageState::Complete));
        assert!(!PageState::Closed.can_transition_to(PageState::Crashed));
        assert!(!PageState::Crashed.can_transition_to(PageState::Complete));
        assert!(!PageState::Crashed.can_transition_to(PageState::Loading));
    }

    #[test]
    fn test_transition_ok() {
        let result = PageState::Creating.transition_to(PageState::Loading);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PageState::Loading);
    }

    #[test]
    fn test_transition_err() {
        let result = PageState::Creating.transition_to(PageState::Closed);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_active() {
        assert!(PageState::Creating.is_active());
        assert!(PageState::Loading.is_active());
        assert!(PageState::Interactive.is_active());
        assert!(PageState::Complete.is_active());
        assert!(!PageState::Closing.is_active());
        assert!(!PageState::Closed.is_active());
        assert!(!PageState::Crashed.is_active());
    }

    #[test]
    fn test_is_terminal() {
        assert!(!PageState::Creating.is_terminal());
        assert!(!PageState::Loading.is_terminal());
        assert!(!PageState::Interactive.is_terminal());
        assert!(!PageState::Complete.is_terminal());
        assert!(!PageState::Closing.is_terminal());
        assert!(PageState::Closed.is_terminal());
        assert!(PageState::Crashed.is_terminal());
    }

    #[test]
    fn test_is_failure() {
        assert!(!PageState::Creating.is_failure());
        assert!(!PageState::Loading.is_failure());
        assert!(!PageState::Interactive.is_failure());
        assert!(!PageState::Complete.is_failure());
        assert!(!PageState::Closing.is_failure());
        assert!(!PageState::Closed.is_failure());
        assert!(PageState::Crashed.is_failure());
    }

    #[test]
    fn test_display() {
        assert_eq!(PageState::Creating.to_string(), "creating");
        assert_eq!(PageState::Loading.to_string(), "loading");
        assert_eq!(PageState::Interactive.to_string(), "interactive");
        assert_eq!(PageState::Complete.to_string(), "complete");
        assert_eq!(PageState::Closing.to_string(), "closing");
        assert_eq!(PageState::Closed.to_string(), "closed");
        assert_eq!(PageState::Crashed.to_string(), "crashed");
    }

    #[test]
    fn test_all_allowed_transitions() {
        let transitions = vec![
            (PageState::Creating, PageState::Loading),
            (PageState::Loading, PageState::Interactive),
            (PageState::Loading, PageState::Complete),
            (PageState::Loading, PageState::Crashed),
            (PageState::Interactive, PageState::Complete),
            (PageState::Interactive, PageState::Crashed),
            (PageState::Complete, PageState::Loading),
            (PageState::Complete, PageState::Crashed),
            (PageState::Complete, PageState::Closing),
            (PageState::Closing, PageState::Closed),
            (PageState::Closing, PageState::Crashed),
            (PageState::Closed, PageState::Creating),
            (PageState::Crashed, PageState::Creating),
        ];
        for (from, to) in &transitions {
            assert!(
                from.can_transition_to(*to),
                "expected {:?} -> {:?} to be allowed",
                from,
                to
            );
        }
    }

    #[test]
    fn test_state_error_display() {
        let err = PageStateError::InvalidTransition {
            from: PageState::Creating,
            to: PageState::Closed,
        };
        let msg = err.to_string();
        assert!(msg.contains("invalid page state transition"));
        assert!(msg.contains("Creating"));
        assert!(msg.contains("Closed"));
    }
}
