---
layout: default
title: Contributing
nav_order: 5
---

# Contributing to Allux
{: .no_toc }

<details open markdown="block">
<summary>Table of contents</summary>
{: .text-delta }
1. TOC
{:toc}
</details>

---

## Development Setup

```bash
# Clone
git clone https://github.com/MarvinQuevedo/allux-agent.git
cd allux-agent

# Requirements
rustup update stable
ollama pull qwen2.5-coder:14b

# Build
cargo build

# Run tests
cargo test

# Run in dev mode
cargo run
```

---

## Code Style

- **No `unwrap()`** in library code — use `Result<T, anyhow::Error>`
- **Async/await** with Tokio — no blocking calls on async tasks
- **One responsibility per module** — keep files focused
- Clippy warnings are treated as errors in CI: `cargo clippy -- -D warnings`

---

## Architecture Principles

Before adding a feature, ask:

1. **Is this pure software or does it need AI?** — If deterministic, write software. Don't call the LLM.
2. **Does this consume tokens?** — Every token in context slows inference. Justify the cost.
3. **Does this require a permission check?** — If it modifies state, it must go through `PermissionGuard`.
4. **Is this reversible?** — File edits must push to the undo stack before writing.

---

## Adding a New Tool

1. Create `src/tools/my_tool.rs`
2. Implement the `Tool` trait
3. Register in `src/tools/mod.rs` dispatcher
4. Add to the tool list sent to Ollama in `src/repl/mod.rs`
5. Assign the correct `RiskLevel`
6. Document in `docs/architecture/tools.md`

---

## Pull Request Guidelines

- Keep PRs focused — one feature or fix per PR
- Run `cargo test` and `cargo clippy` before submitting
- Update `docs/` if the change affects user-facing behavior
- Reference any related tasks from `docs/dev/tasks.md`

---

## Project Roadmap

See [Development Tasks](dev/tasks) for the current task queue and planned features.
