---
layout: default
title: Orchestra Mode
parent: Architecture
has_children: true
---

# Orchestra Mode — Design Specification

Orchestra is a session mode for Allux that drives a single local model through
many short, context-isolated LLM calls instead of one growing conversation.
It exists because small local models fail on sustained context, not on task
difficulty.

## Reading order

Each file is self-contained. An implementing agent only needs to load the
files relevant to the module being built.

| # | File | Load when implementing… |
|---|------|-------------------------|
| 1 | [01-overview.md](01-overview.md) | anything (always read first) |
| 2 | [02-data-model.md](02-data-model.md) | anything (types are shared) |
| 3 | [03-planner.md](03-planner.md) | planner role |
| 4 | [04-worker.md](04-worker.md) | worker role |
| 5 | [05-validator.md](05-validator.md) | validator / checks |
| 6 | [06-diagnoser.md](06-diagnoser.md) | diagnoser role |
| 7 | [07-driver.md](07-driver.md) | main loop / state machine |
| 8 | [08-store.md](08-store.md) | persistence / sandbox |
| 9 | [09-integration.md](09-integration.md) | wiring into existing code |
| 10 | [10-implementation-plan.md](10-implementation-plan.md) | phased rollout |

## Glossary

| Term | Definition |
|------|------------|
| **Driver** | Rust code that owns the loop; calls LLM only when reasoning is needed. |
| **Role** | A named LLM invocation pattern: Planner, Worker, Validator, Diagnoser. |
| **Micro-session** | One short LLM call with its own `Vec<Message>`; discarded after. |
| **Sandbox / Store** | On-disk two-layer state: raw log + distilled structured state. |
| **TaskSpec** | Structured description of a task, including acceptance checks. |
| **TaskReport** | Compact result of executing a task (≤ 300 chars summary). |
| **Artifact** | Any file produced or modified by a task. |
| **L1 task** | High-level task from the initial plan. |
| **L2 task** | Subtask of an L1 task, generated lazily. |
| **Check** | A deterministic validation routine with no LLM involvement. |

## Core invariants

These must hold for every implementation choice:

1. No single LLM call sees more than the state projected for its role.
2. The driver can resume from disk after a crash without replaying LLM calls.
3. All state that crosses role boundaries is `serde`-serializable.
4. Validation of a task never depends on the worker's conversation history.
5. The raw log is never loaded into any LLM prompt.

## Non-goals

- Multi-model orchestration (one model is loaded at a time).
- Real parallelism across workers (one model, one GPU).
- Supporting models that do not expose tool calling.
