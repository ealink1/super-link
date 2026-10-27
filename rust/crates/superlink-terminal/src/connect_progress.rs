//! Typed SSH setup progress.
//!
//! Events carry only a stage and a state, never server output or credentials,
//! so they can be forwarded to the UI thread as-is.

/// A real step of the SSH setup sequence, in presentation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConnectStage {
    Tcp,
    Auth,
    Channel,
    Ready,
}

impl ConnectStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tcp => "正在连接服务器",
            Self::Auth => "正在验证身份",
            Self::Channel => "正在打开终端",
            Self::Ready => "已连接",
        }
    }
}

/// Start, completion or failure of a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectState {
    Started,
    Completed,
    Failed,
}

/// One progress event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectEvent {
    pub stage: ConnectStage,
    pub state: ConnectState,
}

/// Reports progress synchronously while the caller blocks on setup.
pub struct ConnectProgress<'a> {
    observer: Option<&'a mut dyn FnMut(ConnectEvent)>,
    stage: ConnectStage,
}

impl<'a> ConnectProgress<'a> {
    pub fn new(observer: Option<&'a mut dyn FnMut(ConnectEvent)>) -> Self {
        Self {
            observer,
            stage: ConnectStage::Tcp,
        }
    }

    /// The stage that is currently in flight.
    pub fn stage(&self) -> ConnectStage {
        self.stage
    }

    pub fn report(&mut self, stage: ConnectStage, state: ConnectState) {
        self.stage = stage;
        if let Some(observer) = self.observer.as_deref_mut() {
            observer(ConnectEvent { stage, state });
        }
    }

    pub fn started(&mut self, stage: ConnectStage) {
        self.report(stage, ConnectState::Started);
    }

    pub fn completed(&mut self, stage: ConnectStage) {
        self.report(stage, ConnectState::Completed);
    }

    pub fn failed(&mut self) {
        let stage = self.stage;
        self.report(stage, ConnectState::Failed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_stay_in_presentation_order() {
        assert!(ConnectStage::Tcp < ConnectStage::Auth);
        assert!(ConnectStage::Auth < ConnectStage::Channel);
        assert!(ConnectStage::Channel < ConnectStage::Ready);
    }

    #[test]
    fn failure_is_reported_for_the_stage_in_flight() {
        let mut events = Vec::new();
        {
            let mut observer = |event| events.push(event);
            let mut progress = ConnectProgress::new(Some(&mut observer));
            progress.started(ConnectStage::Tcp);
            progress.completed(ConnectStage::Tcp);
            progress.started(ConnectStage::Auth);
            progress.failed();
            assert_eq!(progress.stage(), ConnectStage::Auth);
        }
        assert_eq!(
            events,
            vec![
                ConnectEvent {
                    stage: ConnectStage::Tcp,
                    state: ConnectState::Started
                },
                ConnectEvent {
                    stage: ConnectStage::Tcp,
                    state: ConnectState::Completed
                },
                ConnectEvent {
                    stage: ConnectStage::Auth,
                    state: ConnectState::Started
                },
                ConnectEvent {
                    stage: ConnectStage::Auth,
                    state: ConnectState::Failed
                },
            ]
        );
    }

    #[test]
    fn observers_are_optional() {
        let mut progress = ConnectProgress::new(None);
        progress.started(ConnectStage::Tcp);
        progress.completed(ConnectStage::Tcp);
        assert_eq!(progress.stage(), ConnectStage::Tcp);
    }
}
