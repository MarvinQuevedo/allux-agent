//! Visual demo: simulates a real Allux agent session and shows what each
//! compression stage produces.
//!
//! Run with: cargo test --test ai_compress_demo -- --nocapture

use std::fs;
use std::path::Path;

// ── Reuse compression helpers from the main crate ────────────────────────

fn strip_ansi(text: &str) -> String {
    let mut r = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n.is_ascii_alphabetic() || n == '~' || n == '@' { break; }
                }
            }
        } else {
            r.push(c);
        }
    }
    r
}

fn collapse_blanks(text: &str) -> String {
    let mut r = String::with_capacity(text.len());
    let mut blanks = 0u32;
    for line in text.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks <= 1 { r.push('\n'); }
        } else {
            blanks = 0;
            r.push_str(line);
            r.push('\n');
        }
    }
    if !text.ends_with('\n') && r.ends_with('\n') { r.pop(); }
    r
}

fn trim_trailing(text: &str) -> String {
    text.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n")
}

fn dedup_lines(text: &str, threshold: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut r = String::with_capacity(text.len());
    let mut i = 0;
    while i < lines.len() {
        let cur = lines[i];
        let mut count = 1;
        while i + count < lines.len() && lines[i + count] == cur { count += 1; }
        if count >= threshold {
            r.push_str(cur); r.push('\n');
            r.push_str(&format!("[... repeated {} more times ...]\n", count - 1));
            i += count;
        } else {
            for _ in 0..count { r.push_str(cur); r.push('\n'); }
            i += count;
        }
    }
    if !text.ends_with('\n') && r.ends_with('\n') { r.pop(); }
    r
}

fn compact_line_nums(text: &str) -> String {
    let mut r = String::with_capacity(text.len());
    for line in text.lines() {
        if let Some(p) = line.find(" | ") {
            let prefix = &line[..p];
            if prefix.trim().chars().all(|c| c.is_ascii_digit()) {
                r.push_str(prefix.trim());
                r.push('|');
                r.push_str(&line[p + 3..]);
                r.push('\n');
                continue;
            }
        }
        r.push_str(line);
        r.push('\n');
    }
    if !text.ends_with('\n') && r.ends_with('\n') { r.pop(); }
    r
}

fn compact_json(text: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        if let Ok(c) = serde_json::to_string(&v) {
            if c.len() < text.len() { return c; }
        }
    }
    text.to_string()
}

fn smart_truncate(text: &str, max: usize) -> String {
    if text.len() <= max { return text.to_string(); }
    let head_b = (max * 60) / 100;
    let tail_b = max - head_b - 80;
    let he = text[..head_b.min(text.len())].rfind('\n').unwrap_or(head_b.min(text.len()));
    let head = &text[..he];
    let ts_raw = text.len().saturating_sub(tail_b);
    let ts = text[ts_raw..].find('\n').map(|p| ts_raw + p + 1).unwrap_or(ts_raw);
    let tail = &text[ts..];
    let omit = text.len() - head.len() - tail.len();
    format!("{}\n\n[... {} chars (~{} tokens) omitted ...]\n\n{}", head, omit, omit / 4, tail)
}

fn compress_standard(tool: &str, text: &str) -> String {
    let mut t = strip_ansi(text);
    t = collapse_blanks(&t);
    t = trim_trailing(&t);
    match tool {
        "bash" => { t = dedup_lines(&t, 3); t = compact_json(&t); }
        "read_file" => { t = compact_line_nums(&t); }
        "grep" => { t = dedup_lines(&t, 2); }
        _ => { t = compact_json(&t); t = dedup_lines(&t, 3); }
    }
    t
}

fn compress_aggressive(tool: &str, text: &str) -> String {
    let mut t = compress_standard(tool, text);
    if t.len() > 8000 { t = smart_truncate(&t, 8000); }
    t
}

// ── AI summarization prompt builder ──────────────────────────────────────

const AI_SYSTEM: &str = "\
You are a context compressor. Your job is to summarize a conversation history \
into a concise but complete summary that preserves ALL important information. \
Include: what the user asked, what files were read/modified, what tools were called \
and their key results, what decisions were made, what errors occurred, and what \
the current state of work is. \
Use bullet points. Be precise with file paths, function names, and error messages. \
Do NOT add commentary or opinions — only factual summary. \
Reply ONLY with the summary, no preamble.";

