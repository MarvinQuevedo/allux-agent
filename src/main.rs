//! Allux CLI binary.
//!
//! This binary provides the command-line interface for the Allux tool.

mod actions;
mod compression;
mod config;
#[allow(dead_code)]
mod input;
#[allow(dead_code)]
mod doctor;
mod headless;
mod monitor;
mod ollama;
mod permissions;
mod prompts;
#[allow(dead_code)]
mod repl;
mod session;
mod setup;
mod tools;
mod tui;
mod workspace;

use std::env;

use anyhow::Result;
use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // Ignore Ctrl+C globally so it never force-quits the CLI.
    // (In raw mode, it's captured as a key event and clears the line).
    tokio::spawn(async {
        loop {
            let _ = tokio::signal::ctrl_c().await;
        }
    });

    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => run_headless(&args[1..]).await,
        Some("-h") | Some("--help") => {
            print_help();
            Ok(())
        }
        _ => launch_tui().await,
    }
}

async fn launch_tui() -> Result<()> {
    let config = match Config::load()? {
        Some(cfg) => cfg,
        None => setup::run_wizard().await?,
    };

    let workspace_root = env::current_dir()?;
    let metrics = monitor::new_shared();
    monitor::spawn_collector(metrics.clone());

    tui::run(config, workspace_root, metrics).await
}

/// `allux run [OPTIONS] "<prompt>"` — non-interactive agent loop for testing/benchmarking.
async fn run_headless(args: &[String]) -> Result<()> {
    let mut config = Config::load()?.unwrap_or_default();
    let mut opts = headless::HeadlessOptions::default();
    let mut prompts: Vec<String> = Vec::new();
    let mut cwd: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        // Value for flags that take one: consumes the next arg and advances `i`.
        macro_rules! val {
            () => {{
                i += 1;
                args.get(i).cloned()
            }};
        }
        match args[i].as_str() {
            "-m" | "--model" => opts.model = val!(),
            "--ctx" | "--num-ctx" => opts.num_ctx = val!().and_then(|s| s.parse().ok()),
            "--max-rounds" => {
                if let Some(n) = val!().and_then(|s| s.parse().ok()) {
                    opts.max_rounds = n;
                }
            }
            "--yes" | "-y" => opts.allow_mutating = true,
            "--read-only" => opts.allow_mutating = false,
            "--think" => opts.think = Some(true),
            "--no-think" => opts.think = Some(false),
            "--think-default" => opts.think = None,
            "--json" => opts.json = true,
            "--verbose" | "-v" => opts.verbose = true,
            "-C" | "--cwd" => cwd = val!(),
            "-f" | "--prompts-file" => {
                if let Some(path) = val!() {
                    let content = std::fs::read_to_string(&path)?;
                    for line in content.lines() {
                        let line = line.trim();
                        if !line.is_empty() && !line.starts_with('#') {
                            prompts.push(line.to_string());
                        }
                    }
                }
            }
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            other if other.starts_with('-') => {
                anyhow::bail!("Unknown flag: {other}  (try `allux run --help`)");
            }
            _ => prompts.push(args[i].clone()),
        }
        i += 1;
    }

    if let Some(dir) = cwd {
        env::set_current_dir(&dir)?;
    }
    // CLI override wins over config for the persisted model name too (used in snapshot, etc.).
    if let Some(m) = &opts.model {
        config.model = m.clone();
    }

    if prompts.is_empty() {
        anyhow::bail!("No prompt given. Usage: allux run [OPTIONS] \"<prompt>\"  (or -f <file>)");
    }

    let workspace_root = env::current_dir()?;
    headless::run(config, workspace_root, prompts, opts).await
}

fn print_help() {
    println!(
        "allux — local code agent powered by Ollama\n\n\
         USAGE:\n  \
           allux                         Launch the interactive TUI\n  \
           allux run [OPTIONS] \"<prompt>\"  Run one prompt headless (for testing/benchmarks)\n\n\
         RUN OPTIONS:\n  \
           -m, --model <name>      Model to use (default: config)\n  \
           --ctx <n>               Context window / num_ctx (default: config)\n  \
           --max-rounds <n>        Max tool rounds per prompt (default: 25)\n  \
           -y, --yes               Allow mutating tools (write_file/edit_file/bash) to run\n  \
           --read-only             Deny mutating tools (default)\n  \
           --think | --no-think    Toggle model reasoning (default: --no-think)\n  \
           --json                  Emit one JSON object per prompt (machine-readable)\n  \
           -v, --verbose           Print tool output previews\n  \
           -C, --cwd <dir>         Run in this directory\n  \
           -f, --prompts-file <f>  Read prompts from a file (one per line, # = comment)\n"
    );
}
