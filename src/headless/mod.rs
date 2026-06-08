//! Headless agent runner — drives the full agentic loop without the TUI.
//!
//! Used for non-interactive testing and benchmarking of local models:
//! the same tools, system prompt and workspace snapshot the TUI uses, but
//! driven from CLI args and printing a plain transcript + final stats.
//!
//! Permission policy (no interactive prompts here):
//!   - read-only tools (read_file, glob, grep, tree) always run
//!   - mutating tools (write_file, edit_file, bash) run only when `allow_mutating`
//!     is true; otherwise a denial is fed back to the model so it can adapt.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use serde_json::json;

use crate::config::Config;
use crate::ollama::client::OllamaClient;
use crate::ollama::types::{ChatOptions, LlmResponse, Message};
use crate::prompts::AGENT_SYSTEM_PROMPT;
use crate::tools;
use crate::workspace;

const MUTATING_TOOLS: &[&str] = &["write_file", "edit_file", "bash"];

/// Options for a headless run, parsed from CLI args.
#[derive(Debug, Clone)]
pub struct HeadlessOptions {
    pub model: Option<String>,
    pub num_ctx: Option<u32>,
    pub max_rounds: usize,
    /// Allow mutating tools (write_file/edit_file/bash) to actually run.
    pub allow_mutating: bool,
    /// `Some(false)` disables model reasoning for faster tool loops; `None` = default.
    pub think: Option<bool>,
    /// Emit a single machine-readable JSON result instead of a human transcript.
    pub json: bool,
    /// Print every tool call's full arguments and result.
    pub verbose: bool,
}

impl Default for HeadlessOptions {
    fn default() -> Self {
        Self {
            model: None,
            num_ctx: None,
            max_rounds: 25,
            allow_mutating: false,
            think: Some(false),
            json: false,
            verbose: false,
        }
    }
}

/// Outcome of a single headless prompt, for JSON output and summaries.
#[derive(Debug, Default)]
pub struct RunReport {
    pub rounds: usize,
    pub tool_calls: usize,
    pub denied_calls: usize,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub elapsed_secs: f64,
    pub final_text: String,
    pub hit_max_rounds: bool,
    pub errored: Option<String>,
}

/// Run one or more prompts through the headless agent loop.
pub async fn run(config: Config, workspace_root: PathBuf, prompts: Vec<String>, opts: HeadlessOptions) -> Result<()> {
    let model = opts.model.clone().unwrap_or_else(|| config.model.clone());
    let num_ctx = opts.num_ctx.unwrap_or(config.context_size);

    // Detect capabilities so we don't send `think` to a model that lacks it,
    // and so we can warn early if the model can't use tools at all.
    let caps = OllamaClient::capabilities(&config.ollama_url, &model)
        .await
        .ok();
    let think = match &caps {
        Some(c) if !c.thinking => None, // model can't think — never send the flag
        _ => opts.think,
    };
    let client = OllamaClient::new(&config.ollama_url, &model)
        .with_think(think)
        .with_keep_alive(config.keep_alive.clone());

    if !opts.json {
        let think_label = match think {
            Some(true) => "on",
            Some(false) => "off",
            None => "default",
        };
        let tools_note = match &caps {
            Some(c) if !c.tools => " · ⚠ NO TOOL SUPPORT",
            _ => "",
        };
        eprintln!(
            "allux headless · model={model} · num_ctx={num_ctx} · think={think_label} · mutating={} · max_rounds={}{tools_note}",
            opts.allow_mutating, opts.max_rounds
        );
    }

    // Fresh history per process; the system prompt mirrors the TUI (intro + workspace snapshot).
    let system_prompt = format!("{AGENT_SYSTEM_PROMPT}\n\n{}", workspace::snapshot(&workspace_root));
    let mut history = vec![Message::system(system_prompt)];

    let mut reports = Vec::new();
    for (i, prompt) in prompts.iter().enumerate() {
        if !opts.json {
            eprintln!("\n\x1b[1m── prompt {}/{}\x1b[0m  {}", i + 1, prompts.len(), prompt);
        }
        history.push(Message::user(prompt.clone()));
        let report = run_one(&client, &mut history, num_ctx, &opts).await;
        if opts.json {
            print_json(prompt, &report);
        }
        reports.push(report);
    }

    if !opts.json {
        print_summary(&reports);
    }
    Ok(())
}

