---
layout: default
title: 07 — Driver
parent: Orchestra Mode
nav_order: 7
---

# 07 — Driver

The driver is the Rust state machine that owns the run. It calls planner,
worker, validator, and diagnoser in the right order. It persists state
after every phase transition.

## Location

`src/orchestra/driver.rs`

## Entry point

```rust
pub async fn run_orchestra(
    client: &OllamaClient,
    workspace: &Path,
    goal: String,
    mode: FailurePolicy,
    ctx_size: u32,
    event_tx: mpsc::UnboundedSender<DriverEvent>,
) -> Result<FinalReport>;
```

`DriverEvent` is used by the TUI to render progress. It is not part of the
persisted state.

```rust
pub enum DriverEvent {
    PhaseChanged(OrchestratorPhase),
    TaskStarted(TaskId),
    TaskProgress { task_id: TaskId, note: String },
    TaskFinished { task_id: TaskId, verdict: Verdict },
    UserEscalationNeeded { task_id: TaskId, reason: String, report: ValidationReport },
    RunFinished(FinalReport),
}
```

## State machine

```
       ┌───────────────┐
       │    Init       │───► load_or_create_state()
       └──────┬────────┘
              ▼
       ┌───────────────┐
       │   Planning    │───► planner::plan_l1
       └──────┬────────┘
              ▼
       ┌───────────────┐     no more L1          ┌───────────────┐
  ┌───►│  SelectL1     │──────────────────────►  │  Finalizing   │
  │    └──────┬────────┘                         └───────┬───────┘
  │           │ next L1                                  │
  │           ▼                                          ▼
  │    ┌───────────────┐                          ┌───────────────┐
  │    │  ExpandL2     │───► planner::plan_l2     │     Done      │
  │    └──────┬────────┘                          └───────────────┘
  │           ▼
  │    ┌───────────────┐     no more L2
  │    │  SelectL2     │──────────► (back to SelectL1)
  │    └──────┬────────┘
  │           │ next L2
  │           ▼
  │    ┌───────────────┐
  │    │ RunWorker     │───► worker::run_worker
  │    └──────┬────────┘
  │           ▼
  │    ┌───────────────┐
  │    │  Validate     │───► validator::validate (no LLM)
  │    └──────┬────────┘
  │           ▼
  │        verdict?
  │      ┌───┼────┬──────────────┐
  │     Ok  Uncertain  Failed    │
  │      │     │       │         │
  │      │     │       ▼         │
  │      │     │   ┌───────────┐ │
  │      │     │   │ Diagnose  │ │
  │      │     │   └─────┬─────┘ │
  │      │     │         ▼       │
  │      │     │     apply_strategy
  │      │     │    ┌────┬──┬──┐
  │      │     │  Retry Replan Skip Escalate
  │      │     │    │    │    │   │
  │      │     │    ▼    ▼    │   ▼
  │      │     │ RunWorker ExpandL2 SelectL2  user_gate
  │      │     │                               │
  │      │     ▼                               ▼
  │      │  mark NeedsReview             Interactive: pause
  │      ▼                                Autonomous:  Defer
  └───► record report, SelectL2
```

## Phase handlers

Each handler is a pure function on `&mut OrchestratorState + &mut Store`.
All phase transitions call `store.persist_state(&state)` before returning.

```rust
// skeleton
async fn step(
    state: &mut OrchestratorState,
    store: &mut Store,
    client: &OllamaClient,
    event_tx: &mpsc::UnboundedSender<DriverEvent>,
    ctx_size: u32,
) -> Result<StepOutcome> {
    let next = match &state.phase {
        OrchestratorPhase::Planning             => handle_planning(...).await?,
        OrchestratorPhase::ExpandingL2 { .. }   => handle_expand_l2(...).await?,
        OrchestratorPhase::ExecutingTask { .. } => handle_execute(...).await?,
        OrchestratorPhase::Validating { .. }    => handle_validate(...)?,
        OrchestratorPhase::Diagnosing { .. }    => handle_diagnose(...).await?,
        OrchestratorPhase::Finalizing           => handle_finalize(...).await?,
        OrchestratorPhase::Done                 => return Ok(StepOutcome::Done),
    };
    state.phase = next;
    store.persist_state(state)?;
    Ok(StepOutcome::Continue)
}

pub enum StepOutcome {
    Continue,
    Done,
    AwaitingUser { task_id: TaskId, reason: String },
}
```

