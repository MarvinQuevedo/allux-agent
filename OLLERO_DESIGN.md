# OLLERO — Local Code Agent powered by Ollama

> *"Your local code craftsman, shaped by Ollama"*

### Why "Ollero"?

An **ollero** is a Spanish word for a craftsman who shapes pottery with their own hands — working locally, with their own materials, no external dependencies. Just like this agent: it runs 100% on your machine using local models via **Ollama**. The name also has a natural phonetic link with "Ollama" (Oll-ero / Oll-ama), reinforcing the connection to the local inference backend. In short: a local craftsman that shapes your code.

---

Interactive terminal-based development agent written in Rust. Uses local LLMs via Ollama to assist with software engineering tasks. Features intelligent context management, safe command execution, internet access, session persistence, diff rendering, and a production-grade TUI.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Intelligent Context Management](#2-intelligent-context-management)
3. [Tool System](#3-tool-system)
4. [Permission & Security System](#4-permission--security-system)
5. [Ollama Integration](#5-ollama-integration)
6. [Web Search & Fetch](#6-web-search--fetch)
7. [Terminal UI (TUI)](#7-terminal-ui-tui)
8. [Session Management](#8-session-management)
9. [Diff Rendering & Undo System](#9-diff-rendering--undo-system)
10. [Slash Commands](#10-slash-commands)
11. [Configuration System](#11-configuration-system)
12. [Token Tracking & Cost Awareness](#12-token-tracking--cost-awareness)
13. [MCP Protocol Support](#13-mcp-protocol-support)
14. [Project Structure (Rust)](#14-project-structure-rust)
15. [Conversation Flow](#15-conversation-flow)
16. [Key Dependencies (Crates)](#16-key-dependencies-crates)
17. [Implementation Phases](#17-implementation-phases)

---

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                       OLLERO TUI                             │
│  ratatui + crossterm — markdown, diffs, confirmations       │
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
│     ~/.config/ollero/sessions/ — resume, list, rename         │
└─────────────────────────────────────────────────────────────┘
         │                    │                    │
    Ollama Local         Internet             MCP Servers
    (local LLMs)       (web search)        (external tools)
```

### Core Components

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

## 2. Intelligent Context Management

This is the **most critical component**. Local models have limited context windows (4K–128K tokens depending on model), so every token counts.

### 2.1 Layered Context Strategy

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

### 2.2 Automatic Project Detection

On startup, OLLERO scans the working directory and builds a project map:

```rust
struct ProjectMap {
    /// Project root (where .git, Cargo.toml, package.json, etc. live)
    root: PathBuf,
    /// Detected languages with approximate percentage
    languages: Vec<(Language, f32)>,
    /// Directory tree (2-3 levels deep only)
    tree: DirectoryTree,
    /// Key files detected automatically
    key_files: Vec<KeyFile>,
    /// Project dependencies (from Cargo.toml, package.json, etc.)
    dependencies: Vec<String>,
    /// Gitignore patterns (to exclude from context)
    ignore_patterns: Vec<String>,
    /// Current git branch (if applicable)
    git_branch: Option<String>,
}

struct KeyFile {
    path: PathBuf,
    kind: KeyFileKind, // Config, EntryPoint, Schema, Test, Documentation
    priority: u8,      // 1-10, higher = more important
}
```

**Auto-detected key files:**
- `README.md`, `CLAUDE.md`, `.ollero.toml` → docs/config
- `Cargo.toml`, `package.json`, `pyproject.toml` → dependencies
- `src/main.rs`, `src/lib.rs`, `index.ts`, `app.py` → entry points
- `schema.prisma`, `migrations/` → DB schemas
- `Dockerfile`, `docker-compose.yml` → infrastructure
- `.env.example` → env vars (never `.env` itself)

### 2.3 Intelligent File Inclusion

```rust
struct ContextBudget {
    /// Max tokens for context (depends on model)
    max_tokens: usize,
    /// Tokens already consumed by system prompt + project meta
    used_tokens: usize,
    /// Tokens reserved for model response
    reserved_for_response: usize,
}

impl ContextManager {
    /// Given a user message, decide which files are relevant
    fn resolve_context(&self, user_message: &str, budget: &ContextBudget) -> Context {
        // 1. Extract explicit file mentions from the message
        let mentioned_files = self.extract_file_references(user_message);
        
        // 2. If user mentions a function/class, find where it's defined
        let symbol_files = self.find_symbol_definitions(user_message);
        
        // 3. Prioritize recently accessed files in this session
        let recent_files = self.session_recent_files();
        
        // 4. Prioritize and truncate to fit budget
        self.fit_to_budget(mentioned_files, symbol_files, recent_files, budget)
    }

    /// Smart truncation for large files
    fn smart_truncate(&self, content: &str, max_lines: usize) -> String {
        // - Keep first lines (imports, declarations)
        // - Keep the relevant section (if a specific function was requested)
        // - Replace middle sections with "... (X lines omitted) ..."
    }
}
```

### 2.4 History Retention & Compression

Inspired by claude-code-rust's retention policy with soft/hard limits:

```rust
struct HistoryRetention {
    /// When total history exceeds this, start summarizing old messages
    soft_limit_tokens: usize, // default: 60_000
    /// When total exceeds this, force-evict oldest messages
    hard_limit_tokens: usize, // default: 120_000
}

enum MessageState {
    /// Full message, as-is
    Full { content: String, retained_bytes: usize },
    /// Summary generated by the LLM
    Summarized(String),
    /// Evicted (only metadata kept)
    Evicted { role: Role, timestamp: DateTime<Utc>, topic: String },
}

impl ConversationManager {
    fn compress_if_needed(&mut self, budget: &ContextBudget) {
        let total = self.total_retained_bytes();
        
        if total > self.retention.hard_limit_tokens {
            // Emergency: evict oldest messages entirely
            self.evict_oldest_until(self.retention.soft_limit_tokens);
        } else if total > self.retention.soft_limit_tokens {
            // Summarize older messages into a single context block
            self.summarize_oldest_messages();
        }
    }
}
```

### 2.5 Configuration File `.ollero.toml`

```toml
# .ollero.toml — in project root

[context]
# Files that ALWAYS go into context
always_include = [
    "src/types.rs",
    "docs/ARCHITECTURE.md",
]

# Files that NEVER get included
never_include = [
    "*.lock",
    "*.min.js",
    "dist/**",
    "node_modules/**",
    ".env",
]

# Custom instructions for the system prompt
[instructions]
system = """
This project uses hexagonal architecture.
Tests go in _test.rs files next to the module.
We prefer Result<T, anyhow::Error> over unwrap().
"""

[model]
default = "qwen2.5-coder:14b"
fallback = "llama3.1:8b"
max_context_tokens = 8192

[permissions]
mode = "balanced"  # "paranoid", "balanced", "yolo"
```

---

## 3. Tool System

### 3.1 Tool Trait

```rust
#[async_trait]
trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    fn risk_level(&self) -> RiskLevel;
    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolResult>;
}

enum RiskLevel {
    /// Read-only: file reads, searches
    Safe,
    /// Modifies local project files
    Moderate,
    /// Executes commands, accesses internet
    Dangerous,
    /// Destructive: delete files, force push, etc.
    Critical,
}
```

### 3.2 Tool Inventory

| Tool | Risk | Description |
|---|---|---|
| `read_file` | Safe | Read a file with line numbers, optional range |
| `glob` | Safe | Find files by glob pattern (respects .gitignore) |
| `grep` | Safe | Search file contents with regex |
| `tree` | Safe | Show directory structure |
| `edit_file` | Moderate | Replace exact text in a file (diff-based) |
| `write_file` | Moderate | Create or overwrite a file |
| `bash` | Dangerous | Execute a shell command with timeout |
| `web_search` | Moderate | Search the internet |
| `web_fetch` | Moderate | Download and clean a web page |
| `ask_user` | Safe | Ask the user a question with options |
| `todo_write` | Safe | Track tasks/progress within a session |
| `mcp_call` | Varies | Call external MCP server tools |

### 3.3 Tool Call Protocol with Ollama

```
1. User sends message
2. OLLERO builds request with available tools:

POST /api/chat
{
  "model": "qwen2.5-coder:14b",
  "messages": [...],
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
  ],
  "stream": true
}

3. Ollama responds with tool_calls or text content
4. If tool_calls:
   a. OLLERO evaluates permissions
   b. Executes the tool
   c. Sends result back to model as "tool" role message
   d. Model generates next response (may request more tools)
5. Repeat until model responds with text only (no more tool_calls)
```

### 3.4 Tool Call Rendering in TUI

Inspired by claude-code-rust's ToolCallContent model:

```rust
/// How a tool call is represented in the UI
struct ToolCallInfo {
    id: String,
    tool_name: String,
    status: ToolCallStatus,
    /// Rich content for display
    content: Vec<ToolCallContent>,
    /// Timing info
    started_at: Instant,
    completed_at: Option<Instant>,
    /// Collapsible in TUI
    collapsed: bool,
}

enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed(String),
}

enum ToolCallContent {
    /// Plain text output (command stdout, search results)
    Text(String),
    /// Unified diff (for edit_file / write_file)
    Diff {
        path: String,
        old_content: String,
        new_content: String,
        hunks: Vec<DiffHunk>,
    },
    /// Terminal output (for bash tool)
    Terminal {
        command: String,
        stdout: String,
        stderr: String,
        exit_code: i32,
    },
}
```

### 3.5 Key Tool Implementations

```rust
/// read_file — Read files with line range control
struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let path = params["path"].as_str().unwrap();
        let abs_path = ctx.resolve_path(path)?;
        
        ctx.assert_within_project(&abs_path)?;
        ctx.assert_not_sensitive(&abs_path)?;
        
        let content = tokio::fs::read_to_string(&abs_path).await?;
        let lines: Vec<&str> = content.lines().collect();
        
        let start = params.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        let end = params.get("end_line").and_then(|v| v.as_u64())
            .unwrap_or(lines.len() as u64) as usize;
        
        let max_lines = 200;
        let (actual_end, truncated) = if end - start > max_lines {
            (start + max_lines, true)
        } else {
            (end, false)
        };
        
        let numbered: String = lines[start-1..actual_end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>4}\t{}", start + i, line))
            .collect::<Vec<_>>()
            .join("\n");
        
        let mut result = numbered;
        if truncated {
            result.push_str(&format!(
                "\n... ({} more lines omitted, use start_line/end_line)",
                end - actual_end
            ));
        }
        
        Ok(ToolResult::text(result))
    }
}

/// bash — Execute commands with sandboxing
struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn risk_level(&self) -> RiskLevel { RiskLevel::Dangerous }
    
    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let command = params["command"].as_str().unwrap();
        let timeout_ms = params.get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30_000);
        
        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .current_dir(&ctx.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let result = tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            output.wait_with_output()
        ).await??;
        
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        
        let max_output_chars = 10_000;
        
        Ok(ToolResult::terminal(
            command.to_string(),
            truncate_smart(&stdout, max_output_chars),
            truncate_smart(&stderr, max_output_chars / 2),
            result.status.code().unwrap_or(-1),
        ))
    }
}

/// edit_file — Exact string replacement (like Claude Code's Edit)
struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn risk_level(&self) -> RiskLevel { RiskLevel::Moderate }
    
    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let path = params["path"].as_str().unwrap();
        let old_string = params["old_string"].as_str().unwrap();
        let new_string = params["new_string"].as_str().unwrap();
        let abs_path = ctx.resolve_path(path)?;
        
        ctx.assert_within_project(&abs_path)?;
        
        let content = tokio::fs::read_to_string(&abs_path).await?;
        
        let count = content.matches(old_string).count();
        if count == 0 {
            return Ok(ToolResult::error("old_string not found in file"));
        }
        if count > 1 {
            return Ok(ToolResult::error(format!(
                "old_string appears {} times. Provide more surrounding context.",
                count
            )));
        }
        
        let new_content = content.replacen(old_string, new_string, 1);
        
        // Save to undo stack BEFORE writing
        ctx.undo_stack.push(UndoEntry {
            path: abs_path.clone(),
            old_content: content.clone(),
            timestamp: Utc::now(),
        });
        
        tokio::fs::write(&abs_path, &new_content).await?;
        
        // Return as diff for TUI rendering
        Ok(ToolResult::diff(path.to_string(), &content, &new_content))
    }
}
```

---

## 4. Permission & Security System

### 4.1 Philosophy — The Claude Code Model

> **The agent ALWAYS asks before executing anything that modifies state.**
> The user decides the **temporal scope** of each permission grant: just this once, this session, or permanently for this workspace.

OLLERO never "takes control" — the user always has the final word, but can progressively delegate trust as they get comfortable.

### 4.2 The 4 Permission Scopes

When OLLERO asks for permission and the user accepts, they choose **how long** that permission lasts:

```
┌───────────────────────────────────────────────────────────────────┐
│                     PERMISSION SCOPES                             │
├───────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. ONCE — Execute once, then forget. Next time, ask again.       │
│     Use case: one-off commands you want to review each time.      │
│                                                                   │
│  2. SESSION — Remembered while OLLERO is running.                  │
│     Forgotten when OLLERO exits. Use case: "let cargo test         │
│     run freely while I work on this bug."                         │
│                                                                   │
│  3. WORKSPACE — Saved to .ollero/permissions.json.                 │
│     Persists across sessions for THIS project only.               │
│     Use case: "cargo build is always OK in this project."         │
│                                                                   │
│  4. GLOBAL — Saved to ~/.config/ollero/permissions.json.           │
│     Applies to ALL projects. Use case: "git status, ls,           │
│     reading files — never ask me about these."                    │
│                                                                   │
└───────────────────────────────────────────────────────────────────┘
```

### 4.3 Permission Types & Keys

```rust
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
struct PermissionKey {
    /// Tool or category: "bash", "edit_file", "web_search", etc.
    tool: String,
    /// Pattern within the tool.
    /// For bash: command prefix ("cargo *", "git status", "npm run *")
    /// For edit: path glob ("src/**", "tests/**")
    /// For web_search: "*" (always generic)
    pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GrantedPermission {
    key: PermissionKey,
    scope: PermissionScope,
    granted_at: DateTime<Utc>,
    use_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum PermissionScope {
    Once,
    Session,
    Workspace,
    Global,
}
```

### 4.4 Confirmation UX — Keyboard Shortcuts

Inspired by claude-code-rust's smart option matching with keyboard shortcuts:

```
┌─────────────────────────────────────────────────────────────────┐
│  OLLERO wants to execute:                                        │
│                                                                 │
│    $ cargo test --lib                                           │
│                                                                 │
│  ───────────────────────────────────────────────────────────    │
│                                                                 │
│  [Enter]    Allow this once                                     │
│  [Ctrl+S]   Allow for this session                              │
│  [Ctrl+W]   Allow always in this workspace                      │
│  [Ctrl+G]   Allow globally (all projects)                       │
│  [Ctrl+N]   Reject                                              │
│  [?]        Explain what this command does                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

For file edits, the diff is shown inline:

```
┌─────────────────────────────────────────────────────────────────┐
│  OLLERO wants to edit: src/auth/jwt.rs                           │
│                                                                 │
│  ┌─ Diff ────────────────────────────────────────────────┐     │
│  │  @@ -45,1 +45,1 @@                                    │     │
│  │  - let expiry = Utc::now();                            │     │
│  │  + let expiry = Utc::now() + Duration::hours(24);      │     │
│  └────────────────────────────────────────────────────────┘     │
│                                                                 │
│  [Enter]    Allow this once                                     │
│  [Ctrl+S]   Allow edits to src/** this session                  │
│  [Ctrl+W]   Allow edits to src/** in this workspace             │
│  [Ctrl+N]   Reject                                              │
│  [d]        Show full diff                                      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 4.5 Permission Resolution Pipeline

```rust
struct PermissionGuard {
    /// Rules compiled into the binary — NEVER skippable
    hardcoded_rules: Vec<HardcodedRule>,
    /// Global grants (~/.config/ollero/permissions.json)
    global_grants: PermissionStore,
    /// Workspace grants (.ollero/permissions.json)
    workspace_grants: PermissionStore,
    /// Session grants (in memory, cleared on exit)
    session_grants: HashSet<PermissionKey>,
    /// One-time grants (cleared after use)
    once_grants: HashSet<PermissionKey>,
}

impl PermissionGuard {
    fn evaluate(&self, action: &PermissionAction) -> PermissionDecision {
        let key = action.to_permission_key();

        // Step 1: Hardcoded rules (safety, non-negotiable)
        match self.check_hardcoded(&key, action) {
            HardcodedResult::Deny(reason) => return PermissionDecision::Deny { reason },
            HardcodedResult::AlwaysAsk => {
                return PermissionDecision::Ask {
                    reason: "Action flagged as always-confirm for safety".into(),
                    allow_session_grant: false,
                    allow_workspace_grant: false,
                };
            }
            HardcodedResult::Continue => {}
        }

        // Step 2: One-time grants
        if self.once_grants.remove(&key) {
            return PermissionDecision::Allow;
        }

        // Step 3: Session grants (in memory)
        if self.session_grants.contains(&key) {
            return PermissionDecision::Allow;
        }

        // Step 4: Workspace grants (disk)
        if self.workspace_grants.matches(&key) {
            return PermissionDecision::Allow;
        }

        // Step 5: Global grants (disk)
        if self.global_grants.matches(&key) {
            return PermissionDecision::Allow;
        }

        // Step 6: No grant found → ask user
        PermissionDecision::Ask {
            reason: format!("No prior permission for: {}", key),
            allow_session_grant: true,
            allow_workspace_grant: true,
        }
    }

    fn grant(&mut self, key: PermissionKey, scope: PermissionScope) {
        match scope {
            PermissionScope::Once => { self.once_grants.insert(key); }
            PermissionScope::Session => { self.session_grants.insert(key); }
            PermissionScope::Workspace => {
                self.workspace_grants.add(key);
                self.workspace_grants.save_to_disk();
            }
            PermissionScope::Global => {
                self.global_grants.add(key);
                self.global_grants.save_to_disk();
            }
        }
    }

    fn revoke(&mut self, key: &PermissionKey, scope: PermissionScope) {
        match scope {
            PermissionScope::Session => { self.session_grants.remove(key); }
            PermissionScope::Workspace => {
                self.workspace_grants.remove(key);
                self.workspace_grants.save_to_disk();
            }
            PermissionScope::Global => {
                self.global_grants.remove(key);
                self.global_grants.save_to_disk();
            }
            _ => {}
        }
    }
}
```

### 4.6 Permission Key Generation

```rust
impl PermissionAction {
    fn to_permission_key(&self) -> PermissionKey {
        match self {
            // Bash: generalize to command prefix
            // "cargo test --lib" → "cargo test *"
            PermissionAction::ExecuteCommand { command, .. } => {
                PermissionKey { tool: "bash".into(), pattern: generalize_command(command) }
            }
            // Edit: generalize to parent directory
            // "src/auth/jwt.rs" → "src/auth/**"
            PermissionAction::WriteFile { path } |
            PermissionAction::CreateFile { path } => {
                let dir = path.parent().unwrap_or(Path::new("."));
                PermissionKey { tool: "edit".into(), pattern: format!("{}/**", dir.display()) }
            }
            // Read: always generic (usually auto-allowed)
            PermissionAction::ReadFile { .. } => {
                PermissionKey { tool: "read".into(), pattern: "**".into() }
            }
            // Web fetch: generalize to domain
            PermissionAction::WebFetch { url } => {
                let domain = Url::parse(url)
                    .map(|u| u.host_str().unwrap_or("*").to_string())
                    .unwrap_or("*".into());
                PermissionKey { tool: "web_fetch".into(), pattern: domain }
            }
            // Web search: always generic
            PermissionAction::WebSearch { .. } => {
                PermissionKey { tool: "web_search".into(), pattern: "*".into() }
            }
            // Delete: always exact path (never generalize)
            PermissionAction::DeleteFile { path } => {
                PermissionKey { tool: "delete".into(), pattern: path.display().to_string() }
            }
        }
    }
}
```

### 4.7 Preset Permission Modes

```rust
enum PermissionMode {
    /// Ask everything. Nothing runs without confirmation.
    Paranoid,
    
    /// Default: reads free, writes and commands ask.
    /// Pre-approves: read_file, glob, grep, tree, git status/diff/log
    Balanced,
    
    /// Everything auto-approved EXCEPT hardcoded rules.
    /// rm -rf, force push, etc. ALWAYS ask regardless.
    Yolo,
}
```

### 4.8 Hardcoded Safety Rules

```rust
impl PermissionGuard {
    fn check_hardcoded(&self, key: &PermissionKey, action: &PermissionAction) -> HardcodedResult {
        match action {
            PermissionAction::ExecuteCommand { command, .. } => {
                let cmd = command.to_lowercase();
                
                // ABSOLUTE DENY: never execute
                if cmd.contains("| bash") || cmd.contains("| sh") ||
                   cmd.contains("| powershell") {
                    return HardcodedResult::Deny("Remote pipe to shell not allowed".into());
                }
                if cmd.starts_with("chmod 777") || cmd.starts_with("chown root") {
                    return HardcodedResult::Deny("System permission changes not allowed".into());
                }
                
                // ALWAYS ASK: too destructive to auto-approve
                let always_ask = ["rm -rf", "rm -r /", "rmdir /s",
                    "git push --force", "git push -f", "git reset --hard",
                    "git clean -f", "drop table", "drop database",
                    "format c:", "shutdown", "reboot", "del /f",
                    "mkfs.", "dd if="];
                for pat in &always_ask {
                    if cmd.contains(pat) { return HardcodedResult::AlwaysAsk; }
                }
                
                HardcodedResult::Continue
            }
            
            // Never write outside the project
            PermissionAction::WriteFile { path } |
            PermissionAction::CreateFile { path } |
            PermissionAction::DeleteFile { path } => {
                if !path.starts_with(&self.project_root) {
                    return HardcodedResult::Deny(format!(
                        "Cannot modify files outside project: {}", path.display()
                    ));
                }
                HardcodedResult::Continue
            }
            
            // Never read files with secrets
            PermissionAction::ReadFile { path } => {
                let path_str = path.display().to_string();
                let sensitive = [".env", "id_rsa", "id_ed25519", ".pem",
                    "credentials.json", ".aws/credentials", ".npmrc"];
                for pat in &sensitive {
                    if path_str.contains(pat) {
                        return HardcodedResult::Deny(format!(
                            "Sensitive file: {}. Use manual cat if you really need it.", path.display()
                        ));
                    }
                }
                HardcodedResult::Continue
            }
            _ => HardcodedResult::Continue,
        }
    }
}
```

### 4.9 Bash Sandbox

```rust
struct BashSandbox {
    working_dir: PathBuf,
    env_allowlist: HashSet<String>,
    max_timeout: Duration,
    max_output_bytes: usize,
}

impl BashSandbox {
    fn create_command(&self, cmd: &str) -> Command {
        let mut command = Command::new("bash");
        command.arg("-c").arg(cmd);
        command.current_dir(&self.working_dir);
        
        // Clean environment, only pass what's necessary
        command.env_clear();
        command.env("PATH", &self.safe_path());
        command.env("HOME", dirs::home_dir().unwrap());
        command.env("TERM", "xterm-256color");
        command.env("LANG", "en_US.UTF-8");
        
        command
    }
}
```

---

## 5. Ollama Integration

### 5.1 HTTP Client

```rust
struct OllamaClient {
    base_url: String,       // default: http://localhost:11434
    model: String,
    client: reqwest::Client,
    options: OllamaOptions,
}

struct OllamaOptions {
    temperature: f32,       // 0.1 for code, 0.7 for conversation
    num_ctx: u32,           // Context window (4096, 8192, 32768, etc.)
    top_p: f32,
    repeat_penalty: f32,
    num_predict: i32,       // -1 for unlimited
}

impl OllamaClient {
    /// Send a chat request with streaming
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<impl Stream<Item = Result<ChatChunk>>> {
        let body = json!({
            "model": self.model,
            "messages": messages,
            "tools": tools,
            "stream": true,
            "options": {
                "temperature": self.options.temperature,
                "num_ctx": self.options.num_ctx,
            }
        });
        
        let response = self.client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await?;
        
        Ok(response.bytes_stream().map(parse_json_line))
    }
    
    /// Verify Ollama is running and model is available
    async fn health_check(&self) -> Result<ModelInfo> {
        let resp = self.client
            .get(format!("{}/api/show", self.base_url))
            .json(&json!({"name": self.model}))
            .send()
            .await?;
        
        if !resp.status().is_success() {
            bail!("Model '{}' not found. Run: ollama pull {}", self.model, self.model);
        }
        
        Ok(resp.json().await?)
    }
    
    /// List available models
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let resp = self.client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?;
        Ok(resp.json::<ModelsResponse>().await?.models)
    }
}
```

### 5.2 Message Types

```rust
#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,               // "system", "user", "assistant", "tool"
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,          // always "function"
    function: FunctionCall,
}
```

### 5.3 Recommended Models

| Model | Params | Context | Best for | VRAM |
|---|---|---|---|---|
| `qwen2.5-coder:7b` | 7B | 32K | Fast tool-use, code | 6GB |
| `qwen2.5-coder:14b` | 14B | 32K | **Recommended general** | 12GB |
| `qwen2.5-coder:32b` | 32B | 32K | Best code quality | 24GB |
| `deepseek-coder-v2:16b` | 16B | 128K | Long context | 12GB |
| `llama3.1:8b` | 8B | 128K | General conversation | 6GB |
| `codestral:22b` | 22B | 32K | Premium code | 16GB |

---

## 6. Web Search & Fetch

### 6.1 Search Providers

```rust
/// Pluggable search backend
#[async_trait]
trait WebSearchProvider: Send + Sync {
    async fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchResult>>;
}

// Option A: SearXNG (local instance, best for privacy)
struct SearxngSearch { instance_url: String }

// Option B: DuckDuckGo (no API key needed)
struct DdgSearch;

// Option C: Tavily / Brave Search (needs API key, best results)
struct TavilySearch { api_key: String }
```

### 6.2 Web Fetch with Readability

```rust
struct WebFetcher {
    client: reqwest::Client,
    max_size: usize, // 1MB default
}

impl WebFetcher {
    async fn fetch_readable(&self, url: &str) -> Result<String> {
        let resp = self.client.get(url).send().await?;
        let content_type = resp.headers().get("content-type")...;
        
        if content_type.contains("text/html") {
            let html = resp.text().await?;
            let readable = extract_readable_content(&html); // like Readability.js
            html_to_markdown(&readable)
        } else if content_type.contains("text/plain") || content_type.contains("json") {
            Ok(resp.text().await?)
        } else {
            bail!("Unsupported content type: {}", content_type)
        }
    }
}
```

---

## 7. Terminal UI (TUI)

### 7.1 Main Layout

```
┌─ OLLERO v0.1.0 ─── qwen2.5-coder:14b ─── ~/projects/my-app ──────┐
│                                                                    │
│  You: There's a bug in the auth function, tokens expire            │
│       immediately after creation                                   │
│                                                                    │
│  OLLERO: Let me investigate. First I'll look for auth-related files │
│                                                                    │
│  ┌─ grep "token.*expir" ────────────────────── 0.3s ──────────┐   │
│  │ src/auth/jwt.rs:45:  let expiry = Utc::now();              │   │
│  │ src/auth/jwt.rs:46:  // TODO: add duration                 │   │
│  └────────────────────────────────────────────────────────────┘   │
│                                                                    │
│  OLLERO: Found the issue. In `src/auth/jwt.rs:45`, the expiry      │
│  is set to `Utc::now()` without adding any duration.               │
│                                                                    │
│  ┌─ edit_file src/auth/jwt.rs ────────────────────────────────┐   │
│  │  @@ -45,1 +45,1 @@                                        │   │
│  │  - let expiry = Utc::now();                                │   │
│  │  + let expiry = Utc::now() + Duration::hours(24);          │   │
│  └────────────────────────────────────────────────────────────┘   │
│                                                                    │
├────────────────────────────────────────────────────────────────────┤
│ > _                                               tokens: 2.1K ▐  │
└────────────────────────────────────────────────────────────────────┘
```

### 7.2 TUI Architecture

Inspired by claude-code-rust's production-grade rendering pipeline:

```rust
struct OlleroApp {
    // === Message State ===
    messages: Vec<ChatMessage>,
    /// Index for O(1) tool call lookup by ID
    tool_call_index: HashMap<String, (usize, usize)>, // (msg_idx, block_idx)
    /// Streaming buffer for in-flight LLM response
    streaming_buffer: IncrementalMarkdown,
    
    // === Viewport ===
    viewport: ChatViewport,
    
    // === Render Cache ===
    block_cache: BlockCache,
    
    // === Input ===
    input: TextArea,
    
    // === State ===
    is_streaming: bool,
    pending_confirmation: Option<Confirmation>,
    current_model: String,
    project_dir: PathBuf,
    session_id: Uuid,
}
```

### 7.3 Virtual Scrolling (from claude-code-rust)

```rust
/// Viewport with scroll anchor preservation across resize
struct ChatViewport {
    /// Scroll position in wrapped lines from top
    scroll_offset: usize,
    /// Terminal height available for chat
    visible_height: usize,
    /// Cached wrapped-line heights per message block
    height_cache: HashMap<(usize, u16), usize>, // (msg_idx, terminal_width) → height
    /// Auto-scroll when new content arrives
    auto_scroll: bool,
}

impl ChatViewport {
    /// Progressive remeasure: only recompute visible + buffer messages per frame
    fn remeasure_budget(&self) -> usize {
        std::cmp::max(12, self.visible_height)
    }
    
    /// Skip rendering messages that are completely offscreen
    fn should_render(&self, msg_idx: usize) -> bool {
        // Culling: only render messages within viewport + margin
        let msg_top = self.message_top_offset(msg_idx);
        let msg_bottom = msg_top + self.message_height(msg_idx);
        msg_bottom > self.scroll_offset && msg_top < self.scroll_offset + self.visible_height
    }
}
```

### 7.4 Render Cache with Eviction (from claude-code-rust)

```rust
/// Per-message block cache with eviction budgeting
struct BlockCache {
    /// Rendered content per (message_idx, terminal_width)
    entries: HashMap<CacheKey, CachedBlock>,
    /// Total budget in bytes for cached blocks
    total_budget: usize,
    /// Currently used bytes
    used_bytes: usize,
    /// Set of evictable entries (sorted by last access)
    evictable: BTreeSet<(Instant, CacheKey)>,
    /// Protected entries (in-flight messages, never evicted)
    protected: HashSet<CacheKey>,
}

impl BlockCache {
    fn get_or_render(&mut self, key: CacheKey, render_fn: impl FnOnce() -> CachedBlock) -> &CachedBlock {
        if !self.entries.contains_key(&key) {
            let block = render_fn();
            self.used_bytes += block.size_bytes();
            self.evict_if_over_budget();
            self.entries.insert(key.clone(), block);
        }
        &self.entries[&key]
    }
}
```

### 7.5 Markdown Rendering in Terminal

```rust
/// Render markdown to terminal spans using pulldown-cmark + syntect
struct MarkdownRenderer {
    /// Syntax highlighting theme
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl MarkdownRenderer {
    fn render(&self, markdown: &str, width: u16) -> Vec<Line<'_>> {
        // Parse markdown with pulldown-cmark
        // For code blocks: apply syntax highlighting via syntect
        // For inline code: dim background
        // For headers: bold
        // For lists: bullet points with indentation
        // For links: blue underline
        // Word-wrap to terminal width
    }
}
```

### 7.6 Incremental Markdown Streaming

```rust
/// Buffer that accumulates streamed markdown chunks
/// and only re-renders the changed portion
struct IncrementalMarkdown {
    /// Full accumulated text
    text: String,
    /// Number of bytes already rendered
    rendered_up_to: usize,
    /// Cached rendered lines
    cached_lines: Vec<Line<'static>>,
}

impl IncrementalMarkdown {
    fn push_chunk(&mut self, chunk: &str) {
        self.text.push_str(chunk);
        // Only re-render from the last paragraph break
        // to avoid full re-parse on every token
    }
}
```

### 7.7 Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Enter` | Send message |
| `Shift+Enter` | New line in input |
| `Ctrl+C` | Cancel streaming / Exit |
| `Ctrl+L` | Clear screen |
| `Esc` | Cancel pending confirmation |
| `Tab` | Autocomplete file paths |
| `Ctrl+Y` | Approve pending action |
| `Ctrl+N` | Reject pending action |
| `Ctrl+S` | Approve for session |
| `Ctrl+W` | Approve for workspace |
| `Page Up/Down` | Scroll through history |
| `Home/End` | Jump to top/bottom |
| `Mouse scroll` | Scroll viewport |
| `Click` | Select text |
| `Ctrl+C` (on selection) | Copy to clipboard |

### 7.8 Mouse Support

```rust
// Inspired by claude-code-rust: full mouse integration
struct MouseState {
    /// Text selection start/end
    selection: Option<(Position, Position)>,
    /// Scrollbar drag state
    scrollbar_drag: Option<ScrollbarDrag>,
}

// Features:
// - Click to place cursor in input
// - Click on tool call to expand/collapse
// - Drag scrollbar thumb
// - Select text for clipboard copy (via arboard crate)
// - Mouse wheel for smooth scrolling
```

---

## 8. Session Management

**NEW SECTION** — Inspired by claude-code-rust's session persistence.

### 8.1 Session Persistence

Sessions are saved to disk so conversations can be resumed across OLLERO restarts:

```rust
struct Session {
    id: Uuid,
    /// AI-generated title summarizing the conversation
    title: String,
    /// User can rename
    custom_title: Option<String>,
    /// When the session was created
    created_at: DateTime<Utc>,
    /// Last activity timestamp
    updated_at: DateTime<Utc>,
    /// Git branch when session started
    git_branch: Option<String>,
    /// Working directory
    cwd: PathBuf,
    /// Model used
    model: String,
    /// Full conversation history
    messages: Vec<ChatMessage>,
    /// Token usage stats
    usage: SessionUsage,
}

struct SessionManager {
    /// Directory where sessions are stored
    sessions_dir: PathBuf, // ~/.config/ollero/sessions/
}

impl SessionManager {
    /// Save current session to disk
    async fn save(&self, session: &Session) -> Result<()> {
        let path = self.sessions_dir.join(format!("{}.json", session.id));
        let json = serde_json::to_string_pretty(session)?;
        tokio::fs::write(&path, json).await?;
        Ok(())
    }
    
    /// Resume a previous session
    async fn resume(&self, session_id: &Uuid) -> Result<Session> {
        let path = self.sessions_dir.join(format!("{}.json", session_id));
        let json = tokio::fs::read_to_string(&path).await?;
        Ok(serde_json::from_str(&json)?)
    }
    
    /// List recent sessions
    async fn list_recent(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        // Read all session files, sort by updated_at desc, return summaries
    }
    
    /// Auto-generate title from conversation content
    async fn generate_title(&self, session: &Session, ollama: &OllamaClient) -> Result<String> {
        // Ask the LLM to summarize the conversation in 5-10 words
        let prompt = format!(
            "Summarize this conversation in 5-10 words as a title:\n{}",
            session.first_exchange_summary()
        );
        ollama.generate_short(&prompt).await
    }
}
```

### 8.2 Session Commands

```
ollero                        → Start new session
ollero --resume <session_id>  → Resume a specific session
ollero --last                 → Resume most recent session

/sessions                    → List recent sessions
/resume <id>                 → Resume a session mid-conversation
/rename "New Title"          → Rename current session
/session-info                → Show current session details
```

### 8.3 Auto-Save

```rust
impl OlleroApp {
    /// Save session after each complete turn (user + assistant)
    async fn auto_save_session(&self) {
        if let Err(e) = self.session_manager.save(&self.session).await {
            tracing::warn!("Failed to auto-save session: {}", e);
        }
    }
}
```

---

## 9. Diff Rendering & Undo System

**NEW SECTION** — Inspired by claude-code-rust's unified diff rendering.

### 9.1 Unified Diff Rendering

Using the `similar` crate for computing diffs and rendering them with syntax highlighting:

```rust
use similar::{ChangeTag, TextDiff};

struct DiffRenderer {
    /// Number of context lines around changes
    context_lines: usize, // default: 3
    syntax_set: SyntaxSet,
}

impl DiffRenderer {
    fn render_diff(&self, old: &str, new: &str, path: &str) -> Vec<Line<'static>> {
        let diff = TextDiff::from_lines(old, new);
        let mut lines = vec![];
        
        // Header
        lines.push(Line::styled(format!("--- a/{}", path), Style::default().dim()));
        lines.push(Line::styled(format!("+++ b/{}", path), Style::default().dim()));
        
        for hunk in diff.unified_diff().context_radius(self.context_lines).iter_hunks() {
            // Hunk header: @@ -old_start,old_count +new_start,new_count @@
            lines.push(Line::styled(
                format!("{}", hunk.header()),
                Style::default().fg(Color::Cyan)
            ));
            
            for change in hunk.iter_changes() {
                let (prefix, style) = match change.tag() {
                    ChangeTag::Delete => ("-", Style::default().fg(Color::Red)),
                    ChangeTag::Insert => ("+", Style::default().fg(Color::Green)),
                    ChangeTag::Equal  => (" ", Style::default().dim()),
                };
                lines.push(Line::styled(
                    format!("{}{}", prefix, change.value().trim_end()),
                    style
                ));
            }
        }
        
        lines
    }
}
```

### 9.2 Undo System

```rust
struct UndoStack {
    /// Stack of reversible file edits
    entries: Vec<UndoEntry>,
    /// Maximum entries to keep
    max_size: usize, // default: 50
}

struct UndoEntry {
    path: PathBuf,
    /// Content BEFORE the edit
    old_content: String,
    /// Content AFTER the edit (for redo)
    new_content: String,
    /// When the edit was made
    timestamp: DateTime<Utc>,
    /// Which tool call produced this edit
    tool_call_id: String,
}

impl UndoStack {
    /// Undo the most recent edit
    async fn undo(&mut self) -> Result<UndoResult> {
        let entry = self.entries.pop().ok_or(anyhow!("Nothing to undo"))?;
        
        // Verify the file still has the expected content
        let current = tokio::fs::read_to_string(&entry.path).await?;
        if current != entry.new_content {
            return Err(anyhow!(
                "File {} has been modified since the edit. Cannot undo safely.",
                entry.path.display()
            ));
        }
        
        // Restore old content
        tokio::fs::write(&entry.path, &entry.old_content).await?;
        
        Ok(UndoResult {
            path: entry.path,
            diff: compute_diff(&entry.new_content, &entry.old_content),
        })
    }
    
    /// Undo all edits from a specific tool call
    async fn undo_tool_call(&mut self, tool_call_id: &str) -> Result<Vec<UndoResult>> {
        // Find and undo all entries with matching tool_call_id
    }
}
```

---

## 10. Slash Commands

**NEW SECTION** — Comprehensive slash command system.

```rust
enum SlashCommand {
    // === Session ===
    Clear,                          // /clear — New conversation (keep session)
    Sessions,                       // /sessions — List recent sessions
    Resume(String),                 // /resume <id> — Resume a session
    Rename(String),                 // /rename "title" — Rename current session
    SessionInfo,                    // /session-info — Show session details

    // === Model ===
    Model(String),                  // /model <name> — Switch model
    Models,                         // /models — List available models

    // === Permissions ===
    Permissions(Option<String>),    // /permissions [scope] — Show grants
    Allow(PermissionGrant),         // /allow bash "cargo *" session
    Revoke(PermissionKey),          // /revoke bash "cargo *"
    Mode(PermissionMode),           // /mode balanced

    // === Context ===
    Context,                        // /context — Show what's in context
    Tokens,                         // /tokens — Show token usage
    
    // === Tools ===
    Undo,                           // /undo — Undo last file edit
    UndoAll,                        // /undo-all — Undo all edits in session
    
    // === Config ===
    Config,                         // /config — Open config TUI tab
    Usage,                          // /usage — Show token usage stats
    
    // === Help ===
    Help,                           // /help — Show available commands
}

impl OlleroApp {
    fn handle_slash_command(&mut self, cmd: SlashCommand) -> Result<()> {
        match cmd {
            SlashCommand::Model(name) => {
                self.ollama.model = name.clone();
                self.show_system_message(format!("Switched to model: {}", name));
            }
            SlashCommand::Models => {
                let models = self.ollama.list_models().await?;
                self.show_model_list(models);
            }
            SlashCommand::Undo => {
                let result = self.undo_stack.undo().await?;
                self.show_undo_result(result);
            }
            SlashCommand::Context => {
                self.show_context_summary();
            }
            SlashCommand::Tokens => {
                self.show_token_usage();
            }
            // ...
        }
    }
}
```

---

## 11. Configuration System

**NEW SECTION** — Multi-layered config inspired by claude-code-rust.

### 11.1 Config Sources (priority order)

```
1. CLI arguments         (--model, --mode, etc.)
2. Environment variables (OLLERO_MODEL, OLLERO_OLLAMA_URL, etc.)
3. Project config        (.ollero.toml in project root)
4. Global config         (~/.config/ollero/config.toml)
5. Built-in defaults
```

### 11.2 Global Config

```toml
# ~/.config/ollero/config.toml

[general]
# Default language for responses
language = "en"  # "en", "es", "pt", etc.
# Auto-save sessions
auto_save = true
# Max sessions to keep on disk
max_sessions = 100

[ollama]
url = "http://localhost:11434"
default_model = "qwen2.5-coder:14b"
temperature = 0.1
num_ctx = 8192

[tui]
# Theme
theme = "dark"  # "dark", "light"
# Show token count in status bar
show_tokens = true
# Auto-scroll on new content
auto_scroll = true
# Diff context lines
diff_context_lines = 3
# Mouse support
mouse_enabled = true

[web_search]
# Search provider: "duckduckgo", "searxng", "tavily", "brave"
provider = "duckduckgo"
# For searxng:
# searxng_url = "http://localhost:8080"
# For tavily/brave:
# api_key = "..."

[permissions]
mode = "balanced"
```

### 11.3 Config TUI Tab

Accessible via `/config` — inspired by claude-code-rust's multi-tab config:

```
┌─ Config ────────────────────────────────────────────────────────┐
│  [Settings]  [Permissions]  [Usage]  [MCP]  [Session]           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Model:     qwen2.5-coder:14b  [change]                       │
│  Context:   8192 tokens                                        │
│  Mode:      balanced                                           │
│  Provider:  duckduckgo                                         │
│                                                                 │
│  Language:  en                                                  │
│  Theme:     dark                                                │
│  Mouse:     enabled                                             │
│                                                                 │
│  [Save]  [Reset to defaults]                                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 12. Token Tracking & Cost Awareness

**NEW SECTION** — Even with local models, tracking token usage is important for context management.

```rust
struct TokenTracker {
    /// Per-message usage
    message_usage: Vec<MessageUsage>,
    /// Session totals
    session_total: SessionUsage,
}

struct MessageUsage {
    input_tokens: usize,
    output_tokens: usize,
    /// Ollama reports eval_count and eval_duration
    eval_duration_ms: u64,
    /// Tokens per second (from Ollama response)
    tokens_per_second: f32,
}

struct SessionUsage {
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_messages: u64,
    total_tool_calls: u64,
    /// Total LLM inference time
    total_eval_time: Duration,
    /// Average tokens/second
    avg_tokens_per_second: f32,
    session_started: DateTime<Utc>,
}

impl TokenTracker {
    /// Extract usage from Ollama response metadata
    fn record_from_ollama_response(&mut self, response: &OllamaFinalResponse) {
        // Ollama includes in final chunk:
        // "eval_count": 123,
        // "eval_duration": 456000000,  (nanoseconds)
        // "prompt_eval_count": 789,
        // "prompt_eval_duration": 101000000
        
        let usage = MessageUsage {
            input_tokens: response.prompt_eval_count.unwrap_or(0),
            output_tokens: response.eval_count.unwrap_or(0),
            eval_duration_ms: response.eval_duration.unwrap_or(0) / 1_000_000,
            tokens_per_second: response.eval_count.unwrap_or(0) as f32
                / (response.eval_duration.unwrap_or(1) as f32 / 1e9),
        };
        
        self.session_total.total_input_tokens += usage.input_tokens as u64;
        self.session_total.total_output_tokens += usage.output_tokens as u64;
        self.message_usage.push(usage);
    }
}
```

Displayed in the status bar and via `/usage`:

```
┌─ Token Usage ───────────────────────────────────────────────────┐
│                                                                 │
│  Session: 2.1K in / 1.8K out (3.9K total)                     │
│  Tool calls: 12                                                │
│  Avg speed: 45.2 tok/s                                         │
│  Inference time: 42.3s total                                   │
│  Context budget: 3.9K / 8.0K (48% used)                       │
│                                                                 │
│  ████████████████░░░░░░░░░░░░░░░░░░ 48%                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 13. MCP Protocol Support

**NEW SECTION** — Model Context Protocol for extensibility with external tools.

### 13.1 What is MCP?

MCP (Model Context Protocol) allows OLLERO to connect to external tool servers. This means users can extend OLLERO's capabilities without modifying its code.

Examples:
- A database MCP server that exposes query tools
- A Jira/Linear MCP server for ticket management
- A custom internal API exposed as tools
- A code analysis MCP server (linting, type checking)

### 13.2 MCP Configuration

```toml
# .ollero.toml or ~/.config/ollero/config.toml

[[mcp.servers]]
name = "database"
type = "stdio"  # "stdio", "sse", "http"
command = "npx"
args = ["-y", "@mcp/sqlite-server", "./dev.db"]
enabled = true

[[mcp.servers]]
name = "github"
type = "stdio"
command = "npx"
args = ["-y", "@mcp/github-server"]
env = { GITHUB_TOKEN = "..." }
enabled = true

[[mcp.servers]]
name = "custom-api"
type = "http"
url = "http://localhost:3001/mcp"
enabled = false
```

### 13.3 MCP Client

```rust
struct McpManager {
    servers: Vec<McpServer>,
}

struct McpServer {
    name: String,
    config: McpServerConfig,
    status: McpStatus,
    /// Tools exposed by this server
    tools: Vec<McpTool>,
    /// Process handle (for stdio servers)
    process: Option<Child>,
}

enum McpStatus {
    Disconnected,
    Connecting,
    Connected { tool_count: usize },
    Error(String),
}

struct McpTool {
    name: String,
    description: String,
    parameters: serde_json::Value,
    /// Tool annotations from MCP spec
    annotations: McpToolAnnotations,
}

struct McpToolAnnotations {
    /// Tool only reads data, doesn't modify anything
    read_only: bool,
    /// Tool may have destructive side effects
    destructive: bool,
    /// Tool accesses external/open-world resources
    open_world: bool,
}

impl McpManager {
    /// Connect to all enabled MCP servers
    async fn connect_all(&mut self) -> Result<()> {
        for server in &mut self.servers {
            if server.config.enabled {
                server.connect().await?;
                server.discover_tools().await?;
            }
        }
        Ok(())
    }
    
    /// Merge MCP tools into the main tool list for Ollama
    fn all_tools(&self) -> Vec<ToolDefinition> {
        self.servers.iter()
            .filter(|s| matches!(s.status, McpStatus::Connected { .. }))
            .flat_map(|s| s.tools.iter().map(|t| t.to_tool_definition()))
            .collect()
    }
    
    /// Route a tool call to the correct MCP server
    async fn execute(&self, tool_name: &str, params: Value) -> Result<ToolResult> {
        for server in &self.servers {
            if let Some(tool) = server.tools.iter().find(|t| t.name == tool_name) {
                return server.call_tool(tool_name, params).await;
            }
        }
        bail!("MCP tool not found: {}", tool_name)
    }
}
```

### 13.4 MCP in the Permission System

MCP tools integrate with the permission guard using their annotations:

```rust
impl McpTool {
    fn risk_level(&self) -> RiskLevel {
        if self.annotations.destructive {
            RiskLevel::Critical
        } else if self.annotations.open_world {
            RiskLevel::Dangerous
        } else if self.annotations.read_only {
            RiskLevel::Safe
        } else {
            RiskLevel::Moderate
        }
    }
}
```

### 13.5 MCP Config Tab

```
┌─ Config > MCP ──────────────────────────────────────────────────┐
│                                                                 │
│  MCP Servers:                                                   │
│                                                                 │
│  ● database       connected    3 tools    [disable] [auth]     │
│  ● github         connected    8 tools    [disable]            │
│  ○ custom-api     disabled                [enable]             │
│  ✗ jira           error: connection refused  [retry]           │
│                                                                 │
│  [+ Add Server]                                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 14. Project Structure (Rust)

```
ollero/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── .ollero.toml.example
│
├── src/
│   ├── main.rs                  # Entry point, CLI args (clap)
│   ├── app.rs                   # OlleroApp: main loop, state machine
│   │
│   ├── config/
│   │   ├── mod.rs
│   │   ├── ollero_config.rs      # .ollero.toml parsing
│   │   └── global_config.rs     # ~/.config/ollero/config.toml
│   │
│   ├── ollama/
│   │   ├── mod.rs
│   │   ├── client.rs            # HTTP client for Ollama API
│   │   ├── types.rs             # ChatMessage, ToolCall, etc.
│   │   └── stream.rs            # Streaming response parser
│   │
│   ├── context/
│   │   ├── mod.rs
│   │   ├── project_map.rs       # Project detection and mapping
│   │   ├── budget.rs            # ContextBudget, token counting
│   │   ├── resolver.rs          # Decide what to include
│   │   └── compressor.rs        # History compression/eviction
│   │
│   ├── tools/
│   │   ├── mod.rs               # Tool trait, ToolDispatcher
│   │   ├── read_file.rs
│   │   ├── write_file.rs
│   │   ├── edit_file.rs
│   │   ├── glob.rs
│   │   ├── grep.rs
│   │   ├── tree.rs
│   │   ├── bash.rs
│   │   ├── web_search.rs
│   │   ├── web_fetch.rs
│   │   ├── ask_user.rs
│   │   └── todo.rs
│   │
│   ├── permissions/
│   │   ├── mod.rs
│   │   ├── guard.rs             # PermissionGuard (4-scope system)
│   │   ├── rules.rs             # Hardcoded safety rules
│   │   ├── store.rs             # Disk persistence for grants
│   │   └── modes.rs             # Paranoid/Balanced/Yolo presets
│   │
│   ├── session/
│   │   ├── mod.rs
│   │   ├── manager.rs           # SessionManager (save/resume/list)
│   │   ├── types.rs             # Session, SessionSummary
│   │   └── auto_title.rs        # AI-generated session titles
│   │
│   ├── mcp/
│   │   ├── mod.rs
│   │   ├── client.rs            # MCP client (stdio/sse/http)
│   │   ├── manager.rs           # McpManager (multi-server)
│   │   └── types.rs             # McpTool, McpAnnotations
│   │
│   ├── tui/
│   │   ├── mod.rs
│   │   ├── app.rs               # TUI state, event loop
│   │   ├── render.rs            # Main render function
│   │   ├── chat_view.rs         # Chat viewport with virtual scrolling
│   │   ├── diff.rs              # Unified diff rendering
│   │   ├── markdown.rs          # Markdown → terminal spans
│   │   ├── highlight.rs         # Syntax highlighting (syntect)
│   │   ├── input.rs             # Input field, paste handling
│   │   ├── confirmation.rs      # Permission confirmation dialogs
│   │   ├── tool_call.rs         # Tool call rendering (collapsible)
│   │   ├── config_view.rs       # Config tabs (settings, MCP, usage)
│   │   ├── scrollbar.rs         # Scrollbar with thumb drag
│   │   ├── selection.rs         # Text selection + clipboard
│   │   └── status_bar.rs        # Bottom bar (tokens, model, branch)
│   │
│   ├── slash/
│   │   ├── mod.rs
│   │   ├── parser.rs            # Parse /commands
│   │   └── handlers.rs          # Execute slash commands
│   │
│   └── utils/
│       ├── mod.rs
│       ├── tokenizer.rs         # Approximate token counting
│       ├── truncate.rs          # Smart string truncation
│       └── html.rs              # HTML → Markdown/plaintext
│
└── tests/
    ├── integration/
    │   ├── ollama_client_test.rs
    │   ├── tools_test.rs
    │   ├── permissions_test.rs
    │   └── session_test.rs
    └── fixtures/
        └── sample_project/
```

---

## 15. Conversation Flow

### 15.1 Main Loop

```rust
async fn main_loop(app: &mut OlleroApp) -> Result<()> {
    loop {
        // 1. Wait for user input
        let user_input = app.tui.read_input().await?;
        
        // 2. Handle slash commands
        if let Some(cmd) = parse_slash_command(&user_input) {
            app.handle_slash_command(cmd).await?;
            continue;
        }
        
        // 3. Add user message to history
        app.conversation.push_user_message(&user_input);
        
        // 4. Resolve relevant context
        let context = app.context_manager.resolve_context(&user_input, &app.budget);
        
        // 5. Build messages for Ollama
        let messages = app.build_messages(&context);
        
        // 6. Merge MCP tools with built-in tools
        let all_tools = app.merge_tool_schemas();
        
        // 7. Tool-use loop (LLM may invoke multiple tools)
        loop {
            let mut stream = app.ollama.chat_stream(&messages, &all_tools).await?;
            let response = app.tui.stream_response(&mut stream).await?;
            
            // Track token usage from Ollama response
            app.token_tracker.record_from_ollama_response(&response);
            
            if let Some(tool_calls) = &response.tool_calls {
                for call in tool_calls {
                    // Check permissions
                    let action = call.to_permission_action();
                    match app.permissions.evaluate(&action) {
                        PermissionDecision::Allow => {}
                        PermissionDecision::Ask { .. } => {
                            let user_choice = app.tui.show_confirmation(&call).await?;
                            match user_choice {
                                UserChoice::AllowOnce => {}
                                UserChoice::AllowSession => {
                                    app.permissions.grant(action.to_key(), PermissionScope::Session);
                                }
                                UserChoice::AllowWorkspace => {
                                    app.permissions.grant(action.to_key(), PermissionScope::Workspace);
                                }
                                UserChoice::AllowGlobal => {
                                    app.permissions.grant(action.to_key(), PermissionScope::Global);
                                }
                                UserChoice::Reject => {
                                    messages.push(tool_result(call.id, "Rejected by user"));
                                    continue;
                                }
                            }
                        }
                        PermissionDecision::Deny { reason } => {
                            messages.push(tool_result(call.id, format!("Denied: {}", reason)));
                            continue;
                        }
                    }
                    
                    // Execute (route to MCP or built-in)
                    let result = if app.mcp.has_tool(&call.function.name) {
                        app.mcp.execute(&call.function.name, call.function.arguments.clone()).await?
                    } else {
                        app.tool_dispatcher.execute(&call).await?
                    };
                    
                    app.tui.show_tool_result(&call, &result);
                    messages.push(tool_result(call.id, &result));
                }
                continue; // Back to LLM with results
            }
            
            // No tool_calls → LLM is done
            app.conversation.push_assistant_message(&response.content);
            break;
        }
        
        // 8. Compress history if needed
        app.conversation.compress_if_needed(&app.budget).await?;
        
        // 9. Auto-save session
        app.auto_save_session().await;
    }
}
```

### 15.2 Sequence Diagram

```
User           OLLERO             Ollama          Tool/MCP
  │              │                  │               │
  │── message ──▶│                  │               │
  │              │── build ctx ────▶│               │
  │              │── chat(stream) ─▶│               │
  │◀── tokens ───│◀── streaming ───│               │
  │              │◀── tool_call ───│               │
  │              │                  │               │
  │◀─ confirm? ──│                  │               │
  │── [Ctrl+S] ─▶│  (grant session) │               │
  │              │                  │── execute ───▶│
  │              │                  │◀── result ───│
  │              │── chat(+result) ▶│               │
  │              │◀── tool_call ───│               │
  │              │                  │── execute ───▶│
  │              │                  │◀── result ───│
  │              │── chat(+result) ▶│               │
  │◀── tokens ───│◀── streaming ───│               │
  │              │── auto-save ────▶│               │
  │              │── track tokens ─▶│               │
```

---

## 16. Key Dependencies (Crates)

| Crate | Why | Alternative |
|---|---|---|
| `tokio` | Standard async runtime | `async-std` |
| `reqwest` | HTTP client with streaming | `hyper` (lower level) |
| `ratatui` 0.30 | TUI framework (same as claude-code-rust) | `cursive` |
| `crossterm` | Cross-platform terminal backend | `termion` (Unix only) |
| `clap` | CLI args with derive macros | `argh` |
| `serde` + `serde_json` | Serialization | — |
| `toml` | Config file parsing | — |
| `ignore` | File walking respecting .gitignore | `glob` (no gitignore) |
| `similar` | Diff computation (same as claude-code-rust) | `diffy` |
| `pulldown-cmark` | Markdown parsing | `comrak` |
| `syntect` | Syntax highlighting for code blocks and diffs | `tree-sitter` |
| `tui-textarea` | Multiline input widget | Manual implementation |
| `arboard` | Clipboard copy (text selection) | `copypasta` |
| `notify-rust` | Desktop notifications (optional) | — |
| `uuid` | Session IDs | `ulid` |
| `chrono` | Timestamps | `time` |
| `dirs` | System directories (home, config) | `directories` |
| `anyhow` + `thiserror` | Error handling | `eyre` |
| `tracing` | Structured logging/diagnostics | `log` |
| `scraper` | HTML parsing for web_fetch | `select.rs` |
| `tiktoken-rs` | Token counting | Heuristic (chars/4) |

### Cargo.toml

```toml
[package]
name = "ollero"
version = "0.1.0"
edition = "2021"
description = "Local code agent powered by Ollama"
license = "MIT"

[dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = { version = "4", features = ["derive"] }
ratatui = "0.30"
crossterm = "0.28"
tui-textarea = "0.7"
toml = "0.8"
ignore = "0.4"
similar = "2.7"
pulldown-cmark = "0.13"
syntect = "5.3"
arboard = "3.6"
notify-rust = "4.12"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
dirs = "6"
anyhow = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
url = "2"
urlencoding = "2"
futures = "0.3"
async-trait = "0.1"
scraper = "0.21"
tiktoken-rs = "0.6"
unicode-width = "0.2"

[dev-dependencies]
tokio-test = "0.4"
tempfile = "3"
assert_cmd = "2"
wiremock = "0.6"
```

---

## 17. Implementation Phases

### Phase 1: Foundation (Week 1-2)
- [ ] Rust project setup with Cargo
- [ ] Basic Ollama client (chat without streaming)
- [ ] Simple REPL loop (stdin/stdout, no TUI)
- [ ] Basic system prompt
- [ ] Health check (verify Ollama is running, model available)
- [ ] CLI args with clap (--model, --ollama-url)

**Result:** Basic conversation with Ollama from terminal.

### Phase 2: Core Tools (Week 3-4)
- [ ] `Tool` trait and `ToolDispatcher`
- [ ] Implement `read_file`, `glob`, `grep`, `tree`
- [ ] Implement `edit_file`, `write_file`
- [ ] Implement `bash` (basic, no sandbox yet)
- [ ] Tool-use loop (LLM → tool → LLM → tool → ...)
- [ ] Undo stack for file edits

**Result:** Agent can read, search, and edit code.

### Phase 3: Permissions (Week 5)
- [ ] `PermissionGuard` with 4-scope system
- [ ] Hardcoded safety rules
- [ ] Terminal confirmations with scope selection
- [ ] Disk persistence for workspace/global grants
- [ ] Permission modes (paranoid/balanced/yolo)
- [ ] Bash sandbox (env cleaning, timeout, output limit)

**Result:** Safe execution with granular permission control.

### Phase 4: Context Intelligence (Week 6-7)
- [ ] `ProjectMap`: automatic project detection
- [ ] `ContextBudget`: token counting and limits
- [ ] Smart file inclusion based on user message
- [ ] History retention with soft/hard limits
- [ ] History compression via LLM summarization
- [ ] `.ollero.toml` configuration

**Result:** Efficient context management for limited windows.

### Phase 5: TUI (Week 8-10)
- [ ] Ratatui layout with header, chat, input, status bar
- [ ] Streaming response rendering
- [ ] Markdown rendering with syntax highlighting
- [ ] Diff rendering with unified format
- [ ] Confirmation dialogs with keyboard shortcuts
- [ ] Virtual scrolling with viewport culling
- [ ] Render cache with eviction budgeting
- [ ] Tool call rendering (collapsible)
- [ ] Mouse support (scroll, select, clipboard)
- [ ] Incremental markdown streaming

**Result:** Production-grade terminal interface.

### Phase 6: Sessions & Config (Week 11)
- [ ] Session save/resume/list
- [ ] Auto-generated session titles
- [ ] Token tracking from Ollama responses
- [ ] Config TUI tab (settings, usage)
- [ ] Slash command system
- [ ] Global config file

**Result:** Persistent sessions and full configurability.

### Phase 7: Internet & MCP (Week 12-13)
- [ ] `web_search` with DuckDuckGo/SearXNG
- [ ] `web_fetch` with HTML cleaning
- [ ] MCP client (stdio transport)
- [ ] MCP server management (connect, discover tools, route calls)
- [ ] MCP config tab
- [ ] MCP tool annotations → permission integration

**Result:** Web access and extensible tool ecosystem.

### Phase 8: Polish & Distribution (Week 14)
- [ ] Desktop notifications (notify-rust)
- [ ] File path autocomplete in input
- [ ] Structured logging with tracing
- [ ] Cross-platform binary builds (Linux, macOS, Windows)
- [ ] `cargo install` support
- [ ] README and documentation
- [ ] Integration tests
- [ ] CI/CD with GitHub Actions

**Result:** Polished, distributable tool.

---

## Appendix A: Token Counting

```rust
fn estimate_tokens(text: &str) -> usize {
    // Heuristic: ~4 chars per token in English, ~3 in code
    let chars = text.len();
    let words = text.split_whitespace().count();
    let by_chars = chars / 4;
    let by_words = (words as f32 * 1.3) as usize;
    (by_chars + by_words) / 2
}
```

For better accuracy, use `tiktoken-rs` with `cl100k_base` encoding as a reasonable approximation.

## Appendix B: Models with Best Tool-Use Support in Ollama

1. **Qwen 2.5 Coder** — Excellent tool-use, optimized for code
2. **Llama 3.1/3.2** — Good tool support, multilingual
3. **Mistral/Mixtral** — Solid function calling support
4. **Command R+** — Designed for tool-use and RAG
5. **DeepSeek Coder V2** — Good tool-use with long context

**Note:** Not all Ollama models support tool-use. Verify with `ollama show <model>`.

## Appendix C: Environment Variables

```bash
OLLERO_OLLAMA_URL=http://localhost:11434    # Ollama URL
OLLERO_MODEL=qwen2.5-coder:14b              # Default model
OLLERO_MAX_CONTEXT=8192                      # Max context tokens
OLLERO_LOG_LEVEL=info                        # Log level
OLLERO_CONFIG_DIR=~/.config/ollero            # Global config directory
```

## Appendix D: Differences from claude-code-rust

| Feature | claude-code-rust | OLLERO |
|---|---|---|
| **LLM Backend** | Anthropic API via TypeScript bridge | Ollama local HTTP API (no bridge) |
| **Architecture** | Rust TUI + Node.js bridge (stdio JSON) | Pure Rust (single binary, no Node.js) |
| **Cost** | API usage costs, quota tracking | Free (local inference) |
| **Privacy** | Data sent to Anthropic servers | 100% local, nothing leaves your machine |
| **Models** | Claude only | Any Ollama model (Qwen, Llama, Mistral, etc.) |
| **Context window** | 200K tokens | 4K-128K depending on model |
| **Session storage** | SDK-managed | Direct JSON files |
| **MCP** | Full support via SDK | Direct implementation in Rust |
| **Plugins** | Claude CLI marketplace | Not planned (MCP covers extensibility) |
| **Undo** | Not implemented | Built-in undo stack |
| **Token tracking** | API billing + quota gauges | Local inference stats (tok/s, eval time) |
