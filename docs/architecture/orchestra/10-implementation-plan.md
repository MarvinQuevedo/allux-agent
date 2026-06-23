---
layout: default
title: 10 — Implementation Plan
parent: Orchestra Mode
nav_order: 10
---

# 10 — Implementation Plan

Phased rollout. Each phase is independently mergeable and leaves the
project in a working state. Subagents should own one phase at a time.

## Phase 1 — Foundations (no behaviour change)

**Scope.** Add the module tree, the ALF parser, the type definitions.
Orchestra mode is not yet callable.

**Deliverables:**

- `src/orchestra/` module stubbed (`mod.rs`, all submodules present but
  mostly empty).
- `src/orchestra/types.rs` — all structs / enums from
  [02-data-model.md](02-data-model.md) with `serde` derives.
- `src/orchestra/alf.rs` — parser + writer per
  [00-format.md](00-format.md). `FromAlf` / `ToAlf` traits with impls for
  `TaskSpec`, `TaskReport`, `Diagnosis`.
- Unit tests: ALF round-trip for 20+ hand-written records; malformed
  input recovery; size-cap enforcement.
- `SessionMode::Orchestra` added to both enums; slash commands registered
  but return `"not yet implemented"` messages.

**Acceptance:**

- `cargo build --release` succeeds.
- `cargo test --package allux-agent orchestra::` passes.
- No regressions in existing Chat / Agent / Plan tests.

**Load:** [00-format.md](00-format.md), [02-data-model.md](02-data-model.md),
[09-integration.md](09-integration.md).

---

## Phase 2 — Validator (fully deterministic, standalone)

**Scope.** The full check catalog, aggregation, human-rendered report.
Runnable via an internal CLI hook for testing, not yet wired to the
driver.

**Deliverables:**

- `src/orchestra/validator/checks/structural.rs` — 4 checks.
- `src/orchestra/validator/checks/syntax.rs` — per-extension parsers.
  `rs` via `syn`; `json`/`toml`/`yaml` via `serde`; `html`/`css` via
  in-house balancers; external tools (`node --check`, `python -m
  py_compile`, `bash -n`, `tsc --noEmit`) invoked conditionally.
- `src/orchestra/validator/checks/content.rs` — placeholders, zstd loop
  ratio, unique-line ratio, n-gram, entropy, keywords present, language
  detect, empty critical blocks.
- `src/orchestra/validator/checks/cross_file.rs` — references resolve,
  symbols defined.
- `src/orchestra/validator/checks/execution.rs` — `command_exits_zero`
  with timeout + log capture.
- `src/orchestra/validator/checks/manual.rs` — `ManualReview` check.
- `src/orchestra/validator/auto.rs` — workspace-signal detection.
- `src/orchestra/validator/mod.rs` — public `validate(&spec, ws, pre)`,
  `ValidationReport::aggregate`, `render_human`.
- `tests/fixtures/orchestra/validator/` — fixture files exercising each
  outcome (pass, fail, soft).
- Unit tests per check + aggregation test.
- New dependency: `zstd` (already indirectly via some transitive?
  otherwise add it).

**Acceptance:**

- `cargo test validator::` passes, including aggregation edge cases
  (all-pass, one-fail-kills-everything, soft-mix boundaries 0.49 / 0.5 /
  0.69 / 0.70).
- Performance: full deterministic battery on a 1,000-line file ≤ 500 ms
  on CI.

**Load:** [02-data-model.md](02-data-model.md), [05-validator.md](05-validator.md).

---

## Phase 3 — Store + raw log

**Scope.** On-disk persistence for state, plan, task reports, validation
reports, artifacts, and events.

**Deliverables:**

- `src/orchestra/store.rs` — full `Store` API from
  [08-store.md](08-store.md).
- Atomic write helper (`tmp + rename`).
- Lock file acquisition at `.allux/runs/.lock`.
- Events log rotation (gzip on `finalize()`).
- `list_runs` + `RunSummary`.
- `.gitignore` append on first run.
- Unit tests using `tempdir` for: create run, persist state, reload
  state, record attempts, list runs, resume.

**Acceptance:**

- `cargo test store::` passes.
- Killing the process mid-write leaves the store in a parseable state
  (old file intact via atomic rename).

**Load:** [02-data-model.md](02-data-model.md), [08-store.md](08-store.md).

---

## Phase 4 — Planner + Diagnoser roles

**Scope.** LLM-driven roles (no tools). Both fully testable with a fake
Ollama client.

**Deliverables:**

- `src/orchestra/keywords.rs` — deterministic keyword extraction
  (tokenize, stopwords EN+ES, light stemmer, dedup).
- `src/orchestra/planner.rs` — `plan_l1`, `plan_l2`, prompt builders,
  output parsing with one retry on malformed ALF.
- `src/orchestra/diagnoser.rs` — `diagnose`, deterministic short-circuits
  before LLM call, output parsing with one retry.
- Fake `OllamaClient` test harness in `tests/common/fake_ollama.rs`.
- Tests: planner returns valid plan; malformed plan triggers retry; two
  failures trigger error; diagnoser short-circuits exercised; RetryWithHint
  without hint rejected.

