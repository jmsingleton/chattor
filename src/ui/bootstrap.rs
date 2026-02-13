use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Status updates sent from the Tor bootstrap process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapUpdate {
    /// Progress percentage (0-100).
    Progress(u8),
    /// Tor connection established successfully.
    Connected,
    /// Tor connection failed with the given error message.
    Failed(String),
}

/// State machine for the bootstrap/splash screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapPhase {
    /// Actively connecting to the Tor network.
    Connecting {
        progress: u8,
        frame: usize,
        tick: u64,
    },
    /// Connection attempt failed.
    Failed {
        error: String,
        frame: usize,
        tick: u64,
    },
    /// Bootstrap complete, ready to transition to main UI.
    Done,
}

/// Actions produced by the bootstrap key handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapAction {
    /// Retry the Tor connection.
    Retry,
    /// Skip Tor and continue in offline mode.
    ContinueOffline,
    /// Quit the application.
    Quit,
}

impl BootstrapPhase {
    /// Create a new bootstrap phase in the initial Connecting state.
    pub fn new() -> Self {
        BootstrapPhase::Connecting {
            progress: 0,
            frame: 0,
            tick: 0,
        }
    }

    /// Advance the animation tick. The frame advances every 3 ticks.
    pub fn advance_tick(&mut self) {
        match self {
            BootstrapPhase::Connecting {
                tick, frame, ..
            } => {
                *tick += 1;
                if *tick % 3 == 0 {
                    *frame += 1;
                }
            }
            BootstrapPhase::Failed {
                tick, frame, ..
            } => {
                *tick += 1;
                if *tick % 3 == 0 {
                    *frame += 1;
                }
            }
            BootstrapPhase::Done => {}
        }
    }

    /// Update the progress percentage. Only effective in the Connecting state.
    pub fn set_progress(&mut self, value: u8) {
        if let BootstrapPhase::Connecting { progress, .. } = self {
            *progress = value;
        }
    }

    /// Transition to the Failed state with the given error message.
    /// Resets frame and tick to 0.
    pub fn fail(&mut self, error: String) {
        *self = BootstrapPhase::Failed {
            error,
            frame: 0,
            tick: 0,
        };
    }

    /// Transition to the Done state.
    pub fn done(&mut self) {
        *self = BootstrapPhase::Done;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_phase_starts_connecting() {
        let phase = BootstrapPhase::new();
        assert_eq!(
            phase,
            BootstrapPhase::Connecting {
                progress: 0,
                frame: 0,
                tick: 0,
            }
        );
    }

    #[test]
    fn bootstrap_phase_advance_tick() {
        let mut phase = BootstrapPhase::new();
        phase.advance_tick();
        assert_eq!(
            phase,
            BootstrapPhase::Connecting {
                progress: 0,
                frame: 0,
                tick: 1,
            }
        );
        phase.advance_tick();
        assert_eq!(
            phase,
            BootstrapPhase::Connecting {
                progress: 0,
                frame: 0,
                tick: 2,
            }
        );
    }

    #[test]
    fn bootstrap_phase_frame_advances_every_3_ticks() {
        let mut phase = BootstrapPhase::new();
        // Tick 1, 2: frame stays at 0
        phase.advance_tick();
        phase.advance_tick();
        if let BootstrapPhase::Connecting { frame, .. } = &phase {
            assert_eq!(*frame, 0);
        } else {
            panic!("expected Connecting state");
        }
        // Tick 3: frame advances to 1
        phase.advance_tick();
        if let BootstrapPhase::Connecting { frame, tick, .. } = &phase {
            assert_eq!(*tick, 3);
            assert_eq!(*frame, 1);
        } else {
            panic!("expected Connecting state");
        }
    }

    #[test]
    fn bootstrap_update_variants() {
        let progress = BootstrapUpdate::Progress(42);
        assert_eq!(progress, BootstrapUpdate::Progress(42));

        let connected = BootstrapUpdate::Connected;
        assert_eq!(connected, BootstrapUpdate::Connected);

        let failed = BootstrapUpdate::Failed("timeout".to_string());
        assert_eq!(failed, BootstrapUpdate::Failed("timeout".to_string()));
    }

    #[test]
    fn set_progress_updates_connecting() {
        let mut phase = BootstrapPhase::new();
        phase.set_progress(50);
        if let BootstrapPhase::Connecting { progress, .. } = &phase {
            assert_eq!(*progress, 50);
        } else {
            panic!("expected Connecting state");
        }
    }

    #[test]
    fn set_progress_ignored_in_failed() {
        let mut phase = BootstrapPhase::new();
        phase.fail("error".to_string());
        phase.set_progress(50);
        if let BootstrapPhase::Failed { error, .. } = &phase {
            assert_eq!(error, "error");
        } else {
            panic!("expected Failed state");
        }
    }

    #[test]
    fn fail_transitions_and_resets() {
        let mut phase = BootstrapPhase::new();
        phase.advance_tick();
        phase.advance_tick();
        phase.advance_tick();
        // Now tick=3, frame=1
        phase.fail("connection refused".to_string());
        assert_eq!(
            phase,
            BootstrapPhase::Failed {
                error: "connection refused".to_string(),
                frame: 0,
                tick: 0,
            }
        );
    }

    #[test]
    fn done_transitions() {
        let mut phase = BootstrapPhase::new();
        phase.done();
        assert_eq!(phase, BootstrapPhase::Done);
    }

    #[test]
    fn advance_tick_on_failed_state() {
        let mut phase = BootstrapPhase::new();
        phase.fail("error".to_string());
        phase.advance_tick();
        phase.advance_tick();
        phase.advance_tick();
        if let BootstrapPhase::Failed { tick, frame, .. } = &phase {
            assert_eq!(*tick, 3);
            assert_eq!(*frame, 1);
        } else {
            panic!("expected Failed state");
        }
    }

    #[test]
    fn advance_tick_on_done_is_noop() {
        let mut phase = BootstrapPhase::Done;
        phase.advance_tick();
        assert_eq!(phase, BootstrapPhase::Done);
    }
}
