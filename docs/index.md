---
layout: home
title: Allux — Local Code Agent
nav_order: 1
---

# Allux — Local Code Agent powered by Ollama

> *"Your local code craftsman, shaped by Ollama"*

**Allux** is an interactive terminal-based development agent written in **Rust** that uses local LLMs via [Ollama](https://ollama.com/) — 100% on your machine, no cloud, no API keys.

{: .highlight }
**Why local?** Local models keep your code private, work offline, and cost zero per token. Allux is designed to make local models as capable as possible through intelligent context management.

---

## Quick Navigation

- [Architecture Overview](architecture/overview) — How the system hangs together
- [Context Management](architecture/context-management) — The most critical component
- [Tool System](architecture/tools) — What the agent can do
- [Permission System](architecture/permissions) — How safety is enforced
- [Configuration Guide](guides/configuration) — `.allux.toml` reference
- [CLI Usage](guides/cli-usage) — Running Allux
- [Development Tasks](dev/tasks) — Task queue & iteration workflow
- [Feature Justification](dev/justification) — Why each feature exists

---

## Key Features

| Feature | Description |
|---|---|
| 🧠 Smart Context | Token budgeting so local models use every window byte wisely |
| 🛠 Tool System | read, edit, grep, bash, glob, tree — the LLM's hands |
| 🔒 Permissions | 4-scope grant model: once / session / workspace / global |
| 📡 Streaming | Real-time Ollama streaming with live tool call feedback |
| 💬 REPL | Colored terminal with full markdown & diff rendering |

---

## Get Started

```bash
git clone https://github.com/MarvinQuevedo/allux-agent.git
cd allux-agent
ollama pull qwen2.5-coder:14b
cargo build --release
./target/release/allux
```