**Acceptance:**

- `cargo test planner:: diagnoser:: keywords::` passes.
- Smoke test against a real local Ollama (qwen2.5-coder or similar)
  produces parseable plans for 5 canned goals.

**Load:** [00-format.md](00-format.md), [02-data-model.md](02-data-model.md),
[03-planner.md](03-planner.md), [06-diagnoser.md](06-diagnoser.md).

---

## Phase 5 — Worker

**Scope.** The micro-session agentic loop and final report extraction.

**Deliverables:**

- `src/orchestra/worker.rs` — `run_worker`, `instrumented_dispatch`,
  tool-call quota, file-change snapshot/diff capture (git fallback to
  manual diff).
- Integration with existing `tools::dispatch` and `PermissionStore`.
- Tests with fake Ollama: worker returns ok; worker exceeds max_rounds;
  worker emits prose; worker exceeds `max_tool_calls`.

**Acceptance:**

- Worker test suite green.
- Running a 4-round worker on a disposable temp workspace creates the
  expected files.

**Load:** [02-data-model.md](02-data-model.md), [04-worker.md](04-worker.md).

---

## Phase 6 — Driver state machine

**Scope.** The Rust loop that ties planner, worker, validator, diagnoser,
and store together.

**Deliverables:**

- `src/orchestra/driver.rs` — `run_orchestra`, `resume_orchestra`,
  `step`, `StepOutcome`, `DriverEvent`, `FinalReport`.
- Context-size guard with automatic compaction.
- Autonomous vs Interactive branching on escalation.
- Deferred-task handling and rescue pass.
- Tests: small canned goal against fake Ollama runs end-to-end and
  produces expected `FinalReport`; crash-resume replays the last
  incomplete phase.

**Acceptance:**

- End-to-end fake-Ollama test in `tests/orchestra_e2e.rs` passes.
- Manual run against a real local model completes a 3-L1-task goal
  ("create a tiny static site with index.html, styles.css, and a
  contact section").

**Load:** all previous files + [07-driver.md](07-driver.md).

---

## Phase 7 — TUI wiring

**Scope.** Surface Orchestra in the TUI: submission, event pump,
escalation modal, status line.

**Deliverables:**

- `src/tui/app.rs` changes per [09-integration.md](09-integration.md).
- `src/tui/widgets/` — escalation modal widget reusing permission modal
  code; orchestra phase indicator in the status bar.
- Slash commands: `/mode orchestra`, `/orchestra ...`, `/policy ...`.
- Config fields in `src/config/mod.rs`.
- Doctor check for `.allux/runs` writability.

**Acceptance:**

- Manual: TUI switches to Orchestra mode, runs a goal, renders phase
  updates, shows escalation modal on forced failure, resumes on user
  decision.
- No regressions in Agent / Chat / Plan modes.

**Load:** [09-integration.md](09-integration.md), all phase-6 files.

---

## Phase 8 — Documentation polish + example validation set

**Scope.** Update user-facing docs, add guide, land validation goals.

**Deliverables:**

- `docs/guides/orchestra.md` — user-facing guide with two example runs.
- Update `docs/architecture/overview.md` system diagram to mention
  Orchestra mode alongside Agent.
- `validation/orchestra/` — 6–8 goals exercising different failure
  paths, each with expected verdicts.
- Update `README.md` with a short Orchestra blurb under "Basic Usage".

**Acceptance:**

- Validation goals run under CI (slow job), either against a mock
  Ollama or a pinned local model image.

---

## Phase ordering notes

- Phases 2 and 3 are independent and can be parallelized.
- Phase 4 depends on 1; 5 depends on 1; 6 depends on 2, 3, 4, 5.
- Phase 7 depends on 6.
- Phase 8 is last.

## Non-goals repeated

- No new LLM provider abstraction; Ollama only.
- No concurrent workers.
- No multi-run orchestration (one run per workspace at a time).
- No automatic plan re-generation beyond `ReplanSubtree`.

## Risks & mitigations

| Risk | Mitigation |
|------|-----------|
| Small models produce malformed ALF | Two retries with error appended; fallback to `NeedsReview` |
| Cargo check is slow and dominates runtime | Auto-detect only; gate behind config flag per project |
| Validator false positives on legitimate placeholders | Planner writes `skip: no_placeholders` for those tasks |
| Lockfile conflicts if user runs two allux instances | Lock file prevents this; second instance errors clearly |
| Disk growth from events.log | Gzip on finalize; 50 MB soft cap per run; retention policy |

## Size checkpoints

Approximate LOC target per module to flag over-engineering early:

| Module | Target LOC |
|--------|-----------|
| `alf.rs` | ≤ 400 |
| `types.rs` | ≤ 500 (most is derives) |
| `planner.rs` | ≤ 300 |
| `worker.rs` | ≤ 400 |
| `diagnoser.rs` | ≤ 200 |
| `driver.rs` | ≤ 700 |
| `store.rs` | ≤ 500 |
| `validator/**` | ≤ 1,200 total |
| `keywords.rs` | ≤ 200 |

If any module exceeds its target by > 30%, pause and review for extraction
before merging that phase.
