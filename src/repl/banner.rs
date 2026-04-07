//! Welcome banner: gradient pixel-art logo + tips box.

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
        "  {}  {}",
        accent("◇"),
        accent_dim(&format!(
            "{} in  ·  {} out  ·  {} total",
            fmt_thousands(stats.prompt_tokens),
            fmt_thousands(stats.completion_tokens),
            fmt_thousands(stats.total())
        ))
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

/// El Salvador flag gradient — azul ↔ blanco ↔ azul, one colour per row.
const LOGO_COLORS: [(u8, u8, u8); 5] = [
    (  0,  56, 147), // azul bandera (franja superior)
    ( 70, 130, 200), // transición azul → blanco
    (235, 240, 255), // blanco (franja central)
    ( 70, 130, 200), // transición blanco → azul
    (  0,  56, 147), // azul bandera (franja inferior)
];

pub fn print_welcome(version: &str, model: &str, workspace: &Path, skills: &[String]) {
    let user = user_display_name();
    let cwd = workspace.display().to_string();
    let v = accent("│");

    println!();

    // Gradient pixel-art logo
    for (line, &(r, g, b)) in LOGO.iter().zip(LOGO_COLORS.iter()) {
        println!("  {}", line.truecolor(r, g, b).bold());
    }

    println!();

    // Version / model / workspace subtitle
    println!(
        "  {}",
        format!("v{version}  ·  {model}  ·  {cwd}")
            .truecolor(140, 130, 170)
            .dimmed()
    );

    println!();

    // Welcome / tips box
    println!("  {}", accent(&format!("╭{}╮", "─".repeat(55))));
    println!(
        "  {}  {}",
        v,
        format!("Welcome back, {}!", user).bold()
    );

    // Beautifully format active skills if any exist
    if !skills.is_empty() {
        let mut display_skills = skills.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        if skills.len() > 3 {
            display_skills.push_str("...");
        }
        println!(
            "  {}  {}",
            v,
            format!("🧠 Active skills: {}", display_skills.truecolor(100, 200, 255))
        );
    }

    println!(
        "  {}  {}",
        v,
        "/help · /model list · /quit · Ctrl+D to exit".dimmed()
    );
    println!(
        "  {}  {}",
        v,
        "Chat-only model? Use ```bash blocks — Allux will offer to run them."
            .truecolor(140, 130, 170)
            .dimmed()
    );
    println!("  {}", accent(&format!("╰{}╯", "─".repeat(55))));

    println!();
}
