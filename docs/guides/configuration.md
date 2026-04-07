---
layout: default
title: Configuration
parent: Guides
nav_order: 1
---

# Configuration Reference
{: .no_toc }

<details open markdown="block">
<summary>Table of contents</summary>
{: .text-delta }
1. TOC
{:toc}
</details>

---

## Config File Locations

Allux reads configuration from **two locations**, merged in order:

| File | Scope | Description |
|---|---|---|
| `~/.config/allux/config.toml` | Global | Default model, global permissions |
| `.allux.toml` (project root) | Project | Overrides global, project-specific instructions |

Project config always wins over global config.

---

## Full `.allux.toml` Reference

```toml
# ─────────────────────────────────────────────
# CONTEXT MANAGEMENT
# ─────────────────────────────────────────────
[context]

# Files that ALWAYS go into the LLM context (before any file discovery)
always_include = [
    "src/types.rs",
    "docs/ARCHITECTURE.md",
]

# Files/patterns that are NEVER included (even if the LLM requests them)
never_include = [
    "*.lock",
    "*.min.js",
    "dist/**",
    "node_modules/**",
    ".env",            # Never expose secrets!
    "target/**",
]

# ─────────────────────────────────────────────
# CUSTOM SYSTEM INSTRUCTIONS
# ─────────────────────────────────────────────
[instructions]

# Appended to the system prompt for every conversation in this project
system = """
This project uses hexagonal architecture.
Tests go in _test.rs files next to the module.
We prefer Result<T, anyhow::Error> over unwrap().
Always run `cargo clippy` after edits.
"""

# ─────────────────────────────────────────────
# MODEL SETTINGS
# ─────────────────────────────────────────────
[model]

# Primary model to use
default = "qwen2.5-coder:14b"

# Fallback if default is not available
fallback = "llama3.1:8b"

# Max tokens to use for context (input side)
# Adjust based on your model's actual context window
max_context_tokens = 8192

# Soft limit for history before compression kicks in
history_soft_limit_tokens = 60000

# Hard limit before emergency eviction
history_hard_limit_tokens = 120000

# Temperature (0.0–1.0, lower = more deterministic)
temperature = 0.1

# ─────────────────────────────────────────────
# PERMISSION SETTINGS
# ─────────────────────────────────────────────
[permissions]

# "paranoid" — ask for everything
# "balanced" — reads free, writes/commands ask (default)
# "yolo"     — auto-approve everything except hardcoded denies
mode = "balanced"
```

---

## Global Config (`~/.config/allux/config.toml`)

The global config has the same format as `.allux.toml` but applies to all projects:

```toml
[model]
default = "qwen2.5-coder:14b"
temperature = 0.1

[permissions]
mode = "balanced"
```

---

## Runtime Overrides (Slash Commands)

You can change some settings mid-session without editing files:

| Command | Effect |
|---|---|
| `/model <name>` | Switch the active model |
| `/model list` | Show available Ollama models |
| `/clear` | Clear conversation history |
| `/undo` | Revert the last file edit |
| `/sessions` | List saved sessions |
| `/resume <id>` | Resume a previous session |
| `/permissions` | Show current permission grants |
| `/help` | Show all slash commands |

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `ALLUX_OLLAMA_URL` | `http://localhost:11434` | Ollama server URL |
| `ALLUX_CONFIG_DIR` | `~/.config/allux` | Override config directory |
| `ALLUX_LOG` | (none) | Set to `debug` or `trace` for verbose logs |
