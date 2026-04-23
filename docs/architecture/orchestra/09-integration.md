---
layout: default
title: 09 — Integration
parent: Orchestra Mode
nav_order: 9
---

# 09 — Integration with Existing Code

Orchestra is an additive mode. Nothing in the existing Chat / Agent / Plan
paths changes. This document lists every touch point.

## New module tree

```
src/
├── orchestra/
│   ├── mod.rs              # pub use re-exports
│   ├── types.rs            # all shared types (02-data-model.md)
│   ├── alf.rs              # ALF parser + writer (00-format.md)
│   ├── keywords.rs         # deterministic keyword extraction
│   ├── planner.rs          # plan_l1, plan_l2 (03-planner.md)
│   ├── worker.rs           # run_worker (04-worker.md)
│   ├── diagnoser.rs        # diagnose (06-diagnoser.md)
│   ├── driver.rs           # state machine + entry points (07-driver.md)
│   ├── store.rs            # on-disk persistence (08-store.md)
│   └── validator/
│       ├── mod.rs          # aggregate + public API
│       ├── auto.rs         # workspace-signal auto-detection
│       └── checks/
│           ├── structural.rs
│           ├── syntax.rs
│           ├── content.rs
│           ├── cross_file.rs
│           ├── execution.rs
│           └── manual.rs
```

`src/orchestra/mod.rs` is the single re-export surface consumed by the TUI:

```rust
pub mod types;
pub mod alf;

mod driver;
mod planner;
mod worker;
mod diagnoser;
mod store;
mod validator;
mod keywords;

pub use driver::{run_orchestra, resume_orchestra, DriverEvent, FinalReport, UserDecision};
pub use store::{list_runs, RunSummary};
pub use types::{OrchestratorState, FailurePolicy, TaskId};
```

## `SessionMode` extension

Add `Orchestra` to both enums:

- `src/repl/mod.rs:59`
- `src/tui/app.rs:42`

```rust
pub enum SessionMode {
    Chat,
    Agent,
    Plan,
    Orchestra,
}
```

`SessionMode::label()` returns `"orchestra"`. The TUI completion list in
`src/tui/widgets/input_area.rs:23` gets a new entry for `/mode orchestra`.

## Slash commands

Add to both `src/repl/mod.rs` and `src/tui/app.rs` command handlers:

| Command | Action |
|---------|--------|
| `/mode orchestra` | switch current session into Orchestra mode |
| `/orchestra <goal>` | one-shot run from the current message (ignores mode) |
| `/orchestra list` | list past runs (`store::list_runs`) |
| `/orchestra resume <run_id>` | resume a past run if its state is not Done |
| `/orchestra cancel` | mark current run Aborted |
| `/policy interactive` | set `FailurePolicy::Interactive` |
| `/policy autonomous` | set `FailurePolicy::Autonomous` |

## TUI flow changes

### App state additions (`src/tui/app.rs`)

```rust
pub struct App {
    // ... existing fields ...
    pub orchestra_run_id: Option<String>,
    pub orchestra_policy: FailurePolicy,
    pub orchestra_events_rx: Option<mpsc::UnboundedReceiver<DriverEvent>>,
    pub orchestra_handle: Option<tokio::task::JoinHandle<Result<FinalReport>>>,
    pub pending_escalation: Option<PendingEscalation>,
}

pub struct PendingEscalation {
    pub task_id: TaskId,
    pub reason: String,
    pub report: ValidationReport,
}
```

### Submission path

When the user hits Enter in Orchestra mode:

```rust
if app.mode == SessionMode::Orchestra && app.orchestra_handle.is_none() {
    let (tx, rx) = mpsc::unbounded_channel();
    let handle = tokio::spawn(orchestra::run_orchestra(
        app.client.clone(),
        app.workspace_root.clone(),
        input_text,
        app.orchestra_policy,
        app.config.context_size,
        tx,
    ));
    app.orchestra_events_rx = Some(rx);
    app.orchestra_handle = Some(handle);
}
```

### Event pump

In the main `tick` or `update` method, drain `orchestra_events_rx` and map
`DriverEvent` variants into `ChatMessage` for rendering:

