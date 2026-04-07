---
layout: default
title: CLI Usage
parent: Guides
nav_order: 2
---

# CLI Usage
{: .no_toc }

<details open markdown="block">
<summary>Table of contents</summary>
{: .text-delta }
1. TOC
{:toc}
</details>

---

## Starting Allux

Navigate to your project directory and run:

```bash
cd my-project
allux
```

Allux automatically:
1. Detects your project type (Rust, Node, Python, etc.)
2. Loads `.allux.toml` if present
3. Connects to Ollama
4. Opens the interactive REPL

---

## Command-Line Flags

```
allux [OPTIONS]

Options:
  --model <NAME>          Override the model from config
  --ollama-url <URL>      Override the Ollama server URL
  --no-tools              Disable all tools (pure chat mode)
  --verbose               Enable debug logging
  -h, --help              Show help
  -V, --version           Show version
```

Examples:

```bash
# Use a specific model
allux --model llama3.1:8b

# Connect to a remote Ollama instance
allux --ollama-url http://192.168.1.100:11434

# Pure chat mode (no file access)
allux --no-tools
```

---

## Slash Commands Reference

Use slash commands inside the REPL:

| Command | Description |
|---|---|
| `/help` | List all available commands |
| `/model <name>` | Switch to a different model |
| `/model list` | Show all locally available models |
| `/clear` | Clear the current conversation |
| `/undo` | Revert the last file edit made by the agent |
| `/sessions` | List all saved sessions |
| `/resume <id>` | Resume a previous session by ID |
| `/save` | Force-save the current session |
| `/permissions` | Show active permission grants |
| `/permissions revoke <key>` | Revoke a specific permission |
| `/exit` or `/quit` | Exit Allux |

---

## Interacting with the Agent

### Basic conversation

Just type naturally:

```
> Fix the bug in src/auth.rs where the token never expires
> Add unit tests for the UserService
> What does the ContextManager do?
> Refactor the connection pool to use a generic trait
```

### Direct file references

Mention files explicitly to include them in context:

```
> Look at src/repl/mod.rs and explain the tool dispatch flow
> Read Cargo.toml and add the `uuid` crate
```

### Asking the agent to run commands

```
> Run the tests and fix any failures
> Build the project and show me any warnings
```

The agent will ask for permission before executing (unless pre-approved).

---

## Permission Responses

When the agent asks for permission, respond with:

| Key | Action |
|---|---|
| `Enter` | Allow once |
| `Ctrl+S` | Allow for this session |
| `Ctrl+W` | Allow for this workspace (saved) |
| `Ctrl+G` | Allow globally (saved) |
| `Ctrl+N` | Reject |
| `?` | Ask the agent to explain the command |

---

## TypeScript Task Runner (Advanced)

For automated testing and task queues, use the included TypeScript CLI:

```bash
# List all defined tasks
npx tsx scripts/allux-cli.ts list

# Show a task's prompt
npx tsx scripts/allux-cli.ts show T01

# Run a specific task against Ollama
npx tsx scripts/allux-cli.ts run T01

# Ask a direct question (no task ID)
npx tsx scripts/allux-cli.ts ask "diagnose this repository" --autonomous

# Run the full task queue (T01 → T10)
npx tsx scripts/run-task-queue.ts
```

See [Development Tasks](../dev/tasks) for the full task queue documentation.

---

## Keyboard Shortcuts (REPL)

| Shortcut | Action |
|---|---|
| `↑` / `↓` | Navigate command history |
| `Ctrl+C` | Clear current input line |
| `Ctrl+L` | Scroll to bottom of conversation |
| `Page Up/Down` | Scroll conversation |
| `Ctrl+D` or `/exit` | Exit Allux |
