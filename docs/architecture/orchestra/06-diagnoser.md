---
layout: default
title: 06 — Diagnoser Role
parent: Orchestra Mode
nav_order: 6
---

# 06 — Diagnoser Role

The diagnoser is invoked **only on failure**. It receives a minimal context
and returns a structured `Diagnosis`. Its job is to decide a retry strategy,
not to fix the code.

## Invocation contract

```rust
// src/orchestra/diagnoser.rs
pub async fn diagnose(
    client: &OllamaClient,
    spec: &TaskSpec,
    report: &TaskReport,
    validation: &ValidationReport,
    last_tool_event: Option<&Event>,
    ctx_size: u32,
) -> Result<Diagnosis>;
```

The diagnoser is a **stateless LLM call**. It does not see the worker's
conversation history. It does not have tools.

## System prompt

```
You are a failure diagnoser for a software agent. You are given ONE
failed task and its validation results. Your job is to decide what to do
next.

HARD RULES:
- Reply in ALF with a single Diagnosis record.
- Do NOT attempt to solve the task. Only diagnose.
- Do NOT call tools.
- hint (if present) MUST be concrete and under 300 chars.
- Do NOT wrap in code fences. Do NOT add prose.

FORMAT:
- Each line is `<key> <value>`. Terminate with `.` on its own line.
- Use `-` for empty / none.

Diagnosis fields:
  root_cause — ≤ 200 chars
  strategy   — one of: RetryAsIs | RetryWithHint | ReplanSubtree | Skip | EscalateToUser
  hint       — ≤ 300 chars, or `-` (required present when strategy is RetryWithHint)

Strategy guidance:
- RetryAsIs      transient failure (network, timeout); same spec should work.
- RetryWithHint  worker misunderstood one concrete thing; `hint` must state it.
- ReplanSubtree  the L2 plan itself was wrong; parent L1 needs re-expansion.
- Skip           task is not critical and blocking the run is worse than missing it.
- EscalateToUser the failure requires information the agent does not have.

EXAMPLE:
root_cause Worker created styles.css but did not create index.html
strategy RetryWithHint
hint Start by creating src/index.html; styles.css already exists
.
```

## User prompt

```
Task spec:
<spec in ALF>

Worker report:
<report in ALF>

Validation outcomes (failures first, ≤ 5):
- <name>: <Pass|Fail reason|Soft score>
- ...

Last tool event (if any):
  tool   <name>
  output <≤ 400 chars, truncated>
```

## Chat options

```rust
ChatOptions { temperature: Some(0.1), num_ctx: Some(ctx_size) }
```

## Output parsing

```rust
fn parse_diagnosis(raw: &str) -> Result<Diagnosis> {
    let cleaned = alf::strip_surrounding_prose(raw);
    let rec = alf::parse_one(&cleaned)?;
    let d = Diagnosis::from_alf(&rec)?;
    if d.root_cause.len() > 200 {
        return Err(anyhow!("root_cause too long"));
    }
    if let Some(h) = &d.hint {
        if h.len() > 300 { return Err(anyhow!("hint too long")); }
    }
    if d.strategy == RetryStrategy::RetryWithHint && d.hint.is_none() {
        return Err(anyhow!("RetryWithHint requires a non-empty hint"));
    }
    Ok(d)
}
```

On two parse failures the driver defaults to:
```rust
Diagnosis {
    root_cause: "diagnoser did not return valid ALF".into(),
    strategy: RetryStrategy::EscalateToUser,
    hint: None,
}
```

## Retry budget

The driver applies a per-task retry cap before escalating regardless of
the diagnoser's strategy:

```rust
const MAX_ATTEMPTS: u32 = 3;

fn apply_strategy(attempts: u32, d: &Diagnosis) -> EffectiveAction {
    if attempts >= MAX_ATTEMPTS {
        return EffectiveAction::Escalate; // or Defer in autonomous mode
    }
    match d.strategy {
        RetryStrategy::RetryAsIs       => EffectiveAction::Retry { hint: None },
        RetryStrategy::RetryWithHint   => EffectiveAction::Retry { hint: d.hint.clone() },
        RetryStrategy::ReplanSubtree   => EffectiveAction::ReplanL2,
        RetryStrategy::Skip            => EffectiveAction::Skip,
        RetryStrategy::EscalateToUser  => EffectiveAction::Escalate,
    }
}
```

`EffectiveAction::Escalate` is replaced by `Defer` in `FailurePolicy::Autonomous`.

## Purely-deterministic short-circuits

Before calling the diagnoser, the driver applies these classical rules and
**skips the LLM call** if any match:

| Trigger | Action |
|---------|--------|
| Validation failed only on `CommandExitsZero` with exit 124 (timeout) | RetryAsIs once |
| Validation failed only on `FileExists` for a single file and worker reported status=Ok | RetryWithHint: "`<path>` was not created; create it." |
| Validation score ≥ 0.6 and only Soft signals | Downgrade to `NeedsReview`, do not retry |
| Worker reported status=`NeedsReview` directly | Do not retry; mark `NeedsReview` |
| Same validation error on two consecutive attempts | ReplanSubtree |

These rules save one LLM roundtrip in the common cases.

## What the diagnoser must NOT do

- Suggest or produce code.
- Reference files that are not in the spec or validation report.
- Return more than one strategy.

The parser rejects any response that embeds code blocks or prose before/after
the JSON object.
