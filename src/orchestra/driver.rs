use std::path::PathBuf;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::ollama::client::OllamaClient;

use super::types::{FailurePolicy, FinalReport, OrchestratorState, TaskId, ValidationReport};

/// Events streamed to the TUI during an Orchestra run.
#[derive(Debug, Clone)]
pub enum DriverEvent {
    PhaseChanged(String),
    TaskStarted(TaskId),
    TaskProgress { note: String },
    TaskFinished { task_id: TaskId, verdict: String },
    UserEscalationNeeded { task_id: TaskId, reason: String, report: ValidationReport },
    RunFinished(FinalReport),
}

/// Decision sent back from the user during escalation.
#[derive(Debug, Clone)]
pub enum UserDecision {
    Retry { hint: Option<String> },
    Skip,
    Abort,
}

/// Start a new Orchestra run for a given goal.
pub async fn run_orchestra(
    _client: OllamaClient,
    _workspace: PathBuf,
    _goal: String,
    _policy: FailurePolicy,
    _context_size: u32,
    _tx: mpsc::UnboundedSender<DriverEvent>,
) -> Result<FinalReport> {
    anyhow::bail!("Orchestra mode is not yet implemented")
}

/// Resume a paused Orchestra run.
pub async fn resume_orchestra(
    _run_id: &str,
    _decision: Option<UserDecision>,
    _client: OllamaClient,
    _workspace: PathBuf,
    _config: Config,
    _tx: mpsc::UnboundedSender<DriverEvent>,
) -> Result<FinalReport> {
    anyhow::bail!("Orchestra resume is not yet implemented")
}
