# Allux — Local Code Agent powered by Ollama

> *"Your local code craftsman, shaped by Ollama"*

[![License: GPL-3.0](https://img.shields.io/badge/License-GPL%203.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Ollama](https://img.shields.io/badge/Backend-Ollama-black.svg)](https://ollama.com/)
[![Status](https://img.shields.io/badge/Status-Active%20Development-green.svg)]()

**Allux** is an interactive terminal-based development agent written in Rust. It uses local LLMs via [Ollama](https://ollama.com/) to assist with software engineering tasks — 100% on your machine, no cloud, no API keys.

---

## Why "Allux"?

An **allux** is a Spanish word for a craftsman who shapes pottery with their own hands — working locally, with their own materials, no external dependencies. Just like this agent: it runs entirely on your machine using local models via Ollama. The name also has a natural phonetic link with "Ollama" (All-ux / Oll-ama), reinforcing the local inference connection.

---

## ✨ Features

| Feature | Status |
|---|---|
| 🧠 **Intelligent Context Management** — smart token budgeting for local models | ✅ |
| 🛠 **Tool System** — read, edit, grep, bash, glob, tree | ✅ |
| 🔒 **Permission & Security System** — 4-scope grant model (once/session/workspace/global) | ✅ |
| 📡 **Ollama Integration** — streaming HTTP, tool calling, multi-step reasoning | ✅ |
| 💬 **Interactive REPL** — colored terminal with markdown rendering | ✅ |
| 🔎 **Web Search & Fetch** — internet access for current docs | 🚧 |
| 📋 **Session Persistence** — resume conversations across restarts | 🚧 |
| ↩️ **Diff Rendering & Undo** — see exactly what changed, revert instantly | 🚧 |
| ⚙️ **Slash Commands** — `/model`, `/undo`, `/sessions`, `/clear` | 🚧 |
| 🔌 **MCP Protocol Support** — external tool servers | 📋 |

---

## 🚀 Quick Start

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable)
- [Ollama](https://ollama.com/) running locally

### Installation

```bash
# Clone the repo
git clone https://github.com/MarvinQuevedo/allux-agent.git
cd allux-agent

# Pull a recommended model
ollama pull qwen2.5-coder:14b

# Build
cargo build --release

# Run
./target/release/allux
```

### First Run

On first launch, Allux runs an interactive setup wizard to configure:
- Your preferred Ollama model
- Permission mode (`balanced`, `paranoid`, or `yolo`)
- Project-level settings (optional `.allux.toml`)

---

## 📖 Documentation

Full documentation is available in the [`docs/`](docs/) folder and as [GitHub Pages](https://marvinquevedo.github.io/allux-agent/).

| Document | Description |
|---|---|
| [Architecture](docs/architecture/overview.md) | System design, component map, conversation flow |
| [Context Management](docs/architecture/context-management.md) | Token budgeting, smart truncation, history compression |
| [Tool System](docs/architecture/tools.md) | Tool inventory, protocol, implementations |
| [Permission System](docs/architecture/permissions.md) | 4-scope model, UX, hardcoded safety rules |
| [Ollama Integration](docs/architecture/ollama.md) | HTTP streaming, tool calling protocol |
| [Configuration](docs/guides/configuration.md) | `.allux.toml` reference |
| [CLI Usage](docs/guides/cli-usage.md) | Commands, flags, examples |
| [Development Tasks](docs/dev/tasks.md) | Iteration workflow & task queue |
| [Feature Justification](docs/dev/justification.md) | Why each feature exists, AI vs pure software |

---

## 🏗 Project Structure

```
allux-agent/
├── src/
│   ├── main.rs          # Binary entry point
│   ├── config/          # Config loading (.allux.toml, global config)
│   ├── input/           # Terminal input handling (keyboard, raw mode)
│   ├── ollama/          # Ollama HTTP client & types
│   ├── permissions/     # Permission guard & grant system
│   ├── repl/            # Interactive REPL (TUI, markdown, banner)
│   ├── setup/           # First-run wizard
│   ├── tools/           # Tool implementations (bash, grep, read, edit…)
│   └── workspace/       # Project detection & workspace management
├── scripts/             # TypeScript automation & task runner
├── docs/                # Documentation (GitHub Pages)
├── validation/          # Manual validation suite
├── Cargo.toml           # Rust dependencies
└── .allux.toml          # (optional) Project-level config
```

---

## ⚙️ Configuration

Create `.allux.toml` in your project root:

```toml
[context]
always_include = ["src/types.rs", "docs/ARCHITECTURE.md"]
never_include  = ["*.lock", "*.min.js", "dist/**"]

[instructions]
system = """
This project uses hexagonal architecture.
Tests go in _test.rs files next to the module.
Prefer Result<T, anyhow::Error> over unwrap().
"""

[model]
default = "qwen2.5-coder:14b"
max_context_tokens = 8192

[permissions]
mode = "balanced"  # "paranoid" | "balanced" | "yolo"
```

---

## 🔐 Permission Model

Allux never runs anything without your approval. When it wants to execute a command or edit a file, you choose how long the permission lasts:

| Scope | Duration |
|---|---|
| **Once** | Just this single action |
| **Session** | Until you quit Allux |
| **Workspace** | Saved to `.allux/permissions.json` for this project |
| **Global** | Saved to `~/.config/allux/permissions.json` for all projects |

Some actions (e.g. `rm -rf`, force push) are **hardcoded denies** — they can never be approved, regardless of mode.

---

## 🤝 Contributing

This is an active personal project. Issues and PRs are welcome.

See [Contributing Guide](docs/CONTRIBUTING.md) for code style, architecture guidelines, and development workflow.

---

## 📄 License

GPL-3.0-or-later — see [LICENSE](LICENSE).
