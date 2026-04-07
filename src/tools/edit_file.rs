use anyhow::Result;
use colored::Colorize;

pub fn run_edit_file(path: &str, old_str: &str, new_str: &str) -> Result<String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Cannot read '{path}': {e}"))?;

    let count = content.matches(old_str).count();
    if count == 0 {
        anyhow::bail!(
            "old_str not found in '{path}'. Make sure it matches exactly (including whitespace)."
        );
    }
    if count > 1 {
        anyhow::bail!(
            "old_str found {count} times in '{path}'. Provide more context to make it unique."
        );
    }

    // Print visual diff before applying
    print_diff(path, old_str, new_str);

    let new_content = content.replacen(old_str, new_str, 1);
    std::fs::write(path, &new_content)
        .map_err(|e| anyhow::anyhow!("Cannot write '{path}': {e}"))?;

    Ok(format!("Edited '{path}' successfully."))
}

/// Print a colored inline diff for the edit operation.
fn print_diff(path: &str, old_str: &str, new_str: &str) {
    let border = "│".truecolor(60, 60, 70);
    let top = format!("╭─ {} {}", "edit".truecolor(140, 140, 160), path.cyan());
    let bottom = format!("╰{}", "─".repeat(42)).truecolor(60, 60, 70);

    println!("    {}", top.truecolor(60, 60, 70));

    let max_lines = 6;
    let old_lines: Vec<&str> = old_str.lines().collect();
    let new_lines: Vec<&str> = new_str.lines().collect();

    for (i, line) in old_lines.iter().enumerate() {
        if i >= max_lines {
            println!("    {} {}", border, "  … (more lines)".dimmed());
            break;
        }
        let display = if line.len() > 60 { &line[..60] } else { line };
        println!("    {} {}", border, format!("- {display}").red());
    }
    for (i, line) in new_lines.iter().enumerate() {
        if i >= max_lines {
            println!("    {} {}", border, "  … (more lines)".dimmed());
            break;
        }
        let display = if line.len() > 60 { &line[..60] } else { line };
        println!("    {} {}", border, format!("+ {display}").green());
    }

    println!("    {}", bottom);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_edit_replaces_once() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "fn main() {{\n    println!(\"hello\");\n}}").unwrap();
        let path = f.path().to_str().unwrap();

        run_edit_file(path, "println!(\"hello\")", "println!(\"world\")").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("println!(\"world\")"));
        assert!(!content.contains("println!(\"hello\")"));
    }

    #[test]
    fn test_edit_fails_if_not_found() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "hello world").unwrap();
        let result = run_edit_file(f.path().to_str().unwrap(), "NONEXISTENT", "x");
        assert!(result.is_err());
    }

    #[test]
    fn test_edit_fails_if_ambiguous() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "foo\nfoo").unwrap();
        let result = run_edit_file(f.path().to_str().unwrap(), "foo", "bar");
        assert!(result.is_err());
    }
}
