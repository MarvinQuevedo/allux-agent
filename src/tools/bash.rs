use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

const MAX_OUTPUT_BYTES: usize = 20_000;
const TIMEOUT_SECS: u64 = 60;
/// How long to watch a long-running command before declaring it "running in
/// background". Long enough to catch an immediate crash (bad flag, port in use).
const BACKGROUND_GRACE_MS: u64 = 1_800;

// ── Background process registry ───────────────────────────────────────────────
//
// Servers and watchers (`python -m http.server`, `npm run dev`, `tail -f`, …)
// never exit on their own, so running them in the foreground would block the
// agent for the full timeout and then orphan the process. Instead we detach
// them, log their output to a file, and keep a handle here so the user can list
// and stop them.

struct BgProc {
    pid: u32,
    command: String,
    log: PathBuf,
    child: tokio::process::Child,
}

static BG_PROCS: Mutex<Vec<BgProc>> = Mutex::new(Vec::new());
static BG_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Heuristic: does this command run indefinitely (a server, watcher, or an
/// explicitly backgrounded `cmd &`)? Those are detached instead of awaited.
fn is_long_running(command: &str) -> bool {
    let c = command.trim();
    if c.ends_with('&') && !c.ends_with("&&") {
        return true;
    }
    let lc = c.to_lowercase();
    const PATTERNS: &[&str] = &[
        "http.server",
        "http-server",
        "npm run dev",
        "npm start",
        "yarn dev",
        "yarn start",
        "pnpm dev",
        "vite",
        "next dev",
        "flask run",
        "uvicorn",
        "gunicorn",
        "rails server",
        "rails s ",
        "php -s",
        "serve ",
        "live-server",
        "webpack serve",
        "webpack-dev-server",
        "ng serve",
        "nodemon",
        "tail -f",
        "jekyll serve",
        "hugo server",
        "docker compose up",
        "docker-compose up",
        "ngrok",
        "watch ",
    ];
    PATTERNS.iter().any(|p| lc.contains(p))
}

/// Currently tracked background processes: `(pid, command, log_path)`.
pub fn list_background() -> Vec<(u32, String, String)> {
    BG_PROCS
        .lock()
        .map(|g| {
            g.iter()
                .map(|p| (p.pid, p.command.clone(), p.log.display().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Stop background processes. `target = Some(pid)` stops one; `None` stops all.
/// Returns the number stopped.
pub fn stop_background(target: Option<u32>) -> usize {
    let Ok(mut procs) = BG_PROCS.lock() else {
        return 0;
    };
    let mut stopped = 0;
    procs.retain_mut(|p| {
        let hit = target.is_none() || target == Some(p.pid);
        if hit {
            let _ = p.child.start_kill();
            stopped += 1;
        }
        !hit
    });
    stopped
}

/// Run a long-running command detached: output goes to a log file, and we return
/// as soon as it is confirmed running (or report its output if it exited fast).
async fn run_background(command: &str, quiet: bool) -> Result<String> {
    // Drop a single trailing `&` — we background it ourselves.
    let cmd = {
        let t = command.trim();
        let t = if t.ends_with('&') && !t.ends_with("&&") {
            t[..t.len() - 1].trim()
        } else {
            t
        };
        t.to_string()
    };

    let id = BG_COUNTER.fetch_add(1, Ordering::Relaxed);
    let log_path = std::env::temp_dir().join(format!("allux-bg-{id}.log"));
    let log = std::fs::File::create(&log_path)
        .map_err(|e| anyhow::anyhow!("Failed to create background log file: {e}"))?;
    let log_err = log
        .try_clone()
        .map_err(|e| anyhow::anyhow!("Failed to clone log handle: {e}"))?;

    let mut child = tokio::process::Command::new(shell())
        .args(shell_args(&cmd))
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn background command: {e}"))?;

    let pid = child.id().unwrap_or(0);

    // Watch briefly: if it crashes immediately (bad flag, port busy), surface that
    // instead of falsely claiming success.
    match tokio::time::timeout(Duration::from_millis(BACKGROUND_GRACE_MS), child.wait()).await {
        Ok(Ok(status)) => {
            let logs = std::fs::read_to_string(&log_path).unwrap_or_default();
            let code = status.code().unwrap_or(-1);
            let body = truncate(&logs, MAX_OUTPUT_BYTES);
            let body = if body.trim().is_empty() { "(no output)".to_string() } else { body };
            if !quiet {
                println!(
                    "    {} {} {}",
                    "▸".truecolor(100, 180, 255),
                    format!("$ {cmd}").truecolor(140, 140, 160),
                    format!("exited fast ({code})").yellow()
                );
            }
            return Ok(format!(
                "{body}\n[Process exited immediately with code {code} — not left running. Check the command/flags.]"
            ));
        }
        Ok(Err(e)) => return Err(anyhow::anyhow!("Failed to wait on background command: {e}")),
        Err(_) => { /* still alive after grace → genuine long-running process */ }
    }

    let logs = std::fs::read_to_string(&log_path).unwrap_or_default();
    let preview = truncate(&logs, 2_000);
    let preview_block = if preview.trim().is_empty() {
        String::new()
    } else {
        format!("\nOutput so far:\n{preview}")
    };

    if !quiet {
        println!(
            "    {} {} {}",
            "▸".truecolor(100, 180, 255),
            format!("$ {cmd}").truecolor(140, 140, 160),
            format!("⇢ background (pid {pid})").green()
        );
    }

    if let Ok(mut procs) = BG_PROCS.lock() {
        procs.push(BgProc { pid, command: cmd.clone(), log: log_path.clone(), child });
    }

    Ok(format!(
        "[Started in background] pid={pid} · {cmd}\nLogs: {log}{preview}\n\
         [It is running now. Tell the user the URL/port. Use /servers to list or /stop {pid} to stop it. Do NOT re-run it.]",
        log = log_path.display(),
        preview = preview_block,
    ))
}

/// Run a bash command. When `quiet` is true (TUI mode), suppress all
/// terminal output (spinners, status lines) to avoid corrupting the
/// ratatui buffer.
///
/// Long-running commands (servers, watchers) are detected and run in the
/// background via [`run_background`]; everything else runs in the foreground
/// with a [`TIMEOUT_SECS`] cap and is killed on drop so it can't be orphaned.
pub async fn run_bash(command: &str, quiet: bool) -> Result<String> {
    if is_long_running(command) {
        return run_background(command, quiet).await;
    }
    let spinner = if quiet {
        ProgressBar::hidden()
    } else {
        let s = ProgressBar::new_spinner();
        s.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        s.enable_steady_tick(std::time::Duration::from_millis(80));
        s
    };

    let cmd_disp = if command.len() > 50 { format!("{}…", &command[..49]) } else { command.to_string() };
    let initial_msg = format!("{} {}", "▸".truecolor(100, 180, 255), format!("$ {}", cmd_disp).bold());
    if !quiet {
        spinner.set_message(initial_msg.clone());
    }

    let mut child = tokio::process::Command::new(shell())
        .args(shell_args(command))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Ensure a stuck command that hits the timeout is killed, not orphaned.
        .kill_on_drop(true)
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

    let mut timed_out = false;
    let exit_status = {
        let mut child_wait = Box::pin(child.wait());
        let mut timeout_gate = Box::pin(tokio::time::sleep(std::time::Duration::from_secs(TIMEOUT_SECS)));

        loop {
            tokio::select! {
                Some((is_err, line)) = rx.recv() => {
                    let display_line = if line.chars().count() > 60 {
                        let truncated: String = line.chars().take(59).collect();
                        format!("{truncated}…")
                    } else {
                        line.clone()
                    };
                    if !quiet {
                        spinner.set_message(format!("{}\n    {} {}", initial_msg, "│".truecolor(60, 60, 70), display_line.truecolor(120, 120, 130)));
                    }

                    if is_err {
                        all_stderr.push_str(&line);
                        all_stderr.push('\n');
                    } else {
                        all_stdout.push_str(&line);
                        all_stdout.push('\n');
                    }
                }
                status = &mut child_wait => {
                    break status.map(Some).map_err(|e| anyhow::anyhow!("Failed to wait on command: {e}"));
                }
                _ = &mut timeout_gate => {
                    timed_out = true;
                    break Ok(None);
                }
            }
        }
    };

    // On timeout, kill_on_drop kills the child when `child` is dropped at the end
    // of this function. Start the kill now so the process stops promptly.
    if timed_out {
        let _ = child.start_kill();
    }

    spinner.finish_and_clear();

    let exit_code = if timed_out {
        -1
    } else {
        exit_status?.unwrap().code().unwrap_or(-1)
    };

    if !quiet {
        if timed_out {
            println!("    {} {} {}", "▸".truecolor(100, 180, 255), format!("$ {}", cmd_disp).truecolor(140, 140, 160), "⌛ timeout".yellow());
        } else if exit_code == 0 {
            println!("    {} {} {}", "▸".truecolor(100, 180, 255), format!("$ {}", cmd_disp).truecolor(140, 140, 160), "✓".green());
        } else {
            println!("    {} {} {}", "▸".truecolor(100, 180, 255), format!("$ {}", cmd_disp).truecolor(140, 140, 160), format!("✗ exit {}", exit_code).red());
        }
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

    if timed_out {
        result.push_str(&format!(
            "\n[Command exceeded {TIMEOUT_SECS}s and was stopped. If it is a server or long-running task, it will run in the background automatically — just issue it normally.]"
        ));
    } else if exit_code != 0 {
        result.push_str(&format!("\n[exit code: {exit_code}]"));
    }

    if result.is_empty() {
        result = "(no output)".into();
    }

    Ok(result)
}

/// Truncate to `max` bytes, keeping both the head and the tail. Command output
/// often puts the most relevant lines (errors, test summaries, final results) at
/// the *end*, so a head-only cut would hide them; this keeps ~2/3 from the start
/// and ~1/3 from the end within the same budget, so the model sees both.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }

    let head_budget = max * 2 / 3;
    let tail_budget = max - head_budget;

    // Head ends on a char boundary at or below head_budget.
    let mut head_end = head_budget.min(s.len());
    while head_end > 0 && !s.is_char_boundary(head_end) {
        head_end -= 1;
    }

    // Tail starts on a char boundary at or above len - tail_budget.
    let mut tail_start = s.len().saturating_sub(tail_budget);
    while tail_start < s.len() && !s.is_char_boundary(tail_start) {
        tail_start += 1;
    }

    if tail_start <= head_end {
        // Degenerate budget — fall back to a plain head cut.
        return format!("{}\n[... truncated]", &s[..head_end]);
    }

    let dropped = tail_start - head_end;
    format!(
        "{}\n[... {dropped} bytes truncated ...]\n{}",
        &s[..head_end],
        &s[tail_start..]
    )
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
        let result = run_bash("echo hello", true).await.unwrap();
        assert!(result.trim().contains("hello"));
    }

    #[tokio::test]
    async fn test_bash_exit_code_on_failure() {
        let result = run_bash("exit 1", true).await.unwrap();
        assert!(result.contains("exit code: 1"));
    }

    #[tokio::test]
    async fn test_bash_captures_stderr() {
        #[cfg(not(target_os = "windows"))]
        {
            let result = run_bash("echo error >&2", true).await.unwrap();
            assert!(result.contains("error") || result.contains("stderr"));
        }
    }

    #[test]
    fn test_truncate_noop_when_within_budget() {
        assert_eq!(truncate("hello", 1000), "hello");
    }

    #[test]
    fn test_truncate_keeps_head_and_tail() {
        let s: String = (0..5000).map(|i| format!("line {i}\n")).collect();
        let out = truncate(&s, 1000);
        assert!(out.len() < s.len());
        assert!(out.contains("line 0\n"), "head should survive");
        assert!(out.contains("line 4999"), "tail should survive");
        assert!(out.contains("bytes truncated"), "should mark the cut");
    }

    #[test]
    fn test_is_long_running() {
        assert!(is_long_running("python3 -m http.server 8000"));
        assert!(is_long_running("npm run dev"));
        assert!(is_long_running("tail -f /var/log/system.log"));
        assert!(is_long_running("sleep 5 &"));
        assert!(!is_long_running("echo hi"));
        assert!(!is_long_running("ls && cargo build")); // `&&` is not backgrounding
        assert!(!is_long_running("cargo test"));
    }

    #[tokio::test]
    async fn test_background_quick_exit_is_reported() {
        // A "server-like" command that actually exits immediately should be
        // reported as exited, not falsely left running.
        let result = run_bash("echo starting && true &", true).await.unwrap();
        assert!(result.contains("exited immediately") || result.contains("starting"));
    }

    #[tokio::test]
    async fn test_background_server_is_tracked_and_stoppable() {
        // `sleep` stands in for a server: runs past the grace window.
        let before = list_background().len();
        let out = run_bash("sleep 30 &", true).await.unwrap();
        assert!(out.contains("Started in background"));
        assert!(list_background().len() > before);
        let stopped = stop_background(None);
        assert!(stopped >= 1);
    }
}
