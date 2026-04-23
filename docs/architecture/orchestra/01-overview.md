---
layout: default
title: 01 — Overview
parent: Orchestra Mode
nav_order: 1
---

# 01 — Overview

## Motivation

Small local models (≤ 14B) routinely succeed at individual coding tasks but
degrade in long conversations: they forget the goal, loop on tool calls, or
hallucinate file contents after ~15 K tokens of history. Orchestra works
around this by **never letting a single LLM conversation grow**.

## Mental model

```
┌────────────────────────────────────────────────────────────────────┐
│                       ORCHESTRA DRIVER (Rust)                       │
│                                                                    │
│  OrchestratorState   ◄──────────────────────────┐                  │
│  (lives in memory +                             │                  │
│   persisted to disk)                            │                  │
│         │                                       │                  │
│         │ projects minimal prompt               │ writes report    │
│         ▼                                       │                  │
│   ┌──────────┐      one-shot LLM call      ┌────┴────┐             │
│   │  Role    │──────────────────────────► │  LLM    │             │
│   │ Planner  │◄────────────────────────── │ (Ollama)│             │
│   │ Worker   │         (< 8 K tokens)     └─────────┘             │
│   │ Validator│                                                    │
│   │ Diagnoser│  history dropped after call                        │
│   └──────────┘                                                    │
└────────────────────────────────────────────────────────────────────┘
```

The **driver** is the brain. The LLM is a pure function called many times
with small, role-specific prompts. State lives in Rust, not in the model.

## High-level flow

```
user_goal
    │
    ▼
[Planner L1]  ──► Plan { tasks: [T1, T2, T3, ...] }
    │
    ▼
for each Ti in plan:
    │
    ├─► [Planner L2]  ──► subtasks [Ti.1, Ti.2, ...]   (lazy, per L1)
    │
    ├─► for each subtask:
    │     │
    │     ├─► [Worker]    ──► TaskReport + artifacts
    │     │
    │     ├─► [Validator] (deterministic; NO LLM in the common path)
    │     │
    │     ├─► if Fail:
    │     │     ├─► [Diagnoser] ──► retry_strategy
    │     │     └─► apply strategy (retry / replan / skip / escalate)
    │     │
    │     └─► append TaskReport to store; drop worker history
    │
    └─► on L1 completion: compact all subtask reports into one L1 summary
```

## Design principles

1. **Rust owns the loop.** The LLM is invoked, not in charge.
2. **Minimal context.** Each role's prompt is reconstructed from structured
   state, never carried forward as conversation.
3. **Determinism first.** Validation is code, not an LLM judge, except where
   truly impossible.
4. **Lazy expansion.** L2 subtasks are generated just before execution, so
   they benefit from fresh artifact knowledge.
5. **Same model, many roles.** Avoids model-swap cost; keeps weights hot in
   memory.
6. **Resumable.** The store on disk is the source of truth; the in-memory
   state is a cache.
7. **Fail loud, skip smart.** Failures are structured events, not exceptions.
   Autonomous mode defers, interactive mode escalates.

## When to use Orchestra vs Agent

| Situation | Mode |
|-----------|------|
| Quick question, one file | Agent |
| "Fix this bug" | Agent |
| Multi-step feature spanning many files | **Orchestra** |
| "Build a website for X" (open-ended) | **Orchestra** |
| Refactoring many modules | **Orchestra** |
| User wants streamed conversation | Agent / Chat |

Orchestra adds latency per task (planner + validator overhead) but prevents
the "model got lost at round 25" failure.

## Non-goals (repeat for emphasis)

- Not a replacement for Agent mode.
- Not cross-model orchestration.
- Not concurrent worker execution.
