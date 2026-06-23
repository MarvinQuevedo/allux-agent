---
layout: default
title: 08 — Store & Persistence
parent: Orchestra Mode
nav_order: 8
---

# 08 — Store & Persistence

Two-layer on-disk state. The raw layer is write-only from the driver's
perspective (read only by humans or post-mortem tools). The distilled layer
is the single source of truth for resuming a run.

## Directory layout

```
<workspace>/.allux/
└── runs/
    └── <run_id>/                 # run_id = UNIX timestamp (seconds)
        ├── state.json            # OrchestratorState (distilled)
        ├── plan.json             # Vec<TaskSpec>   — L1 plan with statuses
        ├── tasks/
        │   └── T01/              # one dir per L1
        │       ├── spec.json     # L1 TaskSpec
        │       ├── subtasks.json # Vec<TaskSpec>   — L2, empty until expanded
        │       └── T01.02/       # one dir per L2
        │           ├── spec.json
        │           ├── attempts/
        │           │   └── 01/
        │           │       ├── report.json     # TaskReport
        │           │       ├── validation.json # ValidationReport
        │           │       ├── diff.patch      # artifact diff (if any)
        │           │       └── diagnosis.json  # optional, only on failure
        │           └── latest.json  # pointer: { "attempt": 3, "verdict": "ok" }
        ├── artifacts/
        │   └── index.json        # ArtifactIndex
        └── events.log            # append-only JSONL, the raw layer
```

`run_id` is also embedded in `state.run_id`. Listing past runs is a `readdir`.

## Distilled layer — `state.json` and friends

All `*.json` files are pretty-printed (`serde_json::to_string_pretty`) for
easier post-mortem inspection. They are the only files that are ever
projected into LLM prompts.

### Write policy

| File | Written when |
|------|--------------|
| `state.json` | every phase transition |
| `plan.json` | after L1 planning; after an L1 task's status changes |
| `tasks/<id>/spec.json` | once, at task creation |
| `tasks/<id>/subtasks.json` | after L2 expansion; when an L2 status changes |
| `attempts/NN/report.json` | immediately after worker finishes |
| `attempts/NN/validation.json` | immediately after `validator::validate` returns |
| `attempts/NN/diagnosis.json` | immediately after diagnoser returns |
| `artifacts/index.json` | after each task completes |
| `latest.json` | after each attempt completes |

Writes are atomic: write to `<file>.tmp`, then `fs::rename`.

## Raw layer — `events.log`

JSONL, one `Event` per line. Never read for LLM input. Used for:

- Debugging (`allux debug <run_id>`).
- Post-hoc tool-call replay.
- Performance analysis (token counts, round times).

### Rotation

When a run finishes, `events.log` is gzipped to `events.log.gz` and the
uncompressed file is removed. No rotation during a run — append-only.

### Size cap

A soft cap of 50 MB per run. If exceeded, the driver emits a warning and
truncates `ToolResult` payloads before writing (the tool outputs are
already in-memory only; the log stores byte sizes, not contents).

## `Store` API

`src/orchestra/store.rs`

```rust
pub struct Store {
    root: PathBuf,              // <workspace>/.allux/runs/<run_id>/
}

impl Store {
    pub fn create(workspace: &Path, goal: &str) -> Result<Self>;
    pub fn open(workspace: &Path, run_id: &str) -> Result<Self>;

    // distilled layer
    pub fn persist_state(&self, s: &OrchestratorState) -> Result<()>;
    pub fn load_state(&self) -> Result<OrchestratorState>;
    pub fn write_plan(&self, plan: &[TaskSpec]) -> Result<()>;
    pub fn load_plan(&self) -> Result<Vec<TaskSpec>>;
    pub fn write_task_spec(&self, spec: &TaskSpec) -> Result<()>;
    pub fn load_task_spec(&self, id: &TaskId) -> Result<TaskSpec>;
    pub fn write_subtasks(&self, parent: &TaskId, subs: &[TaskSpec]) -> Result<()>;
    pub fn load_subtasks(&self, parent: &TaskId) -> Result<Vec<TaskSpec>>;
    pub fn write_report(&self, id: &TaskId, attempt: u32, r: &TaskReport) -> Result<()>;
    pub fn write_validation(&self, id: &TaskId, attempt: u32, v: &ValidationReport) -> Result<()>;
    pub fn write_diagnosis(&self, id: &TaskId, attempt: u32, d: &Diagnosis) -> Result<()>;
    pub fn write_diff(&self, id: &TaskId, attempt: u32, diff: &str) -> Result<()>;
    pub fn write_latest(&self, id: &TaskId, attempt: u32, verdict: Verdict) -> Result<()>;
    pub fn update_artifacts(&self, idx: &ArtifactIndex) -> Result<()>;
    pub fn load_artifacts(&self) -> Result<ArtifactIndex>;

    // raw layer
    pub fn append_event(&self, ev: &Event) -> Result<()>;
    pub fn finalize(&self) -> Result<()>;    // gzip events.log, clean tmp
}
```

## Resume flow

```rust
fn resume(workspace: &Path, run_id: &str) -> Result<(OrchestratorState, Store)> {
    let store = Store::open(workspace, run_id)?;
    let state = store.load_state()?;
    // The on-disk state reflects the last completed phase transition.
    // The next call to `step` re-enters that phase and continues.
    Ok((state, store))
}
```

Because state is persisted at every transition, at most one micro-session
is lost on a crash. That call is re-executed on resume.

## Listing runs (for `/orchestra list`)

```rust
pub fn list_runs(workspace: &Path) -> Result<Vec<RunSummary>>;

pub struct RunSummary {
    pub run_id: String,
    pub goal: String,           // first 80 chars
    pub created_at: u64,
    pub updated_at: u64,
    pub status: RunStatus,
    pub completed: usize,
    pub failed: usize,
    pub deferred: usize,
}
```

Listing reads only `state.json` per run dir — no plan/tasks loaded — so a
hundred runs fit well under 10 MB of I/O.

## Data retention

Default retention: keep the last 20 completed runs. Older runs are moved
to `<workspace>/.allux/runs/.archive/` on `allux gc` invocation. Nothing
is deleted automatically.

## Git ignore

The implementation plan adds `.allux/` to `.gitignore` on first run
(append-only; if `.gitignore` is absent, create it).

## Cross-platform notes

- Paths are stored relative to the workspace where possible, absolute only
  when referring to outside files.
- `fs::rename` is atomic on POSIX and on NTFS within the same volume.
- File locking is not required: only the driver writes, and at most one
  driver exists per workspace (enforced by a lock file `.allux/runs/.lock`
  acquired with `fs2::FileExt::try_lock_exclusive`).
