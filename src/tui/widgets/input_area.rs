//! Input area widget using tui-textarea.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders},
};
use tui_textarea::TextArea;

/// Create a new TextArea with Allux styling.
pub fn new_textarea<'a>() -> TextArea<'a> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(Style::default());
    ta.set_cursor_style(
        Style::default()
            .fg(Color::Rgb(100, 149, 237))
            .add_modifier(Modifier::REVERSED),
    );
    ta.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(60, 70, 90)))
            .title(Span::styled(
                " \u{276F} Input (Enter to send, Ctrl+D exit) ",
                Style::default()
                    .fg(Color::Rgb(100, 149, 237))
                    .add_modifier(Modifier::BOLD),
            )),
    );
    ta.set_placeholder_text("Type a message or /help...");
    ta.set_placeholder_style(Style::default().fg(Color::Rgb(80, 80, 100)));
    ta
}

/// Possible input actions from key events.
pub enum InputAction {
    /// User submitted text.
    Submit(String),
    /// User wants to quit (Ctrl+D on empty).
    Quit,
    /// Key was consumed by the textarea (no action needed).
    Consumed,
}

/// Process a key event for the input textarea.
pub fn handle_key(textarea: &mut TextArea, key: KeyEvent) -> InputAction {
    match (key.code, key.modifiers) {
        // Enter: submit the text
        (KeyCode::Enter, KeyModifiers::NONE) => {
            let text: String = textarea.lines().join("\n").trim().to_string();
            // Clear the textarea
            textarea.select_all();
            textarea.cut();
            if text.is_empty() {
                InputAction::Consumed
            } else {
                InputAction::Submit(text)
            }
        }
        // Ctrl+D on empty: quit
        (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
            let text: String = textarea.lines().join("");
            if text.is_empty() {
                InputAction::Quit
            } else {
                textarea.input(key);
                InputAction::Consumed
            }
        }
        // Ctrl+C: clear input
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            textarea.select_all();
            textarea.cut();
            InputAction::Consumed
        }
        // All other keys: let textarea handle it
        _ => {
            textarea.input(key);
            InputAction::Consumed
        }
    }
}
