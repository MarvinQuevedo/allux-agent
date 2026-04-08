//! Pre-built "expert prompt" actions invoked via slash commands.
//!
//! Each action expands into a prompt that is sent to the LLM as if the user
//! typed it, allowing one-command workflows like `/commit` or `/fix`.

/// A single action definition.
#[allow(dead_code)]
pub struct Action {
    /// Slash command name without the leading `/` (e.g. `"commit"`).
    pub name: &'static str,
    /// Short description shown in autocomplete and `/help`.
    pub description: &'static str,
    /// Argument hint for display (e.g. `"<file>"`, `""` for no args).
    pub args: &'static str,
    /// Prompt template. Use `{arg}` as placeholder for user-supplied arguments.
    pub prompt_template: &'static str,
}

pub const ACTIONS: &[Action] = &[
    // ── Development & Git ──────────────────────────────────────────────
    Action {
        name: "commit",
        description: "Auto-commit with smart message",
        args: "",
        prompt_template: "\
Review the current git changes using `git diff --cached` and `git diff`. \
Write a concise commit message following the Conventional Commits format \
(feat/fix/refactor/docs/chore). Stage the relevant files with `git add` \
and run `git commit`. Do not ask for confirmation — just do it.",
    },
    Action {
        name: "review",
        description: "Code review of recent changes",
        args: "",
        prompt_template: "\
Run `git diff` (and `git diff --cached` if there are staged changes). \
Analyze the changes and provide a brief code review: potential bugs, \
readability issues, security concerns, and suggestions for improvement. \
Be direct and specific.",
    },
    Action {
        name: "fix",
        description: "Find and fix build errors",
        args: "",
        prompt_template: "\
Run the project build command (`cargo build 2>&1` for Rust, or the \
appropriate build command for this project). If there are compilation \
errors, read the relevant files and fix the errors. Run the build once \
more to verify. If it still fails, report the remaining errors — do NOT \
retry more than twice.",
    },
    Action {
        name: "test",
        description: "Run tests and fix failures",
        args: "",
        prompt_template: "\
Run the project tests once (`cargo test 2>&1` for Rust, or the appropriate \
test command). If any tests fail, read the test code and the implementation, \
diagnose the issue, and apply a fix. Run tests once more to verify. \
Report the final results — do NOT retry more than twice.",
    },
    Action {
        name: "refactor",
        description: "Refactor a file",
        args: "<file>",
        prompt_template: "\
Read the file `{arg}` completely. Identify code smells: duplication, \
overly long functions, unclear naming, unnecessary complexity. \
Apply targeted refactoring while preserving behavior. Verify the \
project still compiles after changes.",
    },
    // ── Exploration & Analysis ─────────────────────────────────────────
    Action {
        name: "explain",
        description: "Explain a file in detail",
        args: "<file>",
        prompt_template: "\
Read `{arg}` completely. Explain its purpose, structure, key functions \
or types, and how it fits into the rest of the project. Be concise \
but thorough.",
    },
    Action {
        name: "find",
        description: "Find code by description",
        args: "<description>",
        prompt_template: "\
The user is looking for code related to: {arg}. \
Use grep, glob, and tree to locate the relevant files and lines. \
Show the exact file paths and line numbers with brief context.",
    },
    Action {
        name: "todo",
        description: "List all TODOs/FIXMEs",
        args: "",
        prompt_template: "\
Search the entire codebase for TODO, FIXME, HACK, and XXX comments \
using grep. List each finding with the file path, line number, and \
the comment text. Group them by priority (FIXME > TODO > HACK > XXX).",
    },
    Action {
        name: "deps",
        description: "Analyze project dependencies",
        args: "",
        prompt_template: "\
Read the project dependency files (Cargo.toml, package.json, go.mod, \
or equivalent). List all dependencies with their versions. Identify \
any that look outdated or potentially problematic. Suggest cleanup \
if there are unused or redundant dependencies.",
    },
    // ── Generation ─────────────────────────────────────────────────────
    Action {
        name: "doc",
        description: "Generate documentation for a file",
        args: "<file>",
        prompt_template: "\
Read `{arg}`. Generate documentation for all public functions, structs, \
traits, and types using the language's standard doc format (rustdoc, \
jsdoc, etc.). Edit the file to add the documentation in place.",
    },
    Action {
        name: "scaffold",
        description: "Scaffold a new component",
        args: "<type> <name>",
        prompt_template: "\
Create a new {arg} following the conventions already used in this \
project. Look at existing similar files for patterns and structure. \
Create the necessary files and update any module declarations or \
imports as needed.",
    },
    Action {
        name: "changelog",
        description: "Generate changelog from git history",
        args: "",
        prompt_template: "\
Read the recent git history with `git log --oneline -30`. Group commits \
by type (feat, fix, refactor, docs, chore). Generate a changelog entry \
in Keep a Changelog format. If a CHANGELOG.md exists, prepend the new \
entry; otherwise show the output.",
    },
    // ── DevOps & Debugging ─────────────────────────────────────────────
    Action {
        name: "doctor",
        description: "Diagnose project health",
        args: "",
        prompt_template: "\
Run a quick health check on this project. Execute each step once and \
report pass/fail: 1) check toolchain versions (rustc, cargo, node, etc.), \
2) check dependencies are installed, 3) run `cargo check` (or equivalent \
build check). Do NOT run the full test suite. Report a summary table.",
    },
    Action {
        name: "perf",
        description: "Analyze performance of a file",
        args: "<file>",
        prompt_template: "\
Read `{arg}`. Identify potential performance bottlenecks: unnecessary \
allocations, O(n^2) loops, blocking I/O in hot paths, excessive cloning, \
missing caching opportunities. Suggest specific improvements with code \
examples.",
    },
    Action {
        name: "security",
        description: "Security audit of the project",
        args: "",
        prompt_template: "\
Audit this project for common security issues. Search for: unwrap() \
on user input paths, SQL without parameterization, unsanitized inputs, \
hardcoded secrets or credentials, overly permissive file operations, \
command injection risks. Report findings with file paths and line numbers.",
    },
];

/// Try to expand a slash command into an action prompt.
///
/// Returns `Some((display_text, expanded_prompt))` if the input matches an
/// action, or `None` otherwise.
///
/// If the action requires an argument (`{arg}` in template) and none is
/// provided, returns a usage hint as the prompt.
pub fn try_expand(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let (cmd, arg) = match trimmed.find(char::is_whitespace) {
        Some(pos) => (&trimmed[1..pos], trimmed[pos..].trim()),
        None => (&trimmed[1..], ""),
    };

    let action = ACTIONS.iter().find(|a| a.name == cmd)?;

    // If the template expects an argument but none was given, return usage hint.
    if action.prompt_template.contains("{arg}") && arg.is_empty() {
        let usage = format!("Usage: /{} {}", action.name, action.args);
        return Some((trimmed.to_string(), usage));
    }

    let mut prompt = action.prompt_template.replace("{arg}", arg);
    // Append a stop instruction so smaller models don't loop calling tools
    // after the task is complete.
    prompt.push_str(
        " When you are done, respond with your final summary and STOP. \
         Do NOT call any more tools after the task is complete.",
    );
    Some((trimmed.to_string(), prompt))
}