fn build_ai_prompt(messages: &[SimMessage]) -> String {
    let mut prompt = String::from("Summarize this conversation history concisely:\n\n");
    for m in messages {
        let label = match m.role {
            "user" => "USER".to_string(),
            "assistant" => "ASSISTANT".to_string(),
            "tool" => format!("TOOL[{}]", m.tool_name.unwrap_or("unknown")),
            other => other.to_string(),
        };
        let content = if m.content.len() > 2000 {
            let he = m.content[..1000].rfind('\n').unwrap_or(1000);
            let ts = m.content.len().saturating_sub(500);
            let ts = m.content[ts..].find('\n').map(|p| ts + p + 1).unwrap_or(ts);
            format!(
                "{}...[{} chars omitted]...{}",
                &m.content[..he],
                m.content.len() - he - (m.content.len() - ts),
                &m.content[ts..]
            )
        } else {
            m.content.to_string()
        };
        prompt.push_str(&format!("--- {label} ---\n{content}\n\n"));
    }
    prompt
}

// ── Simulated session ────────────────────────────────────────────────────

struct SimMessage {
    role: &'static str,
    content: String,
    tool_name: Option<&'static str>,
}

fn build_simulated_session() -> Vec<SimMessage> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut session = Vec::new();

    // 1. System prompt
    session.push(SimMessage {
        role: "system",
        content: format!(
            "You are Allux, a local code assistant powered by Ollama. \
             You help with software engineering tasks.\n\n\
             ### Workspace snapshot\n- **Project root:** `{}`\n\
             - Rust project: allux v0.1.0\n- Modules: compression, config, \
             input, ollama, permissions, repl, session, setup, tools, workspace",
            root.display()
        ),
        tool_name: None,
    });

    // 2. User asks to refactor
    session.push(SimMessage {
        role: "user",
        content: "I want to add token compression to reduce context window usage. \
                  First read the main entry point and the repl module to understand \
                  the architecture, then suggest where to add compression."
            .to_string(),
        tool_name: None,
    });

    // 3. Assistant calls read_file on main.rs
    session.push(SimMessage {
        role: "assistant",
        content: String::new(), // tool call, no text
        tool_name: None,
    });

    // 4. Tool result: read_file(src/main.rs)
    let main_rs = fs::read_to_string(root.join("src/main.rs")).unwrap_or_default();
    let main_read = main_rs
        .lines()
        .enumerate()
        .map(|(i, l)| format!("{:>4} | {}", i + 1, l))
        .collect::<Vec<_>>()
        .join("\n");
    session.push(SimMessage {
        role: "tool",
        content: main_read,
        tool_name: Some("read_file"),
    });

    // 5. Tool result: read_file(src/repl/mod.rs) — big file
    let repl_rs = fs::read_to_string(root.join("src/repl/mod.rs")).unwrap_or_default();
    let repl_read = repl_rs
        .lines()
        .enumerate()
        .take(500)
        .map(|(i, l)| format!("{:>4} | {}", i + 1, l))
        .collect::<Vec<_>>()
        .join("\n");
    let total_lines = repl_rs.lines().count();
    let repl_output = if total_lines > 500 {
        format!("{}\n\n[... truncated: showing 500/{} lines ...]", repl_read, total_lines)
    } else {
        repl_read
    };
    session.push(SimMessage {
        role: "tool",
        content: repl_output,
        tool_name: Some("read_file"),
    });

    // 6. Tool result: grep for "compact_history"
    session.push(SimMessage {
        role: "tool",
        content: "src/repl/mod.rs:311:    fn compact_history(&mut self) {\n\
                  src/repl/mod.rs:324:        self.run_compression_pass();\n\
                  src/repl/mod.rs:472:            self.compact_history();"
            .to_string(),
        tool_name: Some("grep"),
    });

    // 7. Assistant analysis
    session.push(SimMessage {
        role: "assistant",
        content: "I've analyzed the codebase. Here's the architecture:\n\n\
                  1. **Entry point** (`src/main.rs`): loads config, creates `Repl`, calls `repl.run()`\n\
                  2. **REPL module** (`src/repl/mod.rs`): main event loop with `run_agentic_loop()`\n\
                  3. **Tool dispatch**: `execute_tool_calls()` at line ~1060 collects raw output into `Message::tool_result()`\n\
                  4. **History compaction**: `compact_history()` at line 311 uses simple eviction\n\n\
                  I recommend adding compression at two points:\n\
                  - **After tool execution** (line ~1085): compress output before storing in history\n\
                  - **In `compact_history()`** (line 311): add in-place compression before eviction\n\n\
                  The best strategies for a Rust CLI agent:\n\
                  1. ANSI code stripping (bash outputs have terminal colors)\n\
                  2. Blank line collapsing\n\
                  3. JSON compaction (API responses)\n\
                  4. Line number prefix compression (read_file output)\n\
                  5. Consecutive line deduplication (build logs)\n\
                  6. Smart truncation (keep head+tail for large outputs)\n\n\
                  Shall I implement this?"
            .to_string(),
        tool_name: None,
    });

    // 8. User confirms
    session.push(SimMessage {
        role: "user",
        content: "Yes, implement it. Create a new `src/compression/mod.rs` module.".to_string(),
        tool_name: None,
    });

    // 9. Assistant creates the file
    session.push(SimMessage {
        role: "assistant",
        content: String::new(),
        tool_name: None,
    });

    // 10. Tool result: write_file
    session.push(SimMessage {
        role: "tool",
        content: "File written: src/compression/mod.rs (485 lines)".to_string(),
        tool_name: Some("write_file"),
    });

    // 11. Tool result: edit_file on main.rs
    session.push(SimMessage {
        role: "tool",
        content: "Replaced 1 occurrence in src/main.rs:\n\
                  - mod config;\n\
                  + mod compression;\n\
                  + mod config;"
            .to_string(),
        tool_name: Some("edit_file"),
    });

    // 12. Tool result: edit_file on repl/mod.rs
    session.push(SimMessage {
        role: "tool",
        content: "Replaced 1 occurrence in src/repl/mod.rs:\n\
                  - use crate::tools;\n\
                  + use crate::compression::{self, CompressionLevel};\n\
                  + use crate::tools;\n\n\
                  Replaced 1 occurrence in src/repl/mod.rs (tool output compression):\n\
                  - results.push(Message::tool_result(name.clone(), output));\n\
                  + let cr = compression::compress_tool_output(...);\n\
                  + results.push(Message::tool_result(name.clone(), cr.text));"
            .to_string(),
        tool_name: Some("edit_file"),
    });

    // 13. Tool result: bash(cargo check)
    session.push(SimMessage {
        role: "tool",
        content: format!(
            "\x1b[32m    Checking\x1b[0m allux v0.1.0\n\
             {}\
             \x1b[32m    Finished\x1b[0m `dev` profile in 1.04s",
            "\x1b[33mwarning\x1b[0m: unused variable `phase1_saved`\n".repeat(3)
        ),
        tool_name: Some("bash"),
    });

    // 14. Tool result: bash(cargo test)
    session.push(SimMessage {
        role: "tool",
        content: {
            let mut s = String::from("running 75 tests\n");
            for i in 1..=75 {
                s.push_str(&format!("test test_{:03} ... ok\n", i));
            }
            s.push_str("\ntest result: ok. 75 passed; 0 failed; 0 ignored\n");
            s
        },
        tool_name: Some("bash"),
    });

    // 15. Assistant final reply
    session.push(SimMessage {
        role: "assistant",
        content: "Done! I've implemented the token compression module:\n\n\
                  - Created `src/compression/mod.rs` with 7 strategies\n\
                  - Integrated into the tool output pipeline in `repl/mod.rs`\n\
                  - Improved `compact_history()` with two-phase compression\n\
                  - All 75 tests pass, project compiles with only pre-existing warnings\n\n\
                  Standard compression saves ~8.5% on tool outputs. \
                  Aggressive mode saves ~30% during history compaction."
            .to_string(),
        tool_name: None,
    });

    // 16. User asks follow-up
    session.push(SimMessage {
        role: "user",
        content: "Now add an AI-powered compression mode that uses the LLM to summarize old messages."
            .to_string(),
        tool_name: None,
    });

    session
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn separator(title: &str) -> String {
    format!(
        "\n{}\n  {}\n{}\n",
        "━".repeat(70), title, "━".repeat(70)
    )
}