| DriverEvent | Rendered as |
|-------------|-------------|
| `PhaseChanged(p)` | dim system line: `[phase] Executing T01.02` |
| `TaskStarted(id)` | bold system line with ▶ glyph |
| `TaskProgress { note }` | dim indented line |
| `TaskFinished { verdict }` | coloured ✓ / ⚠ / ✗ line |
| `UserEscalationNeeded { .. }` | modal with Retry / Skip / Abort buttons |
| `RunFinished(fr)` | final summary block + sets `orchestra_handle = None` |

### Escalation modal

Reuses the existing permission modal styling
(`src/tui/app.rs:PermissionPrompt`) but the action set is:

```
[R]etry  [S]kip  [A]bort   e[D]it hint
```

`edit hint` opens a short text input; the resulting `UserDecision::Retry
{ hint: Some(..) }` is sent back via `resume_orchestra`.

## Config additions

`src/config/mod.rs` gains:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // ... existing ...
    #[serde(default = "default_orchestra_policy")]
    pub orchestra_policy: String,        // "interactive" | "autonomous"
    #[serde(default = "default_orchestra_max_attempts")]
    pub orchestra_max_attempts: u32,     // per-task cap, default 3
    #[serde(default)]
    pub orchestra_worker_model: Option<String>, // reserved; None = use main model
}
```

Defaults: `"interactive"`, `3`, `None`. Written on first run by the setup
wizard if Orchestra mode is opt-in selected, otherwise left unset.

## Tool dispatch instrumentation

`src/tools/mod.rs:dispatch` stays unchanged. The driver wraps it through
`src/orchestra/worker.rs:instrumented_dispatch` which:

1. Increments the worker's `worker_tool_calls` counter.
2. Calls the real `tools::dispatch`.
3. Appends `Event { kind: ToolCall { .. } }` and `Event { kind: ToolResult
   { .. } }` to the store's raw log (byte size only, not contents).
4. Returns the same `Result<String>` the caller expected.

No tool code changes.

## Permissions

Orchestra reuses `PermissionStore`. When a worker proposes a `bash` or
`write_file` / `edit_file` call, the existing permission flow is reused
identically. In `FailurePolicy::Autonomous`, the driver configures the
permission store to auto-grant at `AllowWorkspace` scope **only for
commands already granted** — it never auto-approves new commands.

If a worker requests a permission that is not granted in Autonomous mode,
the task is reported as `Failed` with summary `"permission denied: <cmd>"`.
The diagnoser then decides `EscalateToUser` or `Skip`.

## Compression interplay

The existing `src/compression/` is unused inside Orchestra — each
micro-session starts fresh. Compression is still used for:

- Collapsing long user goals before they enter the planner prompt
  (`compression::compress_message` with `CompressionLevel::Standard`).
- Truncating tool-call outputs that are echoed into the diagnoser's last
  tool event field.

## Session persistence (existing sessions)

Orchestra has its own store at `.allux/runs/`. The legacy session files at
`~/.config/allux/sessions/` are untouched. Slash command `/session save`
in Orchestra mode saves the driver run id, not a message history.

## Doctor / telemetry

`src/doctor.rs` gains a new check:

```rust
pub fn orchestra_available(workspace: &Path) -> DoctorCheck {
    // report .allux/runs writability, disk free, lock status
}
```

Surfaced under `/doctor`.

## Keybinds

No new keybinds. Enter still submits; Esc cancels the current worker at
the next round boundary (a soft cancel). `Ctrl+C` during Orchestra aborts
the run and marks it `Aborted`.

## Testing surface

- Unit: `src/orchestra/alf.rs` round-trip, `validator/checks/*` each with
  fixture files in `tests/fixtures/orchestra/`.
- Integration: a small goal (“create hello.txt saying hi”) run end-to-end
  against a fake Ollama (`tests/orchestra_e2e.rs`), with a hand-rolled
  stub that returns canned ALF records.
- Manual: the validation suite at `validation/` gains an Orchestra
  scenarios folder with goals that exercise planner failure, worker
  loop, cross-file references, etc.
