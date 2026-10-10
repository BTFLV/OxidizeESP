//! Long-running operation model: events, state transitions, cancellation
//! and progress reporting.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use ts_rs::TS;

use crate::{Error, ErrorKind, Result};

/// Kind of hardware operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OperationKind {
    Probe,
    FlashElf,
    WriteBin,
    ReadFlash,
    ReadPartitionTable,
    EraseFlash,
    EraseRegion,
    ErasePartitions,
    ChecksumMd5,
    Reset,
    HoldInReset,
}

impl OperationKind {
    /// Whether the operation modifies flash contents.
    pub fn is_destructive(self) -> bool {
        matches!(
            self,
            OperationKind::FlashElf
                | OperationKind::WriteBin
                | OperationKind::EraseFlash
                | OperationKind::EraseRegion
                | OperationKind::ErasePartitions
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            OperationKind::Probe => "Identifying device",
            OperationKind::FlashElf => "Flashing application",
            OperationKind::WriteBin => "Writing binary",
            OperationKind::ReadFlash => "Reading flash",
            OperationKind::ReadPartitionTable => "Reading partition table",
            OperationKind::EraseFlash => "Erasing flash",
            OperationKind::EraseRegion => "Erasing region",
            OperationKind::ErasePartitions => "Erasing partitions",
            OperationKind::ChecksumMd5 => "Calculating checksum",
            OperationKind::Reset => "Resetting device",
            OperationKind::HoldInReset => "Holding device in reset",
        }
    }
}

/// Phase of an operation, reported in [`JobEvent::Stage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Stage {
    Preparing,
    Connecting,
    Erasing,
    Writing,
    Verifying,
    Reading,
    Finalizing,
    Resetting,
}

/// Unit of the values in [`JobEvent::Progress`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ProgressUnit {
    Bytes,
    /// Device-side write blocks, as reported by espflash.
    Blocks,
}

/// Lifecycle state of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum JobState {
    Pending,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(self, JobState::Completed | JobState::Failed | JobState::Cancelled)
    }

    /// Validate a state transition.
    pub fn transition(self, next: JobState) -> Result<JobState> {
        use JobState::*;
        let allowed = matches!(
            (self, next),
            (Pending, Running)
                | (Pending, Cancelled)
                | (Pending, Failed)
                | (Running, Cancelling)
                | (Running, Completed)
                | (Running, Failed)
                | (Running, Cancelled)
                | (Cancelling, Cancelled)
                | (Cancelling, Completed)
                | (Cancelling, Failed)
        );
        if allowed {
            Ok(next)
        } else {
            Err(Error::internal(format!("Invalid job state transition from {self:?} to {next:?}.")))
        }
    }
}

/// Event emitted while a job runs. Delivered to the frontend in order.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "event", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum JobEvent {
    Started {
        job_id: String,
        kind: OperationKind,
        /// Whether the job can be stopped safely before it completes.
        cancellable: bool,
    },
    Stage {
        stage: Stage,
        message: String,
    },
    /// Progress of the current step. Only emitted when the underlying
    /// operation reports real progress.
    Progress {
        #[ts(type = "number")]
        current: u64,
        #[ts(type = "number")]
        total: u64,
        unit: ProgressUnit,
        /// Overall completion across all steps, from 0 to 1.
        overall: f64,
        /// Description of the current step, for example a flash address.
        detail: Option<String>,
    },
    Log {
        level: LogLevel,
        message: String,
    },
    Completed {
        message: String,
    },
    Failed {
        error: Error,
        /// Set when an interrupted write may have left the flash in an
        /// inconsistent state.
        flash_may_be_inconsistent: bool,
    },
    Cancelled {
        message: String,
        flash_may_be_inconsistent: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LogLevel {
    Info,
    Warning,
}

/// Receiver for job events. Implemented by the Tauri layer (forwarding to an
/// IPC channel) and by tests (collecting events).
pub trait EventSink: Send + Sync {
    fn emit(&self, event: JobEvent);
}

impl<F: Fn(JobEvent) + Send + Sync> EventSink for F {
    fn emit(&self, event: JobEvent) {
        self(event)
    }
}

/// Cooperative cancellation flag shared between the requester and the
/// worker thread.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() { Err(Error::cancelled()) } else { Ok(()) }
    }
}

/// Context passed to every operation: event sink, cancellation and progress
/// throttling.
pub struct JobContext<'a> {
    sink: &'a dyn EventSink,
    cancel: CancelToken,
    last_progress: Option<Instant>,
    min_interval: Duration,
    /// Set once data has been written to or erased from flash.
    flash_touched: bool,
}

impl<'a> JobContext<'a> {
    pub fn new(sink: &'a dyn EventSink, cancel: CancelToken) -> Self {
        Self { sink, cancel, last_progress: None, min_interval: Duration::from_millis(50), flash_touched: false }
    }

