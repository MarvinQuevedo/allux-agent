//! Welcome banner, UI boxes, and shared visual helpers.

use std::path::Path;

use colored::Colorize;

use crate::ollama::types::ResponseStats;

/// Orange accent colour used throughout the UI.
pub fn accent(s: &str) -> colored::ColoredString {
    s.truecolor(217, 119, 38)
}

pub fn accent_dim(s: &str) -> colored::ColoredString {
    s.truecolor(180, 100, 45).dimmed()
}

/// Shown above `>` while editing.
pub const INPUT_FOOTER: &str =
    "Ctrl+D exit (empty line) · /help · /read <path> · /quit · Ctrl+C clear line";

/// Pretty-print token counts from Ollama.
pub fn print_token_usage(stats: &ResponseStats) {
    println!(
        "  {} {} {} {} {} {}",
        "tokens".truecolor(80, 80, 100),
        fmt_thousands(stats.prompt_tokens).truecolor(100, 160, 100),
        "in".truecolor(80, 80, 100),
        "·".truecolor(60, 60, 70),
        fmt_thousands(stats.completion_tokens).truecolor(100, 160, 200),
        "out".truecolor(80, 80, 100),
    );
}

fn fmt_thousands(n: u32) -> String {
    let mut s = n.to_string();
    let mut i = s.len();
    while i > 3 {
        i -= 3;
        s.insert(i, ',');
    }
    s
}

fn user_display_name() -> String {
    if cfg!(windows) {
        std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "there".into())
    } else {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "there".into())
    }
}

/// ALLUX in pixel-block letters — each row is exactly 44 visible chars.
///
/// Letter grid (8 chars wide, 1-char gap between letters):
///   A  L  L  U  X
const LOGO: [&str; 5] = [
    "   ██    ██       ██       ██    ██ ██    ██",
    "  ████   ██       ██       ██    ██  ██  ██ ",
    " ██  ██  ██       ██       ██    ██   ████  ",
    "████████ ██       ██       ██    ██  ██  ██ ",
    "██    ██ ████████ ████████  ██████  ██    ██",
];

/// El Salvador flag gradient — azul ↔ blanco ↔ azul, brightened for dark terminals.
const LOGO_COLORS: [(u8, u8, u8); 5] = [
    ( 80, 140, 240), // azul brillante (franja superior)
    (120, 170, 240), // transición azul → blanco
    (240, 245, 255), // blanco (franja central)
    (120, 170, 240), // transición blanco → azul
    ( 80, 140, 240), // azul brillante (franja inferior)
];

pub fn print_welcome(version: &str, model: &str, workspace: &Path, skills: &[String]) {
    let user = user_display_name();
    let cwd = workspace.display().to_string();

    println!();

    // Gradient pixel-art logo
    for (line, &(r, g, b)) in LOGO.iter().zip(LOGO_COLORS.iter()) {
        println!("  {}", line.truecolor(r, g, b).bold());
    }

    println!();

    // Subtitle line
    println!(
        "  {}  {}  {}  {}",
        format!("v{version}").truecolor(217, 119, 38).bold(),
        "·".truecolor(80, 60, 40),
        model.cyan(),
        format!("· {cwd}").truecolor(100, 100, 120)
    );

    println!();

    // Info section
    let bar = "│".truecolor(60, 60, 70);
    println!("  {}", "╭──────────────────────────────────────────────────────────╮".truecolor(60, 60, 70));
    println!(
        "  {}  {}{}{}",
        bar,
        "Welcome back, ".truecolor(180, 180, 190),
        user.truecolor(255, 255, 255).bold(),
        "!".truecolor(180, 180, 190)
    );

    if !skills.is_empty() {
        let mut display_skills = skills.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        if skills.len() > 3 {
            display_skills.push_str("…");
        }
        println!(
            "  {}  {} {}",
            bar,
            "Skills:".truecolor(140, 140, 160),
            display_skills.truecolor(100, 200, 255)
        );
    }

    println!("  {}", "├──────────────────────────────────────────────────────────┤".truecolor(60, 60, 70));
    println!(
        "  {}  {}  {}  {}  {}",
        bar,
        "/help".truecolor(217, 119, 38),
        "/model".truecolor(217, 119, 38),
        "/mode".truecolor(217, 119, 38),
        "Ctrl+D exit".truecolor(100, 100, 120)
    );
    println!("  {}", "╰──────────────────────────────────────────────────────────╯".truecolor(60, 60, 70));

    println!();
}

// ── Box-drawing helpers ────────────────────────────────────────────────

