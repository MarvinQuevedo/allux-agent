---
layout: default
title: 04 — Worker Role
parent: Orchestra Mode
nav_order: 4
---

# 04 — Worker Role

The worker executes exactly one `TaskSpec` (always an L2 / leaf task) using
the existing tool dispatcher. It runs a short agentic loop with a small
context and returns a `TaskReport`.

## Invocation contract

```rust
// src/orchestra/worker.rs
pub async fn run_worker(
    client: &OllamaClient,
    spec: &TaskSpec,
    goal: &str,
    artifacts: &ArtifactIndex,
    ctx_size: u32,
    quiet: bool,
) -> Result<TaskReport>;
```

No conversation history is passed in. Every call is a fresh micro-session.

## Agentic loop

Reuses the existing Ollama chat + tool dispatch, but with its own bounded
history.

```
round = 0
history = [system_prompt(spec, goal, artifacts), user_prompt(spec)]
while round < spec.max_rounds:
    response = client.chat(history, tools=allowed(spec), options)
    match response:
        Text(content)      -> return parse_final_report(content, spec)
        ToolCalls(calls)   -> execute each, append results, round += 1
return TaskReport { status: Failed, summary: "max rounds reached", ... }
```

## System prompt

```
You are a worker agent. You execute ONE concrete task, then stop.

You MUST:
- Stay focused on the task below. Do not work on related tasks.
- Use tools to read, search, edit, and run commands as needed.
- When finished, reply in ALF with a single FinalReport record.
- Keep your summary under 300 characters.

You MUST NOT:
- Invent files that were not created or modified by your actions.
- Continue working after you emit the FinalReport.
- Wrap the FinalReport in code fences.
- Add prose before or after the record.

FORMAT:
- Each line is `<key> <value>`. Arrays are comma-separated.
- Terminate the record with a line containing only `.`.
- Use `-` for empty / none.

FinalReport fields:
  status         — `ok` | `failed` | `needs_review`
  summary        — ≤ 300 chars; single line (or use `summary:` ... `:end` block)
  files_touched  — comma-separated relative paths, or `-`

EXAMPLE:
status ok
summary Created src/index.html with hero, services grid, and contact form
files_touched src/index.html, src/styles.css
.

Original user goal (context only, do NOT pursue it directly):
<goal, ≤ 500 chars>

artifacts:
<compact index body; do NOT recreate listed files>
:end
```

## User prompt

```
Task to execute:
<spec in ALF>

Hint from diagnoser (only present on retry):
<hint, or omitted>

Reply by calling tools to execute the task, then emit the FinalReport.
```

## Allowed tools

Reuses `src/tools/`. Filtered by `spec.allowed_tools`:

- Empty list → all tools available (default).
- Non-empty → only listed tools are exposed to the LLM.

Typical restrictions by task type:

| Task type | Typical allowed_tools |
|-----------|----------------------|
| Explore  | `read_file`, `glob`, `grep`, `tree` |
| Write    | `read_file`, `write_file`, `edit_file`, `glob` |
| Verify   | `bash`, `read_file` |
| Mixed    | all (empty list) |

The planner decides and writes this into the spec.

## Chat options

```rust
ChatOptions {
    temperature: Some(0.3),   // slight creativity for code
    num_ctx: Some(ctx_size),
}
```

## Final report parsing

```rust
fn parse_final_report(raw: &str, spec: &TaskSpec) -> Result<TaskReport> {
    let cleaned = alf::strip_surrounding_prose(raw);
    let rec = alf::parse_one(&cleaned)?;
    let wire = FinalReportWire::from_alf(&rec)?;
    Ok(TaskReport {
        task_id: spec.id.clone(),
        attempt: 0, // filled in by driver
        status: wire.status,
        summary: truncate(&wire.summary, 300),
        files_touched: wire.files_touched,
        started_at: 0,
        finished_at: 0,
        worker_tool_calls: 0,
        tokens_used: None,
    })
}
```

If the worker emits prose instead of ALF, the driver:
1. Calls `alf::strip_surrounding_prose`, which greedily locates the first
   recognized key (`status`) and drops anything before it, then trims
   everything after the first terminator `.`.
2. If no recognized record can be found, synthesizes a report:
   `status: NeedsReview`, `summary: "worker did not return structured
   report"`, and records the raw output in the event log for inspection.

## Tool call quota

A worker has two independent caps:

| Cap | Default | Purpose |
|-----|---------|---------|
| `max_rounds` | 4 | LLM round-trips within this micro-session |
| `max_tool_calls` | 12 | total tool invocations across rounds |

If `max_tool_calls` is exceeded, the driver aborts the worker mid-round,
writes a `Failed` report with summary `"tool call quota exceeded"`, and
proceeds to diagnosis.

## File-change tracking

Before the worker starts, the driver snapshots a list of candidate paths
(from `spec.expected_files` + their parent dirs). After the worker finishes,
the driver:

1. Compares mtimes and hashes against the snapshot.
2. Writes the actual diffs to `tasks/<task_id>/attempts/<n>/diff.patch`
   (via `git diff` if the workspace is a repo, else manual diff).
3. Updates `ArtifactIndex` with created/modified files.

The worker's self-reported `files_touched` is **not** trusted by the
validator — it is only used as a hint.

## Quiet mode

The worker always runs with `quiet: true` to suppress terminal output from
tools. The driver streams its own progress updates through the TUI.