/// Drive the agent loop for the current `history` until the model returns text
/// (no tool calls) or the round cap is hit. Returns a report for this turn.
async fn run_one(
    client: &OllamaClient,
    history: &mut Vec<Message>,
    num_ctx: u32,
    opts: &HeadlessOptions,
) -> RunReport {
    let tools = tools::all_definitions();
    let options = ChatOptions { temperature: Some(0.0), num_ctx: Some(num_ctx) };
    let mut report = RunReport::default();
    let start = Instant::now();

    for round in 0..opts.max_rounds {
        report.rounds = round + 1;

        let result = client
            .chat(history, Some(&tools), Some(options.clone()), |_chunk| {})
            .await;

        match result {
            Err(e) => {
                report.errored = Some(e.to_string());
                if !opts.json {
                    eprintln!("\x1b[31mError: {e}\x1b[0m");
                }
                report.elapsed_secs = start.elapsed().as_secs_f64();
                return report;
            }
            Ok(LlmResponse::Text { content, stats }) => {
                report.prompt_tokens += stats.prompt_tokens;
                report.completion_tokens += stats.completion_tokens;
                report.final_text = content.clone();
                history.push(Message::assistant(content.clone()));
                if !opts.json {
                    println!("{}", content.trim_end());
                }
                report.elapsed_secs = start.elapsed().as_secs_f64();
                return report;
            }
            Ok(LlmResponse::ToolCalls { calls, text, stats }) => {
                report.prompt_tokens += stats.prompt_tokens;
                report.completion_tokens += stats.completion_tokens;
                report.tool_calls += calls.len();
                history.push(Message::assistant_tool_calls(calls.clone(), &text));

                for call in &calls {
                    let name = &call.function.name;
                    let args = &call.function.arguments;
                    let detail = arg_summary(args);

                    let mutating = MUTATING_TOOLS.contains(&name.as_str());
                    if mutating && !opts.allow_mutating {
                        report.denied_calls += 1;
                        if !opts.json {
                            println!("  \x1b[33m✗ denied\x1b[0m {name} {detail}  (pass --yes to allow)");
                        }
                        history.push(Message::tool_result(
                            name.clone(),
                            format!("Permission denied (headless read-only mode): {name} was not executed. Re-run allux with --yes to allow write_file/edit_file/bash."),
                        ));
                        continue;
                    }

                    if !opts.json {
                        println!("  \x1b[34m⚡\x1b[0m {name} {detail}");
                    }
                    let out = match tools::dispatch(name, args, true).await {
                        Ok(s) => s,
                        Err(e) => format!("Error: {e}"),
                    };
                    if opts.verbose && !opts.json {
                        let preview: String = out.chars().take(500).collect();
                        println!("    \x1b[2m{}\x1b[0m", preview.replace('\n', "\n    "));
                    }
                    history.push(Message::tool_result(name.clone(), out));
                }
            }
        }
    }

    report.hit_max_rounds = true;
    report.elapsed_secs = start.elapsed().as_secs_f64();
    if !opts.json {
        eprintln!("\x1b[33m[reached max rounds: {}]\x1b[0m", opts.max_rounds);
    }
    report
}

/// A short human label for a tool call's arguments (path, command, pattern…).
fn arg_summary(args: &serde_json::Value) -> String {
    for key in ["path", "command", "pattern"] {
        if let Some(v) = args.get(key).and_then(|v| v.as_str()) {
            let v = if v.len() > 60 { &v[..60] } else { v };
            return v.to_string();
        }
    }
    String::new()
}

fn tps(report: &RunReport) -> f64 {
    if report.elapsed_secs > 0.0 {
        report.completion_tokens as f64 / report.elapsed_secs
    } else {
        0.0
    }
}

fn print_json(prompt: &str, r: &RunReport) {
    let v = json!({
        "prompt": prompt,
        "rounds": r.rounds,
        "tool_calls": r.tool_calls,
        "denied_calls": r.denied_calls,
        "prompt_tokens": r.prompt_tokens,
        "completion_tokens": r.completion_tokens,
        "elapsed_secs": (r.elapsed_secs * 100.0).round() / 100.0,
        "tokens_per_sec": (tps(r) * 10.0).round() / 10.0,
        "hit_max_rounds": r.hit_max_rounds,
        "errored": r.errored,
        "final_text": r.final_text,
    });
    println!("{}", serde_json::to_string(&v).unwrap_or_default());
}

fn print_summary(reports: &[RunReport]) {
    eprintln!("\n\x1b[1m── summary\x1b[0m");
    let mut total_secs = 0.0;
    let mut total_comp = 0u32;
    let mut total_tools = 0usize;
    for (i, r) in reports.iter().enumerate() {
        let status = if let Some(e) = &r.errored {
            format!("\x1b[31mERR: {}\x1b[0m", e.chars().take(40).collect::<String>())
        } else if r.hit_max_rounds {
            "\x1b[33mmax-rounds\x1b[0m".to_string()
        } else {
            "\x1b[32mok\x1b[0m".to_string()
        };
        eprintln!(
            "  #{:<2} {:<10} rounds={:<2} tools={:<2} denied={:<2} {:>5.1} tok/s  {:>5.1}s",
            i + 1, status, r.rounds, r.tool_calls, r.denied_calls, tps(r), r.elapsed_secs
        );
        total_secs += r.elapsed_secs;
        total_comp += r.completion_tokens;
        total_tools += r.tool_calls;
    }
    let avg_tps = if total_secs > 0.0 { total_comp as f64 / total_secs } else { 0.0 };
    eprintln!(
        "  total: {} prompt(s), {} tool call(s), {:.1}s, avg {:.1} tok/s",
        reports.len(), total_tools, total_secs, avg_tps
    );
}
