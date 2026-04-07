---
layout: default
title: Tool System
parent: Architecture
nav_order: 3
---

# Tool System
{: .no_toc }

<details open markdown="block">
<summary>Table of contents</summary>
{: .text-delta }
1. TOC
{:toc}
</details>

---

## Tool Inventory

| Tool | Risk Level | Description |
|---|---|---|
| `read_file` | 🟢 Safe | Read a file with line numbers, optional range |
| `glob` | 🟢 Safe | Find files by glob pattern (respects .gitignore) |
| `grep` | 🟢 Safe | Search file contents with regex |
| `tree` | 🟢 Safe | Show directory structure |
| `ask_user` | 🟢 Safe | Ask the user a clarifying question |
| `todo_write` | 🟢 Safe | Track tasks/progress within a session |
| `edit_file` | 🟡 Moderate | Replace exact text in a file (diff-based) |
| `write_file` | 🟡 Moderate | Create or overwrite a file |
| `web_search` | 🟡 Moderate | Search the internet |
| `web_fetch` | 🟡 Moderate | Download and clean a web page |
| `bash` | 🔴 Dangerous | Execute a shell command with timeout |
| `mcp_call` | ⚪ Varies | Call external MCP server tools |

---

## Tool Trait

Every tool implements a common trait:

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
    Safe,      // Read-only: file reads, searches
    Moderate,  // Modifies local project files
    Dangerous, // Executes commands, accesses internet
    Critical,  // Destructive: delete files, force push, etc.
}
```

---

## Tool Call Protocol

```
1. User sends message
2. Allux builds request with available tools (Ollama /api/chat)
3. Ollama responds with tool_calls or text content
4. If tool_calls:
   a. Allux evaluates permissions
   b. If approved → execute the tool
   c. Send result back to model as "tool" role message
   d. Model generates next response (may request more tools)
5. Repeat until model responds with text only (no more tool_calls)
```

---

## Key Implementations

### `read_file` — File reading with range control

```rust
// Get a portion of a file with line numbers
// Inputs: path, start_line (optional), end_line (optional)
// Auto-truncates at 200 lines and reports how many were omitted
```

### `edit_file` — Exact string replacement

```rust
// Inputs: path, old_string, new_string
// Rules:
// - old_string must appear EXACTLY ONCE in the file
// - If 0 occurrences → error: "old_string not found"
// - If 2+ occurrences → error: "provide more surrounding context"
// - On success: saves undo entry, writes file, returns diff
```

{: .highlight }
**Why exact-string replacement?** It's deterministic, verifiable, and reversible. If the AI generates the full new file content, it risks corrupting unchanged lines and wastes output tokens.

### `bash` — Shell command execution

```rust
// Inputs: command, timeout_ms (default: 30,000)
// - Runs in project root
// - Stdout capped at 10,000 chars
// - Stderr capped at 5,000 chars
// - Always requires permission evaluation
```

### `grep` — Regex content search

```rust
// Inputs: pattern (regex), path (dir or file), file_glob (optional)
// - Uses the same engine as ripgrep
// - Respects .gitignore
// - Results capped at 50 matches
```

---

## Tool Result Rendering

Tool calls are displayed in the TUI with:

- **Status indicator** — Pending / In Progress / Completed / Failed
- **Collapsible output** — expand/collapse long results
- **Diff display** — for `edit_file` and `write_file`
- **Terminal display** — for `bash` (command + stdout + stderr + exit code)
- **Timing info** — how long each tool took

```rust
enum ToolCallContent {
    Text(String),
    Diff { path: String, old_content: String, new_content: String, hunks: Vec<DiffHunk> },
    Terminal { command: String, stdout: String, stderr: String, exit_code: i32 },
}
```