fn msg_label(m: &SimMessage) -> String {
    match m.role {
        "system" => "SYSTEM".to_string(),
        "user" => "USER".to_string(),
        "assistant" => {
            if m.content.is_empty() {
                "ASSISTANT (tool calls)".to_string()
            } else {
                "ASSISTANT".to_string()
            }
        }
        "tool" => format!("TOOL [{}]", m.tool_name.unwrap_or("?")),
        other => other.to_uppercase(),
    }
}

#[test]
fn ai_compress_visual_demo() {
    let session = build_simulated_session();
    let out_dir = Path::new("/tmp/allux_ai_compress_demo");
    let _ = fs::remove_dir_all(out_dir);
    fs::create_dir_all(out_dir).unwrap();

    // ── File 1: Original conversation (what the LLM context looks like) ──
    let mut original = String::new();
    let mut total_original = 0usize;
    original.push_str(&separator("ORIGINAL CONVERSATION — raw messages in LLM context"));
    for (i, m) in session.iter().enumerate() {
        original.push_str(&format!(
            "\n╭── Message {} │ {} │ {} chars ──\n",
            i, msg_label(m), m.content.len()
        ));
        original.push_str(&m.content);
        if !m.content.ends_with('\n') { original.push('\n'); }
        original.push_str("╰──\n");
        total_original += m.content.len();
    }
    original.push_str(&format!(
        "\n── Total: {} messages, {} chars (~{} tokens) ──\n",
        session.len(), total_original, total_original / 4
    ));

    // ── File 2: After algorithmic compression (Standard + Aggressive) ────
    let mut algo = String::new();
    let mut total_standard = 0usize;
    algo.push_str(&separator(
        "AFTER ALGORITHMIC COMPRESSION — Standard on tool outputs",
    ));
    for (i, m) in session.iter().enumerate() {
        let compressed = if m.role == "tool" && m.content.len() > 50 {
            compress_standard(m.tool_name.unwrap_or("unknown"), &m.content)
        } else {
            m.content.clone()
        };
        let saved = m.content.len().saturating_sub(compressed.len());
        let pct = if m.content.len() > 0 {
            format!(" (−{:.0}%)", saved as f64 / m.content.len() as f64 * 100.0)
        } else {
            String::new()
        };
        let tag = if saved > 10 {
            format!(" │ {} → {} chars{}", m.content.len(), compressed.len(), pct)
        } else {
            format!(" │ {} chars", compressed.len())
        };
        algo.push_str(&format!(
            "\n╭── Message {} │ {}{} ──\n",
            i, msg_label(m), tag
        ));
        algo.push_str(&compressed);
        if !compressed.ends_with('\n') { algo.push('\n'); }
        algo.push_str("╰──\n");
        total_standard += compressed.len();
    }
    let std_saved = total_original.saturating_sub(total_standard);
    algo.push_str(&format!(
        "\n── Total: {} chars (~{} tokens) │ saved {} chars (~{} tokens, −{:.1}%) ──\n",
        total_standard,
        total_standard / 4,
        std_saved,
        std_saved / 4,
        std_saved as f64 / total_original as f64 * 100.0
    ));

    // Also show aggressive
    let mut total_aggressive = 0usize;
    algo.push_str(&separator(
        "AFTER AGGRESSIVE COMPRESSION — applied during history compaction",
    ));
    for (i, m) in session.iter().enumerate() {
        let compressed = if m.role == "tool" && m.content.len() > 50 {
            compress_aggressive(m.tool_name.unwrap_or("unknown"), &m.content)
        } else if (m.role == "assistant" || m.role == "user") && m.content.len() > 200 {
            let mut t = collapse_blanks(&m.content);
            t = trim_trailing(&t);
            if t.len() > 8000 { t = smart_truncate(&t, 8000); }
            t
        } else {
            m.content.clone()
        };
        let saved = m.content.len().saturating_sub(compressed.len());
        let pct = if m.content.len() > 0 {
            format!(" (−{:.0}%)", saved as f64 / m.content.len() as f64 * 100.0)
        } else {
            String::new()
        };
        let tag = if saved > 10 {
            format!(" │ {} → {} chars{}", m.content.len(), compressed.len(), pct)
        } else {
            format!(" │ {} chars", compressed.len())
        };
        algo.push_str(&format!(
            "\n╭── Message {} │ {}{} ──\n",
            i, msg_label(m), tag
        ));
        algo.push_str(&compressed);
        if !compressed.ends_with('\n') { algo.push('\n'); }
        algo.push_str("╰──\n");
        total_aggressive += compressed.len();
    }
    let agg_saved = total_original.saturating_sub(total_aggressive);
    algo.push_str(&format!(
        "\n── Total: {} chars (~{} tokens) │ saved {} chars (~{} tokens, −{:.1}%) ──\n",
        total_aggressive,
        total_aggressive / 4,
        agg_saved,
        agg_saved / 4,
        agg_saved as f64 / total_original as f64 * 100.0
    ));

    // ── File 3: AI summarization — what gets sent to and returned from the LLM ──
    let keep_tail = 6;
    let summarize_end = session.len().saturating_sub(keep_tail);
    let messages_to_summarize = &session[1..summarize_end]; // skip system

    let mut ai_file = String::new();
    ai_file.push_str(&separator("AI COMPRESSION — /compress ai"));

    ai_file.push_str("\n┌─────────────────────────────────────────────────────────\n");
    ai_file.push_str("│ STEP 1: Select messages to summarize\n");
    ai_file.push_str("│\n");
    ai_file.push_str(&format!(
        "│  Total messages: {}\n│  Summarizing: messages 1..{} ({} messages)\n│  Keeping intact: last {} messages + system prompt\n",
        session.len(), summarize_end, messages_to_summarize.len(), keep_tail
    ));
    ai_file.push_str("│\n│  Messages selected:\n");
    for (i, m) in messages_to_summarize.iter().enumerate() {
        ai_file.push_str(&format!(
            "│    [{}] {} — {} chars\n",
            i + 1,
            msg_label(m),
            m.content.len()
        ));
    }
    let sum_chars: usize = messages_to_summarize.iter().map(|m| m.content.len()).sum();
    ai_file.push_str(&format!(
        "│\n│  Total to summarize: {} chars (~{} tokens)\n",
        sum_chars, sum_chars / 4
    ));
    ai_file.push_str("└─────────────────────────────────────────────────────────\n");

    ai_file.push_str("\n┌─────────────────────────────────────────────────────────\n");
    ai_file.push_str("│ STEP 2: System prompt sent to LLM for summarization\n");
    ai_file.push_str("└─────────────────────────────────────────────────────────\n\n");
    ai_file.push_str(AI_SYSTEM);
    ai_file.push_str("\n");

    ai_file.push_str("\n┌─────────────────────────────────────────────────────────\n");
    ai_file.push_str("│ STEP 3: User prompt sent to LLM (the conversation to summarize)\n");
    ai_file.push_str("└─────────────────────────────────────────────────────────\n\n");
    let ai_prompt = build_ai_prompt(messages_to_summarize);
    ai_file.push_str(&ai_prompt);

    ai_file.push_str("\n┌─────────────────────────────────────────────────────────\n");
    ai_file.push_str("│ STEP 4: Expected LLM response (simulated)\n");
    ai_file.push_str("└─────────────────────────────────────────────────────────\n\n");

    let simulated_summary = "\
- User asked to add token compression to reduce context window usage
- Read `src/main.rs` (15 lines): entry point loads Config, creates Repl, calls repl.run()
- Read `src/repl/mod.rs` (500/1600 lines): main event loop in run_agentic_loop(), \
  tool dispatch in execute_tool_calls() (~line 1060), history compaction in compact_history() (line 311)
- Grep for \"compact_history\": found at lines 311, 324, 472 in src/repl/mod.rs
- Assistant identified 2 integration points: after tool execution (line ~1085) and in compact_history()
- Assistant proposed 7 compression strategies: ANSI stripping, blank line collapse, \
  JSON compaction, line number compression, deduplication, smart truncation
- User approved implementation
- Created `src/compression/mod.rs` (485 lines) with CompressionLevel enum (Light/Standard/Aggressive)
- Edited `src/main.rs`: added `mod compression;`
- Edited `src/repl/mod.rs`: added compression import, integrated compress_tool_output() \
  before Message::tool_result(), improved compact_history() with two-phase compression
- `cargo check`: compiles with 3 pre-existing warnings (unused variable phase1_saved)
- `cargo test`: 75/75 tests pass
- Standard compression: ~8.5% reduction on tool outputs
- Aggressive compression: ~30% reduction during history compaction
- User then asked to add AI-powered compression mode using LLM summarization";

    ai_file.push_str(simulated_summary);

    ai_file.push_str("\n\n┌─────────────────────────────────────────────────────────\n");
    ai_file.push_str("│ STEP 5: Final history after AI compression\n");
    ai_file.push_str("└─────────────────────────────────────────────────────────\n\n");

    let summary_msg = format!("[AI-compressed context: {} messages summarized]\n\n{}", messages_to_summarize.len(), simulated_summary);
    let mut final_history = String::new();

    // System prompt (kept)
    final_history.push_str(&format!(
        "╭── Message 0 │ SYSTEM │ {} chars ──\n{}\n╰──\n\n",
        session[0].content.len(), session[0].content
    ));

    // AI summary (new)
    final_history.push_str(&format!(
        "╭── Message 1 │ SYSTEM [AI summary] │ {} chars ──\n{}\n╰──\n\n",
        summary_msg.len(), summary_msg
    ));

    // Kept tail messages
    for (j, m) in session[summarize_end..].iter().enumerate() {
        final_history.push_str(&format!(
            "╭── Message {} │ {} │ {} chars ──\n{}\n╰──\n\n",
            j + 2, msg_label(m), m.content.len(), m.content
        ));
    }

    let final_chars: usize = session[0].content.len()
        + summary_msg.len()
        + session[summarize_end..].iter().map(|m| m.content.len()).sum::<usize>();
    let ai_saved = total_original.saturating_sub(final_chars);

    final_history.push_str(&format!(
        "── Total: {} messages (was {}), {} chars (~{} tokens) ──\n\
         ── Saved: {} chars (~{} tokens, −{:.1}%) ──\n",
        2 + session.len() - summarize_end,
        session.len(),
        final_chars,
        final_chars / 4,
        ai_saved,
        ai_saved / 4,
        ai_saved as f64 / total_original as f64 * 100.0
    ));

    ai_file.push_str(&final_history);

    // ── Write all files ──────────────────────────────────────────────────
    fs::write(out_dir.join("1_original_session.txt"), &original).unwrap();
    fs::write(out_dir.join("2_algorithmic_compression.txt"), &algo).unwrap();
    fs::write(out_dir.join("3_ai_compression.txt"), &ai_file).unwrap();

    // ── Summary ──────────────────────────────────────────────────────────
    let summary = format!(
        "ALLUX COMPRESSION DEMO — Simulated Agent Session\n\
         =================================================\n\n\
         Simulated session: {} messages (user asks to add compression,\n\
         agent reads files, edits code, runs tests, reports results)\n\n\
         File                            Size       ~Tokens   Savings\n\
         ─────────────────────────────   ─────────  ───────   ───────\n\
         1_original_session.txt          {:>7} ch  {:>6} tk  (baseline)\n\
         2_algorithmic_compression.txt\n\
           → Standard                    {:>7} ch  {:>6} tk  −{:.1}%\n\
           → Aggressive                  {:>7} ch  {:>6} tk  −{:.1}%\n\
         3_ai_compression.txt\n\
           → After /compress ai          {:>7} ch  {:>6} tk  −{:.1}%\n\n\
         Open each file to see exactly what each stage produces.\n\
         The AI compression file shows the full 5-step pipeline:\n\
           1. Which messages are selected for summarization\n\
           2. The system prompt sent to the LLM\n\
           3. The user prompt (conversation formatted for summarization)\n\
           4. The expected LLM response (simulated summary)\n\
           5. The final history after compression\n",
        session.len(),
        total_original, total_original / 4,
        total_standard, total_standard / 4,
        std_saved as f64 / total_original as f64 * 100.0,
        total_aggressive, total_aggressive / 4,
        agg_saved as f64 / total_original as f64 * 100.0,
        final_chars, final_chars / 4,
        ai_saved as f64 / total_original as f64 * 100.0,
    );
    fs::write(out_dir.join("SUMMARY.txt"), &summary).unwrap();

    // ── Print results ────────────────────────────────────────────────────
    println!("\n{}", summary);

    println!("Files written to /tmp/allux_ai_compress_demo/\n");
    println!("  1_original_session.txt          — Raw messages as the LLM sees them");
    println!("  2_algorithmic_compression.txt   — After Standard + Aggressive compression");
    println!("  3_ai_compression.txt            — Full AI compression pipeline visualization");
    println!("  SUMMARY.txt\n");
}
