---
layout: default
title: Ollama Integration
parent: Architecture
nav_order: 5
---

# Ollama Integration
{: .no_toc }

<details open markdown="block">
<summary>Table of contents</summary>
{: .text-delta }
1. TOC
{:toc}
</details>

---

## Overview

Allux communicates with Ollama via its HTTP API. All communication uses **chunked streaming** so responses appear character by character rather than after a cold wait.

Default endpoint: `http://localhost:11434`

---

## Supported Models

Any Ollama model that supports **tool calling** works with Allux. Recommended:

| Model | Size | VRAM | Notes |
|---|---|---|---|
| `qwen2.5-coder:14b` | ~9GB | ~12GB | **Recommended** — best code quality |
| `qwen2.5-coder:7b` | ~5GB | ~6GB | Good balance on 8GB VRAM |
| `llama3.1:8b` | ~5GB | ~6GB | General purpose |
| `deepseek-coder-v2:16b` | ~10GB | ~14GB | Strong coding, high VRAM |

---

## Chat Request Format

```json
POST /api/chat
{
  "model": "qwen2.5-coder:14b",
  "stream": true,
  "messages": [
    { "role": "system", "content": "..." },
    { "role": "user",   "content": "Fix the auth bug" },
    { "role": "tool",   "content": "...", "tool_call_id": "..." }
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "read_file",
        "description": "Read a project file with line numbers",
        "parameters": {
          "type": "object",
          "properties": {
            "path": { "type": "string" },
            "start_line": { "type": "integer" },
            "end_line": { "type": "integer" }
          },
          "required": ["path"]
        }
      }
    }
  ]
}
```

---

## Streaming Response Parsing

Ollama streams newline-delimited JSON chunks:

```json
{"model":"qwen2.5-coder:14b","message":{"role":"assistant","content":"I'll"},"done":false}
{"model":"qwen2.5-coder:14b","message":{"role":"assistant","content":" look"},"done":false}
...
{"model":"qwen2.5-coder:14b","message":{"role":"assistant","tool_calls":[...]},"done":false}
{"model":"qwen2.5-coder:14b","done":true,"eval_count":42,"eval_duration":1234567890}
```

The final `done: true` chunk includes:
- `eval_count` — output tokens generated
- `eval_duration` — nanoseconds for generation
- `prompt_eval_count` — input tokens processed

These power the **token & speed display** in the TUI.

---

## Tool Call Handling

When Ollama returns `tool_calls` in a chunk:

1. Allux collects all tool calls from the response
2. For each tool call: evaluate permissions → execute (if approved) → collect result
3. **All results are sent together** in a single follow-up message (batch efficiency)
4. Ollama generates the next response (may chain into more tool calls)

---

## Health Check

On startup, Allux pings Ollama:

```
GET http://localhost:11434/api/tags
```

If Ollama is not running:
- Display a clear error with instructions to start Ollama
- Offer to retry after the user starts it (non-fatal)

---

## Configuration

```toml
# .allux.toml
[model]
default = "qwen2.5-coder:14b"
fallback = "llama3.1:8b"          # Used if default unavailable
max_context_tokens = 8192
temperature = 0.1                  # Low for code, higher for creative
```

Override at runtime with the `/model` slash command:
```
/model qwen2.5-coder:7b
```
