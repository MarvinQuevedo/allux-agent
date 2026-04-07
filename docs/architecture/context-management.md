---
layout: default
title: Context Management
parent: Architecture
nav_order: 2
---

# Intelligent Context Management
{: .no_toc }

{: .highlight }
This is the **most critical component** of Allux. Local models have limited context windows (4K–128K tokens), so every token counts. A good Context Manager can cut inference time in half. A bad one doubles it.

<details open markdown="block">
<summary>Table of contents</summary>
{: .text-delta }
1. TOC
{:toc}
</details>

---

## Layered Context Strategy

```
┌─────────────────────────────────────────────┐
│  Layer 1: System Prompt (fixed, ~500 tok)   │  ← Always present
├─────────────────────────────────────────────┤
│  Layer 2: Project Meta (~200 tok)           │  ← Always present
│  - Project name, detected languages          │
│  - Directory structure (summary, 2-3 levels) │
│  - Key files identified                      │
│  - Dependencies summary                     │
├─────────────────────────────────────────────┤
│  Layer 3: Relevant Files (variable)         │  ← On demand
│  - Only files the LLM requested via tools    │
│  - Truncated if too long                     │
│  - With line numbers                         │
├─────────────────────────────────────────────┤
│  Layer 4: Conversation History (compressed) │  ← Managed
│  - Last N messages in full                   │
│  - Older messages → summary                  │
│  - Soft limit: 60K tokens                    │
│  - Hard limit: 120K tokens (emergency evict) │
├─────────────────────────────────────────────┤
│  Layer 5: Tool Results (ephemeral)          │  ← Discarded after use
│  - Command output                            │
│  - Search results                            │
│  - Truncated to max_output_chars             │
└─────────────────────────────────────────────┘
```

---

## Automatic Project Detection

On startup, Allux scans the working directory and builds a **project map**:

```rust
struct ProjectMap {
    root: PathBuf,
    languages: Vec<(Language, f32)>,
    tree: DirectoryTree,
    key_files: Vec<KeyFile>,
    dependencies: Vec<String>,
    ignore_patterns: Vec<String>,
    git_branch: Option<String>,
}
```

**Auto-detected key files:**

| Pattern | Kind |
|---|---|
| `README.md`, `CLAUDE.md`, `.allux.toml` | Docs / Config |
| `Cargo.toml`, `package.json`, `pyproject.toml` | Dependencies |
| `src/main.rs`, `src/lib.rs`, `index.ts`, `app.py` | Entry Points |
| `schema.prisma`, `migrations/` | DB Schemas |
| `Dockerfile`, `docker-compose.yml` | Infrastructure |
| `.env.example` | Env vars (never `.env` itself) |

---

## Intelligent File Inclusion

When the user sends a message, the Context Manager decides which files to include:

1. **Extract explicit file mentions** — "look at `src/auth.rs`" → include `src/auth.rs`
2. **Find symbol definitions** — "fix the `AuthError` type" → find where `AuthError` is defined
3. **Prioritize recent files** — files accessed earlier in this session rank higher
4. **Fit to token budget** — truncate or exclude to stay within limits

```rust
impl ContextManager {
    fn resolve_context(&self, user_message: &str, budget: &ContextBudget) -> Context {
        let mentioned_files = self.extract_file_references(user_message);
        let symbol_files = self.find_symbol_definitions(user_message);
        let recent_files = self.session_recent_files();
        self.fit_to_budget(mentioned_files, symbol_files, recent_files, budget)
    }
}
```

---

## History Compression

Inspired by Claude Code's retention policy:

| Threshold | Action |
|---|---|
| History < 60K tokens | Keep all messages in full |
| History > 60K tokens (soft limit) | Summarize oldest messages via LLM |
| History > 120K tokens (hard limit) | Emergency evict oldest messages |

```rust
enum MessageState {
    Full { content: String, retained_bytes: usize },
    Summarized(String),
    Evicted { role: Role, timestamp: DateTime<Utc>, topic: String },
}
```

---

## Token Budget Configuration

```toml
# .allux.toml
[model]
default = "qwen2.5-coder:14b"
max_context_tokens = 8192  # Adjust per model
```

**Typical budget breakdown for an 8K-token model:**

| Layer | Tokens | Notes |
|---|---|---|
| System prompt | ~500 | Fixed |
| Project meta | ~200 | Once on startup |
| Relevant files | ~2000 | On demand, truncated |
| History | ~4000 | Compressed to soft limit |
| Reserved for response | ~1000 | Never filled with input |

---

## Optimization Impact Table

| Strategy | Tokens Saved | Impact |
|---|---|---|
| Smart file truncation | 500–5,000 per file | 🔴 High |
| History compression | 2,000–20,000 | 🔴 High |
| Evict tool results after use | 500–5,000 per result | 🔴 High |
| Never include binary files | 1,000–100,000+ | 🔴 High |
| Concise system prompt | 200–500 | 🟡 Medium |
| Project meta summary (not full tree) | 500–2,000 | 🟡 Medium |
