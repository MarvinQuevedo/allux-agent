use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

const MAX_OUTPUT_BYTES: usize = 20_000;
const TIMEOUT_SECS: u64 = 30;

pub async fn run_bash(command: &str) -> Result<String> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let cmd_disp = if command.len() > 40 { format!("{}...", &command[..40]) } else { command.to_string() };
    let initial_msg = format!("{} {}(command={:?})", "⚙".cyan(), "bash".bold(), cmd_disp.dimmed());
    spinner.set_message(initial_msg.clone());

    let mut child = tokio::process::Command::new(shell())
        .args(shell_args(command))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn command: {e}"))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(bool, String)>();

    let tx_out = tx.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx_out.send((false, line));
        }
    });

    let tx_err = tx.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx_err.send((true, line));
        }
    });

    drop(tx);

    let mut all_stdout = String::new();
    let mut all_stderr = String::new();

    let child_future = async {
        while let Some((is_err, line)) = rx.recv().await {
            let display_line = if line.len() > 70 { format!("{}...", &line[..67]) } else { line.clone() };
            spinner.set_message(format!("{}\n    ╰─ {}", initial_msg, display_line.dimmed()));
            
            if is_err {
                all_stderr.push_str(&line);
                all_stderr.push('\n');
            } else {
                all_stdout.push_str(&line);
                all_stdout.push('\n');
            }
        }
        child.wait().await
    };

    let exit_status = tokio::time::timeout(std::time::Duration::from_secs(TIMEOUT_SECS), child_future)
        .await
        .map_err(|_| anyhow::anyhow!("Command timed out after {TIMEOUT_SECS}s: {command}"))?
        .map_err(|e| anyhow::anyhow!("Failed to wait on command: {e}"))?;

    let exit_code = exit_status.code().unwrap_or(-1);

    spinner.finish_and_clear();
    
    if exit_code == 0 {
        println!("    {} {}(command={:?}) {}", "⚙".cyan(), "bash".bold(), cmd_disp.dimmed(), "✓".green());
    } else {
        println!("    {} {}(command={:?}) {}", "⚙".cyan(), "bash".bold(), cmd_disp.dimmed(), "✗".red());
    }

    let mut result = String::new();
    let out_trunc = truncate(&all_stdout, MAX_OUTPUT_BYTES);
    let err_trunc = truncate(&all_stderr, MAX_OUTPUT_BYTES);

    if !out_trunc.is_empty() {
        result.push_str(&out_trunc);
    }
    if !err_trunc.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("[stderr]\n");
        result.push_str(&err_trunc);
    }
    if exit_code != 0 {
        result.push_str(&format!("\n[exit code: {exit_code}]"));
    }

    if result.is_empty() {
        result = "(no output)".into();
    }

    Ok(result)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        let mut idx = max;
        while idx > 0 && !s.is_char_boundary(idx) {
            idx -= 1;
        }
        format!("{}\n[... truncated]", &s[..idx])
    } else {
        s.to_string()
    }
}

#[cfg(target_os = "windows")]
fn shell() -> &'static str {
    "cmd"
}

#[cfg(not(target_os = "windows"))]
fn shell() -> &'static str {
    "sh"
}

#[cfg(target_os = "windows")]
fn shell_args(command: &str) -> Vec<&str> {
    vec!["/C", command]
}

#[cfg(not(target_os = "windows"))]
fn shell_args(command: &str) -> Vec<&str> {
    vec!["-c", command]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bash_echo() {
        let result = run_bash("echo hello").await.unwrap();
        assert!(result.trim().contains("hello"));
    }

    #[tokio::test]
    async fn test_bash_exit_code_on_failure() {
        let result = run_bash("exit 1").await.unwrap();
        assert!(result.contains("exit code: 1"));
    }

    #[tokio::test]
    async fn test_bash_captures_stderr() {
        #[cfg(not(target_os = "windows"))]
        {
            let result = run_bash("echo error >&2").await.unwrap();
            assert!(result.contains("error") || result.contains("stderr"));
        }
    }
}