    pub fn cancel_token(&self) -> &CancelToken {
        &self.cancel
    }

    pub fn check_cancelled(&self) -> Result<()> {
        self.cancel.check()
    }

    pub fn stage(&mut self, stage: Stage, message: impl Into<String>) {
        self.sink.emit(JobEvent::Stage { stage, message: message.into() });
    }

    pub fn info(&self, message: impl Into<String>) {
        self.sink.emit(JobEvent::Log { level: LogLevel::Info, message: message.into() });
    }

    pub fn warn(&self, message: impl Into<String>) {
        self.sink.emit(JobEvent::Log { level: LogLevel::Warning, message: message.into() });
    }

    pub fn mark_flash_touched(&mut self) {
        self.flash_touched = true;
    }

    pub fn flash_touched(&self) -> bool {
        self.flash_touched
    }

    /// Report progress. Events are rate-limited, except for the final update
    /// of a step which is always delivered.
    pub fn progress(&mut self, current: u64, total: u64, unit: ProgressUnit, overall: f64, detail: Option<String>) {
        let now = Instant::now();
        let is_final = current >= total;
        if !is_final
            && let Some(last) = self.last_progress
            && now.duration_since(last) < self.min_interval
        {
            return;
        }
        self.last_progress = Some(now);
        self.sink.emit(JobEvent::Progress { current, total, unit, overall: overall.clamp(0.0, 1.0), detail });
    }

    pub(crate) fn emit(&self, event: JobEvent) {
        self.sink.emit(event);
    }
}

/// Emit the terminal event for an operation result.
pub fn finish<T>(ctx: &JobContext<'_>, result: &Result<T>, success_message: impl FnOnce(&T) -> String) {
    match result {
        Ok(value) => ctx.emit(JobEvent::Completed { message: success_message(value) }),
        Err(err) if err.kind == ErrorKind::Cancelled => ctx.emit(JobEvent::Cancelled {
            message: if ctx.flash_touched() {
                "Cancelled. Flash was partially modified and may be in an inconsistent state.".to_owned()
            } else {
                "Cancelled before the device was modified.".to_owned()
            },
            flash_may_be_inconsistent: ctx.flash_touched(),
        }),
        Err(err) => ctx.emit(JobEvent::Failed { error: err.clone(), flash_may_be_inconsistent: ctx.flash_touched() }),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Collect(Mutex<Vec<JobEvent>>);
    impl EventSink for Collect {
        fn emit(&self, event: JobEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn valid_transitions() {
        use JobState::*;
        assert_eq!(Pending.transition(Running).unwrap(), Running);
        assert_eq!(Running.transition(Cancelling).unwrap(), Cancelling);
        assert_eq!(Cancelling.transition(Cancelled).unwrap(), Cancelled);
        assert_eq!(Running.transition(Completed).unwrap(), Completed);
        // A cancellation request can race with completion.
        assert_eq!(Cancelling.transition(Completed).unwrap(), Completed);
    }

    #[test]
    fn invalid_transitions() {
        use JobState::*;
        assert!(Completed.transition(Running).is_err());
        assert!(Failed.transition(Completed).is_err());
        assert!(Pending.transition(Completed).is_err());
        assert!(Cancelled.transition(Cancelling).is_err());
        assert!(Completed.is_terminal() && Failed.is_terminal() && Cancelled.is_terminal());
        assert!(!Running.is_terminal());
    }

    #[test]
    fn cancel_token_is_shared() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert!(token.check().is_ok());
        clone.cancel();
        assert_eq!(token.check().unwrap_err().kind, ErrorKind::Cancelled);
    }

    #[test]
    fn progress_is_throttled_but_final_update_is_delivered() {
        let sink = Collect::default();
        let mut ctx = JobContext::new(&sink, CancelToken::new());
        for i in 0..=100 {
            ctx.progress(i, 100, ProgressUnit::Bytes, i as f64 / 100.0, None);
        }
        let events = sink.0.lock().unwrap();
        assert!(events.len() < 10, "expected throttling, got {} events", events.len());
        assert!(matches!(events.last(), Some(JobEvent::Progress { current: 100, .. })));
    }

    #[test]
    fn finish_reports_inconsistent_flash_on_cancel_after_write() {
        let sink = Collect::default();
        let mut ctx = JobContext::new(&sink, CancelToken::new());
        ctx.mark_flash_touched();
        finish::<()>(&ctx, &Err(Error::cancelled()), |_| String::new());
        let events = sink.0.lock().unwrap();
        assert!(matches!(events.last(), Some(JobEvent::Cancelled { flash_may_be_inconsistent: true, .. })));
    }

    #[test]
    fn destructive_operations() {
        assert!(OperationKind::EraseFlash.is_destructive());
        assert!(OperationKind::FlashElf.is_destructive());
        assert!(!OperationKind::ReadFlash.is_destructive());
        assert!(!OperationKind::ChecksumMd5.is_destructive());
    }
}
