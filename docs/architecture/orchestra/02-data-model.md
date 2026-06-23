---
layout: default
title: 02 — Data Model
parent: Orchestra Mode
nav_order: 2
---

# 02 — Data Model

All types live in `src/orchestra/types.rs`. They are `serde` serializable and
form the contract between roles.

> **Wire vs persistence.** Types on disk (`state.json`, `plan.json`,
> `artifacts/index.json`, reports, validation results) use JSON for
> stability and tooling. Types that cross an LLM boundary
> (`TaskSpec`, `TaskReport`, `Diagnosis`, the worker `FinalReport`) are
> encoded in **ALF** — see [00-format.md](00-format.md). Every type has
> both `serde` derives and `FromAlf`/`ToAlf` impls.

## `OrchestratorState`

Top-level state. Persisted to `state.json` on every transition.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorState {
    pub run_id: String,                    // UNIX timestamp, also the dir name
    pub goal: String,                      // user's original request, verbatim
    pub created_at: u64,
    pub updated_at: u64,
    pub mode: FailurePolicy,
    pub plan: Vec<TaskId>,                 // ordered L1 task ids
    pub cursor: Option<TaskId>,            // currently executing L1
    pub phase: OrchestratorPhase,
    pub completed_l1: Vec<TaskId>,
    pub failed_l1: Vec<TaskId>,
    pub deferred_l1: Vec<TaskId>,          // autonomous-mode skips
    pub artifacts_index: PathBuf,          // -> artifacts/index.json
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrchestratorPhase {
    Planning,        // generating L1
    ExpandingL2 { l1: TaskId },
    ExecutingTask { l1: TaskId, l2: TaskId },
    Validating   { l1: TaskId, l2: TaskId },
    Diagnosing   { l1: TaskId, l2: TaskId, attempt: u32 },
    Finalizing,
    Done,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailurePolicy {
    Interactive,     // escalate to user on failure
    Autonomous,      // defer and retry at end
}

pub type TaskId = String;   // e.g. "T01", "T01.03"
```

## `TaskSpec`

Structured description of a task. Produced by the planner, consumed by the
worker and validator.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: TaskId,
    pub parent: Option<TaskId>,            // None for L1
    pub title: String,                     // ≤ 80 chars
    pub description: String,               // ≤ 400 chars
    pub deps: Vec<TaskId>,                 // must be Done before this runs
    pub expected_files: Vec<ExpectedFile>,
    pub expected_keywords: Vec<String>,    // literal keywords from goal
    pub extra_commands: Vec<String>,       // e.g. "cargo check"
    pub skip_checks: Vec<String>,          // names of Check to skip
    pub allowed_tools: Vec<String>,        // empty = all
    pub max_rounds: u32,                   // worker LLM round cap, default 4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedFile {
    pub path: PathBuf,
    pub change: FileChange,
    pub min_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileChange {
    Create,
    Modify,
    Delete,
}
```

## `TaskReport`

Compact result. Always ≤ 300 chars in `summary`. This is what the driver
keeps; the worker's full conversation is dropped.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReport {
    pub task_id: TaskId,
    pub attempt: u32,
    pub status: TaskStatus,
    pub summary: String,                   // ≤ 300 chars, worker-written
    pub files_touched: Vec<PathBuf>,
    pub started_at: u64,
    pub finished_at: u64,
    pub worker_tool_calls: u32,
    pub tokens_used: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Ok,
    Failed,
    Skipped,
    NeedsReview,     // validator returned Uncertain
}
```

## `Check` — acceptance criterion

Enum discriminated by a stable `kind` tag. Validator dispatches on this.
Details in [05-validator.md](05-validator.md).

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Check {
    FileExists            { path: PathBuf },
    FileSizeInRange       { path: PathBuf, min: u64, max: u64 },
    DiffHasChanges        { path: PathBuf },
    SyntaxValid           { path: PathBuf },
    NoPlaceholders        { path: PathBuf, whitelist: Vec<String> },
    NoLoopRepetition      { path: PathBuf, max_ratio: f32 },
    KeywordsPresent       { path: PathBuf, keywords: Vec<String>, min_hit: f32 },
    LanguageMatches       { path: PathBuf, lang: Language },
    NoEmptyCriticalBlocks { path: PathBuf },
    ReferencesResolve     { path: PathBuf },
    CommandExitsZero      { cmd: String, cwd: Option<PathBuf> },
    ManualReview          { note: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Language { En, Es, Unknown }
```

## `CheckOutcome` and `ValidationReport`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckOutcome {
    Pass,
    Fail { reason: String },
    Soft(f32),                             // 0.0 = bad, 1.0 = good
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub task_id: TaskId,
    pub outcomes: Vec<(String, CheckOutcome)>,  // (check_name, outcome)
    pub verdict: Verdict,
    pub score: f32,                             // weighted average
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Failed,
    Uncertain,
}
```

Aggregation rule (enforced in `ValidationReport::aggregate`):

1. Any `Fail` → `Verdict::Failed`.
2. No `Fail`, all remaining are `Pass` → `Verdict::Ok`.
3. Otherwise compute `score` = mean of `Soft` values (Pass counts as 1.0).
   - `score >= 0.7` → `Ok`
   - `0.5 <= score < 0.7` → `Uncertain`
   - `score < 0.5` → `Failed`

## `Diagnosis`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnosis {
    pub root_cause: String,                // ≤ 200 chars
    pub strategy: RetryStrategy,
    pub hint: Option<String>,              // ≤ 300 chars, sent to worker on retry
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RetryStrategy {
    RetryAsIs,
    RetryWithHint,
    ReplanSubtree,       // regenerate L2 for this L1
    Skip,                // give up on this task, continue run
    EscalateToUser,
}
```

## `ArtifactIndex`

Flat map of `path -> short description`. Written incrementally as files are
created or modified. Enables cross-task lookups without re-reading contents.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactIndex {
    pub entries: BTreeMap<PathBuf, ArtifactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub created_by: TaskId,
    pub description: String,               // ≤ 120 chars
    pub size_bytes: u64,
    pub sha256: String,                    // for change detection
}
```

## `Event`

Append-only log entry (raw layer, never loaded into prompts).

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts: u64,
    pub task_id: Option<TaskId>,
    pub kind: EventKind,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventKind {
    PlannerCalled { role: PlannerRole },
    PlannerResult,
    WorkerStarted,
    ToolCall { name: String },
    ToolResult { name: String, bytes: usize },
    WorkerFinished,
    ValidationStarted,
    ValidationFinished { verdict: Verdict },
    DiagnoserCalled,
    DiagnoserResult,
    RetryApplied { strategy: RetryStrategy },
    UserEscalation,
    PhaseChanged { from: String, to: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlannerRole { L1, L2 }
```

## Size budget (runtime, ALF-encoded)

When projected into a prompt, the total must fit these caps. Checked by the
driver before every LLM call; if exceeded, the driver compacts reports
before proceeding. Limits are in ALF-encoded characters (roughly half the
equivalent JSON char count for the same payload).

| Role | Budget (chars, ALF) |
|------|----------------|
| Planner L1 input | 1,200 |
| Planner L2 input | 1,800 |
| Worker input | 2,500 |
| Validator input | N/A (deterministic) |
| Diagnoser input | 1,500 |
