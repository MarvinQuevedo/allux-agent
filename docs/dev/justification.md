---
layout: default
title: Feature Justification
parent: Development
nav_order: 2
---

# Feature Justification & Resource Analysis
{: .no_toc }

> Why each feature exists, what uses AI vs pure software, and how we optimize local resource usage.

<details open markdown="block">
<summary>Table of contents</summary>
{: .text-delta }
1. TOC
{:toc}
</details>

---

## 1. AI vs Pure Software — Complete Map

Every feature in Allux falls into one of three categories:

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  PURE SOFTWARE (no AI, no LLM calls)                           │
│  ─────────────────────────────────────                          │
│  These run instantly, cost zero GPU/CPU inference time.         │
│                                                                 │
│    - TUI rendering (ratatui)                                    │
│    - File reading (read_file tool)                              │
│    - File searching (glob, grep tools)                          │
│    - Directory tree (tree tool)                                 │
│    - File editing (edit_file — exact string replace)            │
│    - File writing (write_file)                                  │
│    - Bash execution (bash tool — just spawn a process)          │
│    - Permission evaluation (rule matching, grant lookup)        │
│    - Diff computation & rendering                               │
│    - Undo system (file backup/restore)                          │
│    - Session save/load/listing                                  │
│    - Config parsing                                             │
│    - Token counting & usage display                             │
│    - Slash command parsing                                      │
│    - Project detection & Gitignore parsing                      │
│    - Markdown rendering & Syntax highlighting                   │
│    - HTML cleaning for web_fetch                                │
│    - Desktop notifications                                      │
│    - MCP protocol infrastructure                                │
│                                                                 │
│  AI-POWERED (requires LLM inference via Ollama)                │
│  ──────────────────────────────────────────                     │
│  These consume GPU time and tokens from the context window.     │
│                                                                 │
│    - Conversation responses (core chat)                         │
│    - Tool selection & Argument generation                       │
│    - Multi-step reasoning chains                                │
│    - History compression (summarization)                        │
│    - Session title generation                                   │
│    - "Explain command" in confirmations                         │
│                                                                 │
│  HYBRID (mostly software, AI assists occasionally)             │
│  ──────────────────────────────────────────                     │
│    - Context resolution (heuristics vs AI re-ranking)           │
│    - Smart truncation (line-based vs AI-picked sections)        │
│    - Project map summary generation                             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### The Ratio

```
Pure Software:  ~85% of features
AI-Powered:     ~12% of features
Hybrid:         ~3% of features

Allux is a software tool FIRST, that uses AI as the "brain".
```

---

## 2. Key Justifications

### Exact String Replacement (`edit_file`)
Deterministic, verifiable, and reversible. AI doesn't rebuild the whole file, saving output tokens and preventing corruption of unrelated lines.

### Permission System
Safety must be deterministic. Pattern matching is used instead of AI reasoning to prevent bypasses via prompt injection.

### History Compression
Necessary for long sessions. AI summarizes key decisions and facts, while deterministic truncation would lose critical context.

---

## 3. Resource Usage Profile

| Resource | Usage |
|---|---|
| **GPU/VRAM** | 100% occupied by the model (~6-12GB). Active only during inference. |
| **CPU** | TUI, Regex, Diff, I/O. Usually <5% unless doing heavy regex search. |
| **RAM** | History, Render cache, Undo stack. Typical: ~50-100MB. |
| **Network** | Opt-in (web search). Localhost for Ollama is immediate. |

---

## 4. Processing Cost Matrix

| Operation | Time | Token Impact |
|---|---|---|
| **LLM Response** | 3-30s | 🔴 High |
| **Grep Search** | 10-500ms | 🟢 Negligible |
| **Markdown Render** | 1-5ms | 🟢 Negligible |
| **History Compression** | 3-10s | 🟡 Medium (rare) |

---

## 5. The Core Insight

{: .important }
95% of Allux's code handles pure software tasks that cost nearly zero processing time. **5% of the code (the Ollama client) drives 99% of the cost.** Every token saved in context is a direct performance win.