The main loop in `run_orchestra` is:

```rust
loop {
    match step(&mut state, &mut store, client, &event_tx, ctx_size).await? {
        StepOutcome::Continue => {}
        StepOutcome::Done => break,
        StepOutcome::AwaitingUser { task_id, reason } => {
            // The TUI receives UserEscalationNeeded via event_tx.
            // run_orchestra suspends here; the TUI calls a separate
            // `resume_orchestra(decision)` to continue.
            return Ok(FinalReport::partial(state, "awaiting user"));
        }
    }
}
```

## Resume semantics

The driver can be resumed after a crash or a user-gated pause:

```rust
pub async fn resume_orchestra(
    client: &OllamaClient,
    run_id: &str,
    decision: Option<UserDecision>,
    ...
) -> Result<FinalReport>;
```

`UserDecision` is consumed by the `Diagnosing` phase to inject the user's
answer as a `hint` and retry the worker.

```rust
pub enum UserDecision {
    Retry { hint: Option<String> },
    Skip,
    Abort,
}
```

## Context-size guard

Before every LLM call, the driver:

1. Builds the projected prompt from state.
2. Measures its character length.
3. If > role budget (table in [02-data-model.md](02-data-model.md)), compacts the state:
   - Report summaries older than 10 entries are merged into a single
     "earlier: N tasks completed" line.
   - `ArtifactIndex` is pruned to 60 most-recent entries.
4. Rebuilds and re-measures. If still over, aborts with
   `Error::ContextOverflow` — should never happen with reasonable task
   sizes, but is a hard stop.

## Single-flight LLM

Only one LLM call is in flight at a time. Enforced by a simple
`tokio::sync::Mutex` on `OllamaClient` or by the sequential nature of the
state machine (the default). This avoids contention on the single loaded
model.

## Tool-call counting

The driver wraps `tools::dispatch` in an instrumented version that:

1. Increments the worker's `worker_tool_calls` counter.
2. Writes an `Event { kind: ToolCall { name }, ... }` to the raw log.
3. Writes the result with its byte size (but not full content) to the log.
4. Checks `max_tool_calls`; returns error if exceeded.

The worker itself does not see any difference; dispatch signature is
preserved.

## Deferred task handling (Autonomous)

When a task is deferred:

```rust
state.deferred_l1.push(l1_id);          // or stored per-L2 in a sub-list
state.cursor = next_eligible_l1(&state); // toposort skipping deferred deps
```

After all non-deferred tasks finish, the driver enters a "rescue pass":

```rust
fn rescue_pass(state: &mut OrchestratorState) {
    for task_id in state.deferred_l1.clone() {
        // one extra attempt each, with diagnoser's best hint
        try_rescue(task_id, state);
    }
}
```

Tasks that still fail after the rescue pass are reported as
`NeedsReview` in the `FinalReport`, never as a hard abort.

## FinalReport

```rust
pub struct FinalReport {
    pub run_id: String,
    pub goal: String,
    pub status: RunStatus,
    pub completed: Vec<TaskId>,
    pub deferred: Vec<TaskId>,
    pub failed: Vec<TaskId>,
    pub artifacts: ArtifactIndex,
    pub duration_s: u64,
    pub tokens_total: Option<u64>,
    pub human_summary: String,     // built by `render_final_summary()`
}

pub enum RunStatus {
    Completed,
    PartiallyCompleted,
    AwaitingUser,
    Aborted { reason: String },
}
```

`render_final_summary` is deterministic string building — no LLM call for
the final report. The user gets a clean audit without extra tokens spent.
