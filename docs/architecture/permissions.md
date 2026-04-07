---
layout: default
title: Permission System
parent: Architecture
nav_order: 4
---

# Permission & Security System
{: .no_toc }

<details open markdown="block">
<summary>Table of contents</summary>
{: .text-delta }
1. TOC
{:toc}
</details>

---

## Philosophy

{: .highlight }
**The agent ALWAYS asks before executing anything that modifies state.** The user decides the *temporal scope* of each permission grant: just this once, this session, or permanently for this workspace.

Allux never "takes control" — the user always has the final word, but can progressively delegate trust as they grow comfortable.

---

## The 4 Permission Scopes

When Allux asks for permission and the user accepts, they choose **how long** that permission lasts:

```
┌───────────────────────────────────────────────────────────────────┐
│  1. ONCE — Execute once, then forget. Next time, ask again.       │
│  2. SESSION — Remembered while Allux is running. Forgotten on exit│
│  3. WORKSPACE — Saved to .allux/permissions.json (this project)   │
│  4. GLOBAL — Saved to ~/.config/allux/permissions.json (all)      │
└───────────────────────────────────────────────────────────────────┘
```

| Scope | Storage | Cleared |
|---|---|---|
| **Once** | Memory | After single use |
| **Session** | Memory | On exit |
| **Workspace** | `.allux/permissions.json` | Never (manual revoke) |
| **Global** | `~/.config/allux/permissions.json` | Never (manual revoke) |

---

## Confirmation UI

When Allux needs approval for a command:

```
┌─────────────────────────────────────────────────────────────────┐
│  Allux wants to execute:                                        │
│                                                                 │
│    $ cargo test --lib                                           │
│                                                                 │
│  [Enter]    Allow this once                                     │
│  [Ctrl+S]   Allow for this session                              │
│  [Ctrl+W]   Allow always in this workspace                      │
│  [Ctrl+G]   Allow globally (all projects)                       │
│  [Ctrl+N]   Reject                                              │
│  [?]        Explain what this command does                       │
└─────────────────────────────────────────────────────────────────┘
```

For file edits, the **diff is shown inline** before approval:

```
┌─────────────────────────────────────────────────────────────────┐
│  Allux wants to edit: src/auth/jwt.rs                           │
│                                                                 │
│  @@ -45,1 +45,1 @@                                             │
│  - let expiry = Utc::now();                                     │
│  + let expiry = Utc::now() + Duration::hours(24);               │
│                                                                 │
│  [Enter]    Allow this once                                     │
│  [Ctrl+S]   Allow edits to src/** this session                  │
│  [Ctrl+W]   Allow edits to src/** in this workspace             │
│  [Ctrl+N]   Reject                                              │
└─────────────────────────────────────────────────────────────────┘
```

---

## Permission Resolution Pipeline

Permissions are evaluated in strict order:

```
1. Hardcoded rules → DENY or ALWAYS_ASK if matched (non-negotiable)
2. One-time grants → ALLOW if key matches (then remove)
3. Session grants  → ALLOW if key found in memory
4. Workspace grants → ALLOW if key matches (from disk)
5. Global grants   → ALLOW if key matches (from disk)
6. No grant found  → ASK user
```

---

## Permission Modes

Set in `.allux.toml`:

```toml
[permissions]
mode = "balanced"  # "paranoid" | "balanced" | "yolo"
```

| Mode | Behavior |
|---|---|
| `paranoid` | Ask for every single action — nothing runs without confirmation |
| `balanced` | **Default.** Reads are free; writes and commands ask. Pre-approves: `read_file`, `glob`, `grep`, `tree`, `git status/diff/log` |
| `yolo` | Everything auto-approved **except hardcoded safety rules** |

---

## Hardcoded Safety Rules

These are compiled into the binary and **cannot be bypassed** by any configuration, prompt injection, or permission grant:

| Pattern | Result |
|---|---|
| `rm -rf /` or any `rm -rf` with absolute path | ❌ Always deny |
| `git push --force` to main/master | ❌ Always deny |
| Accessing `.env` file directly | ❌ Always deny |
| Writing outside project root | ❌ Always deny |
| Commands with `> /dev/sda` or raw device writes | ❌ Always deny |
| `sudo rm`, `sudo dd`, `sudo mkfs` | ⚠️ Always ask (never auto-approve) |

---

## Permission Key Generalization

To avoid asking "allow `cargo test --lib`?" and "allow `cargo test --integration`?" separately, Allux **generalizes** permission keys:

| Action | Generated Key |
|---|---|
| `cargo test --lib` | `bash: cargo test *` |
| `src/auth/jwt.rs` edit | `edit: src/auth/**` |
| Read any file | `read: **` |
| Fetch `https://docs.rs/...` | `web_fetch: docs.rs` |
| Web search | `web_search: *` |
| Delete `src/old.rs` | `delete: src/old.rs` (exact, never generalized) |

This means approving one `cargo test` command covers all future `cargo test` invocations in the same scope — but `cargo build` still asks separately.
