//! Markdown to ratatui `Line` renderer.
//!
//! Converts pulldown-cmark events into styled ratatui Spans/Lines
//! for display in the chat panel.

use pulldown_cmark::{
    CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd, TextMergeStream,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Convert markdown source into styled ratatui Lines.
pub fn to_ratatui_lines(src: &str) -> Vec<Line<'static>> {
    let src = src.trim_end();
    if src.is_empty() {
        return vec![];
    }

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);

    let parser = TextMergeStream::new(Parser::new_ext(src, opts));

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();

    let mut inline_stack: Vec<InlineStyle> = Vec::new();
    let mut in_heading: Option<HeadingLevel> = None;
    let mut in_code_block = false;
    let mut code_block_lang = String::new();
    let mut in_link = false;
    let mut list_depth: usize = 0;
    let mut list_ordered: Vec<(bool, u64)> = Vec::new();
    let mut blockquote_depth: u32 = 0;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_line(&mut lines, &mut current_spans);
                lines.push(Line::from(""));
                in_heading = Some(level);
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_line(&mut lines, &mut current_spans);
                in_heading = None;
            }

            Event::Start(Tag::Paragraph) => {
                flush_line(&mut lines, &mut current_spans);
                if !in_code_block {
                    // Don't add extra blank line inside code blocks
                }
            }
            Event::End(TagEnd::Paragraph) => {
                flush_line(&mut lines, &mut current_spans);
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                flush_line(&mut lines, &mut current_spans);
                in_code_block = true;
                code_block_lang = match &kind {
                    CodeBlockKind::Fenced(lang) => {
                        lang.split_whitespace().next().unwrap_or("").to_string()
                    }
                    _ => String::new(),
                };
                // Top border of code block
                let label = if code_block_lang.is_empty() {
                    String::new()
                } else {
                    format!(" {} ", code_block_lang)
                };
                let border_len = 42usize.saturating_sub(label.len());
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  \u{256D}\u{2500}{label}"),
                        Style::default().fg(Color::Rgb(60, 60, 70)),
                    ),
                    Span::styled(
                        "\u{2500}".repeat(border_len),
                        Style::default().fg(Color::Rgb(60, 60, 70)),
                    ),
                ]));
            }
            Event::End(TagEnd::CodeBlock) => {
                flush_line(&mut lines, &mut current_spans);
                in_code_block = false;
                lines.push(Line::from(Span::styled(
                    format!("  \u{2570}{}", "\u{2500}".repeat(42)),
                    Style::default().fg(Color::Rgb(60, 60, 70)),
                )));
            }

            Event::Start(Tag::List(start)) => {
                flush_line(&mut lines, &mut current_spans);
                let ordered = start.is_some();
                let num = start.unwrap_or(1);
                list_ordered.push((ordered, num));
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                flush_line(&mut lines, &mut current_spans);
                list_ordered.pop();
                list_depth = list_depth.saturating_sub(1);
            }

            Event::Start(Tag::Item) => {
                flush_line(&mut lines, &mut current_spans);
                let indent = "  ".repeat(list_depth.saturating_sub(1));
                if let Some((ordered, num)) = list_ordered.last_mut() {
                    if *ordered {
                        current_spans.push(Span::styled(
                            format!("{indent}{}. ", num),
                            Style::default().fg(Color::Cyan),
                        ));
                        *num += 1;
                    } else {
                        current_spans.push(Span::styled(
                            format!("{indent}\u{2022} "),
                            Style::default().fg(Color::Cyan),
                        ));
                    }
                }
            }
            Event::End(TagEnd::Item) => {
                flush_line(&mut lines, &mut current_spans);
            }

            Event::Start(Tag::Strong) => inline_stack.push(InlineStyle::Bold),
            Event::End(TagEnd::Strong) => { inline_stack.pop(); }
            Event::Start(Tag::Emphasis) => inline_stack.push(InlineStyle::Italic),
            Event::End(TagEnd::Emphasis) => { inline_stack.pop(); }
            Event::Start(Tag::Strikethrough) => inline_stack.push(InlineStyle::Dim),
            Event::End(TagEnd::Strikethrough) => { inline_stack.pop(); }

            Event::Start(Tag::Link { .. }) => { in_link = true; }
            Event::End(TagEnd::Link) => { in_link = false; }

            Event::Start(Tag::BlockQuote(_)) => { blockquote_depth += 1; }
            Event::End(TagEnd::BlockQuote(_)) => {
                blockquote_depth = blockquote_depth.saturating_sub(1);
            }

            Event::Text(text) => {
                let style = if in_code_block {
                    // Code block content
                    for code_line in text.split('\n') {
                        if !current_spans.is_empty() {
                            flush_line(&mut lines, &mut current_spans);
                        }
                        current_spans.push(Span::styled(
                            format!("  \u{2502} {code_line}"),
                            Style::default().fg(Color::Rgb(200, 200, 210)),
                        ));
                    }
                    continue;
                } else if let Some(level) = in_heading {
                    let (prefix, color) = match level {
                        HeadingLevel::H1 => (
                            "\u{258C} ",
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                        HeadingLevel::H2 => (
                            "\u{258E} ",
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        _ => ("", Style::default().add_modifier(Modifier::BOLD)),
                    };
                    if !prefix.is_empty() {
                        current_spans.push(Span::styled(
                            prefix.to_string(),
                            Style::default().fg(Color::Rgb(100, 149, 237)),
                        ));
                    }
                    current_spans.push(Span::styled(text.to_string(), color));
                    continue;
                } else {
                    compute_inline_style(&inline_stack, in_link, blockquote_depth)
                };

                current_spans.push(Span::styled(text.to_string(), style));
            }

            Event::Code(code) => {
                current_spans.push(Span::styled(
                    "`".to_string(),
                    Style::default().fg(Color::Rgb(90, 90, 100)),
                ));
                current_spans.push(Span::styled(
                    code.to_string(),
                    Style::default().fg(Color::Rgb(220, 180, 100)),
                ));
                current_spans.push(Span::styled(
                    "`".to_string(),
                    Style::default().fg(Color::Rgb(90, 90, 100)),
                ));
            }

            Event::SoftBreak => {
                current_spans.push(Span::raw(" "));
            }
            Event::HardBreak => {
                flush_line(&mut lines, &mut current_spans);
            }

            Event::Rule => {
                flush_line(&mut lines, &mut current_spans);
                lines.push(Line::from(Span::styled(
                    "\u{2500}".repeat(40),
                    Style::default().fg(Color::Rgb(50, 60, 80)),
                )));
            }

            Event::TaskListMarker(checked) => {
                let mark = if checked { "\u{2611}" } else { "\u{2610}" };
                current_spans.push(Span::styled(
                    format!("{mark} "),
                    Style::default().fg(Color::Cyan),
                ));
            }

            // Skip metadata, footnotes, etc.
            _ => {}
        }
    }

    flush_line(&mut lines, &mut current_spans);
    lines
}

#[derive(Clone, Copy)]
enum InlineStyle {
    Bold,
    Italic,
    Dim,
}

fn compute_inline_style(stack: &[InlineStyle], in_link: bool, blockquote_depth: u32) -> Style {
    let mut style = Style::default().fg(Color::Rgb(220, 220, 230));

    for s in stack {
        match s {
            InlineStyle::Bold => style = style.add_modifier(Modifier::BOLD),
            InlineStyle::Italic => style = style.add_modifier(Modifier::ITALIC),
            InlineStyle::Dim => style = style.add_modifier(Modifier::DIM),
        }
    }

    if in_link {
        style = style.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED);
    }

    if blockquote_depth > 0 {
        style = style.add_modifier(Modifier::DIM);
    }

    style
}

fn flush_line(lines: &mut Vec<Line<'static>>, spans: &mut Vec<Span<'static>>) {
    if !spans.is_empty() {
        lines.push(Line::from(std::mem::take(spans)));
    }
}