/// Width of the inner content area for UI boxes.
const BOX_WIDTH: usize = 62;

pub fn box_top_pub() -> String {
    box_top()
}

pub fn box_bottom_pub() -> String {
    box_bottom()
}

fn box_top() -> String {
    format!("  {}", accent(&format!("╭{}╮", "─".repeat(BOX_WIDTH))))
}

fn box_bottom() -> String {
    format!("  {}", accent(&format!("╰{}╯", "─".repeat(BOX_WIDTH))))
}

fn box_separator() -> String {
    format!("  {}", accent(&format!("├{}┤", "─".repeat(BOX_WIDTH))))
}

fn box_line(content: &str) -> String {
    // Visible length (approximate: strip ANSI).  For simple cases we pad to BOX_WIDTH.
    let visible_len = strip_ansi_len(content);
    let pad = if visible_len < BOX_WIDTH - 2 {
        BOX_WIDTH - 2 - visible_len
    } else {
        0
    };
    format!(
        "  {} {} {}{}",
        accent("│"),
        content,
        " ".repeat(pad),
        accent("│")
    )
}

fn box_empty() -> String {
    box_line("")
}

/// Very rough visible-character count (strips common ANSI escape sequences).
fn strip_ansi_len(s: &str) -> usize {
    let mut count = 0usize;
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c.is_ascii_alphabetic() {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        count += 1;
    }
    count
}

// ── Permission dialog boxes ────────────────────────────────────────────

/// Print a boxed permission prompt for a bash command.
pub fn print_permission_bash(command: &str) {
    println!();
    println!("{}", box_top());
    println!("{}", box_line(&format!("{}", "Allux wants to execute:".bold())));
    println!("{}", box_empty());

    // Wrap long commands
    let cmd_display = if command.len() > 54 {
        format!("{}\u{2026}", &command[..53])
    } else {
        command.to_string()
    };
    let dollar = "$".truecolor(100, 200, 100);
    println!("{}", box_line(&format!("  {dollar} {}", cmd_display.bold())));
    println!("{}", box_empty());
    println!("{}", box_separator());
    let family = command.split_whitespace().next().unwrap_or("this");
    println!("{}", box_line(&format!("{}  Allow this once", "[y]".cyan().bold())));
    println!("{}", box_line(&format!("{}  Allow for this session", "[s]".cyan().bold())));
    println!("{}", box_line(&format!("{}  Allow all {family} commands (session)", "[a]".cyan().bold())));
    println!("{}", box_line(&format!("{}  Allow {family} in this workspace (saved)", "[w]".truecolor(217, 119, 38).bold())));
    println!("{}", box_line(&format!("{}  Allow {family} globally (saved)", "[g]".truecolor(217, 119, 38).bold())));
    println!("{}", box_line(&format!("{}  Reject", "[n]".red().bold())));
    println!("{}", box_bottom());
}

/// Print a boxed permission prompt for a file edit with diff.
pub fn print_permission_edit(path: &str, old: &str, new: &str) {
    println!();
    println!("{}", box_top());
    println!("{}", box_line(&format!(
        "{}  {}",
        "Allux wants to edit:".bold(),
        path.cyan().bold()
    )));
    println!("{}", box_empty());

    // Show a simple inline diff (first few changed lines)
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let max_diff_lines = 8;
    let mut shown = 0;

    for line in &old_lines {
        if shown >= max_diff_lines { break; }
        let trimmed = if line.len() > 52 { &line[..52] } else { line };
        println!("{}", box_line(&format!("{}", format!("  - {trimmed}").red())));
        shown += 1;
    }
    for line in &new_lines {
        if shown >= max_diff_lines { break; }
        let trimmed = if line.len() > 52 { &line[..52] } else { line };
        println!("{}", box_line(&format!("{}", format!("  + {trimmed}").green())));
        shown += 1;
    }

    if old_lines.len() + new_lines.len() > max_diff_lines {
        println!("{}", box_line(&format!("{}", "  … (truncated)".dimmed())));
    }

    println!("{}", box_empty());
    println!("{}", box_separator());
    println!("{}", box_line(&format!("{}  Allow this edit", "[y]".cyan().bold())));
    println!("{}", box_line(&format!("{}  Reject", "[n]".red().bold())));
    println!("{}", box_bottom());
}

/// Horizontal divider used between conversation turns.
pub fn divider() -> String {
    format!(
        "  {}",
        "─".repeat(60).truecolor(80, 60, 40)
    )
}

/// Prefix for assistant responses.
pub fn response_prefix() -> String {
    format!("{} ", accent("❯").bold())
}
