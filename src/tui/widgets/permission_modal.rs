//! Permission modal overlay for bash/edit/write approval.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

pub struct PermissionModal<'a> {
    pub title: &'a str,
    pub command: &'a str,
    pub options: &'a [(&'a str, &'a str)],
    pub selected: usize,
}

impl<'a> Widget for PermissionModal<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the modal
        let modal_width = 60u16.min(area.width.saturating_sub(4));
        let modal_height = (6 + self.options.len() as u16).min(area.height.saturating_sub(2));
        let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        // Clear the area behind the modal
        Clear.render(modal_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(100, 149, 237)))
            .title(Span::styled(
                format!(" {} ", self.title),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        let mut lines: Vec<Line> = Vec::new();

        // Command display
        let cmd_short = if self.command.len() > 50 {
            format!("{}...", &self.command[..47])
        } else {
            self.command.to_string()
        };
        lines.push(Line::from(vec![
            Span::styled("  $ ", Style::default().fg(Color::Rgb(100, 200, 100))),
            Span::styled(
                cmd_short,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));

        // Options
        for (i, (key, desc)) in self.options.iter().enumerate() {
            let is_selected = i == self.selected;
            let key_style = if is_selected {
                Style::default()
                    .fg(Color::Rgb(100, 149, 237))
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default()
                    .fg(Color::Rgb(100, 149, 237))
                    .add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  [{key}]"), key_style),
                Span::styled(format!("  {desc}"), Style::default().fg(Color::Rgb(180, 180, 190))),
            ]));
        }

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        paragraph.render(inner, buf);
    }
}

/// Standard bash permission options.
pub fn bash_options() -> Vec<(&'static str, &'static str)> {
    vec![
        ("y", "Allow this once"),
        ("s", "Allow for this session"),
        ("a", "Allow all commands of this family (session)"),
        ("w", "Allow in this workspace (saved)"),
        ("g", "Allow globally (saved)"),
        ("n", "Reject"),
    ]
}

/// Standard file edit/write permission options.
pub fn file_options() -> Vec<(&'static str, &'static str)> {
    vec![
        ("y", "Allow this edit"),
        ("n", "Reject"),
    ]
}
