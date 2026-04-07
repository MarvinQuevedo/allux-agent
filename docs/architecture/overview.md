---
layout: default
title: Architecture Overview
parent: Architecture
nav_order: 1
---

# Architecture Overview
{: .no_toc }

<details open markdown="block">
<summary>Table of contents</summary>
{: .text-delta }
1. TOC
{:toc}
</details>

---

## System Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                       ALLUX TUI                             │
│  crossterm — markdown, diffs, confirmations, streaming      │
├─────────────────────────────────────────────────────────────┤
│                    Orchestrator (Core)                       │
│  ┌────────────┐ ┌────────────┐ ┌──────────┐ ┌───────────┐  │
│  │  Context    │ │ Permission │ │ Session  │ │ Slash Cmd │  │
│  │  Manager    │ │ Guard      │ │ Manager  │ │ Dispatcher│  │
│  └────────────┘ └────────────┘ └──────────┘ └───────────┘  │
├─────────────────────────────────────────────────────────────┤
│                    Tool Dispatcher                           │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌───────┐ ┌───────┐  │
│  │ Read │ │ Edit │ │ Bash │ │ Grep │ │ Web   │ │  MCP  │  │
│  └──────┘ └──────┘ └──────┘ └──────┘ └───────┘ └───────┘  │
├─────────────────────────────────────────────────────────────┤
│              Ollama Client (HTTP + Streaming)                │
│     POST /api/chat — chunked JSON streaming, tool calls      │
├─────────────────────────────────────────────────────────────┤
│              Session Store (disk persistence)                │
│     ~/.config/allux/sessions/ — resume, list, rename         │
└─────────────────────────────────────────────────────────────┘
         │                    │                    │
    Ollama Local         Internet             MCP Servers
    (local LLMs)       (web search)        (external tools)
```

---

## Core Components

| Component | Responsibility |
|---|---|
| **TUI** | Renders conversation, streams responses, shows diffs, handles confirmations |
| **Orchestrator** | Main loop: user → LLM → tool → LLM → response |
| **Context Manager** | Decides which files/info to include, manages token budget |
| **Permission Guard** | Evaluates action safety with 4-scope grant system |
| **Session Manager** | Persist/resume/list conversations across restarts |
| **Tool Dispatcher** | Executes tools, returns results to LLM |
| **Ollama Client** | HTTP streaming communication with local Ollama server |
| **Slash Cmd Dispatcher** | Parses and executes `/commands` (model, clear, undo, etc.) |

---

## Conversation Flow

```
User types a message
        │
        ▼
  REPL captures input
        │
        ▼
  Context Manager builds request
  (system prompt + project meta + history + relevant files)
        │
        ▼
  Ollama Client streams response
        │
   ┌────┴─────┐
   │ tool_call?│
   └────┬──────┘
        │ YES                     NO
        ▼                         ▼
  Permission Guard          Render response
  evaluates action          (markdown + streaming)
        │
   ┌────┴──────┐
   │ Approved? │
   └────┬──────┘
        │ YES (or auto)           NO
        ▼                         ▼
  Tool Executor            Show rejection message
  runs the tool
        │
        ▼
  Tool result appended as
  "tool" role message
        │
        ▼
  Back to Ollama
  (may chain more tool calls)
        │
        ▼
  Final text response rendered
```

---

## Module Map

```
src/
├── main.rs          # Entry point — loads config, launches Repl
├── config/
│   └── mod.rs       # Config loading from ~/.config/allux/ and .allux.toml
├── input/           # Raw terminal input, keyboard event handling
├── ollama/
│   ├── client.rs    # HTTP streaming client for /api/chat
│   ├── types.rs     # Ollama request/response types (serde)
│   └── mod.rs
├── permissions/
│   └── mod.rs       # PermissionGuard, PermissionScope, grant/revoke
├── repl/
│   ├── mod.rs       # Main REPL loop, tool dispatch, conversation management
│   ├── banner.rs    # Startup banner
│   ├── markdown.rs  # pulldown-cmark → terminal rendering
│   ├── chat_only.rs # Simplified chat-only mode
│   └── auto_scan.rs # Auto project scanning
├── setup/           # First-run wizard
├── tools/
│   ├── mod.rs       # Tool trait + dispatcher
│   ├── bash.rs      # Shell command execution
│   ├── edit_file.rs # Exact-string file editing
│   ├── glob_tool.rs # Glob pattern file search
│   ├── grep_tool.rs # Regex content search
│   ├── read_file.rs # File reading with line numbers
│   ├── tree.rs      # Directory tree
│   └── write_file.rs# File creation/overwrite
└── workspace/
    └── mod.rs       # Project root detection, workspace context
```

---

## Design Principles

1. **Software First, AI Second** — ~85% of features are pure deterministic code. The LLM handles only what genuinely requires reasoning.
2. **Every Token Counts** — Local models have limited windows. The Context Manager is the most critical component.
3. **User Always Wins** — No action modifies state without explicit user approval. The permission system is non-negotiable.
4. **Streaming by Default** — Users see responses character by character, not after a 10-second wait.
5. **Offline Capable** — Core features work without internet. Web search is opt-in.
