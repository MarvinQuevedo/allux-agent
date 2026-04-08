//! TUI module: full terminal user interface using ratatui.
//!
//! Replaces the old REPL with a scrollable, interactive interface.

pub mod app;
pub mod event;
pub mod widgets;

use std::io;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    Terminal,
};
use tokio::sync::mpsc;
// tui_textarea is used via widgets::input_area

use crate::config::Config;
use crate::monitor::SharedMetrics;

use self::app::{AgentPhase, App};
use self::event::{spawn_event_reader, AppEvent};
use self::widgets::{
    chat_panel::{ChatMessage, ChatPanel},
    input_area,
    status_bar::StatusBar,
};

/// Initialize the terminal and run the TUI event loop.
pub async fn run(config: Config, workspace_root: PathBuf, metrics: SharedMetrics) -> Result<()> {
    // Setup terminal
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Install panic hook to restore terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));

    // Create event channel
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AppEvent>();

    // Spawn crossterm event reader (sends Key/Mouse/Tick events)
    let reader_rx = spawn_event_reader();
    // Forward reader events into our unified channel
    let fwd_tx = event_tx.clone();
    tokio::spawn(async move {
        let mut reader_rx = reader_rx;
        while let Some(evt) = reader_rx.recv().await {
            if fwd_tx.send(evt).is_err() {
                break;
            }
        }
    });

    // Create app
    let mut app = App::new(config, workspace_root, metrics, event_tx.clone());

    // Welcome message
    app.chat_messages.push(ChatMessage::System(format!(
        "Welcome to Allux v{} \u{2022} model: {} \u{2022} /help for commands",
        env!("CARGO_PKG_VERSION"),
        app.client.model
    )));

    // Create text area for input
    let mut textarea = input_area::new_textarea();

    // Main event loop
    loop {
        // Draw
        terminal.draw(|frame| {
            let area = frame.area();

            // Layout: status bar (1) | chat panel (fill) | input (3)
            let chunks = Layout::vertical([
                Constraint::Length(1),   // Status bar
                Constraint::Min(5),     // Chat panel
                Constraint::Length(3),   // Input area
            ])
            .split(area);

            // Status bar
            let status = StatusBar { app: &app };
            frame.render_widget(status, chunks[0]);

            // Chat panel
            let is_streaming = matches!(
                app.phase,
                AgentPhase::WaitingForLlm | AgentPhase::ExecutingTools
            );
            let chat = ChatPanel {
                messages: &app.chat_messages,
                streaming_text: &app.streaming_text,
                is_streaming,
                spinner_frame: app.spinner_frame,
                scroll_offset: app.scroll_offset,
            };
            frame.render_widget(chat, chunks[1]);

            // Input area
            frame.render_widget(&textarea, chunks[2]);
        })?;

        if app.should_quit {
            break;
        }

        // Wait for next event
        let Some(evt) = event_rx.recv().await else {
            break;
        };

        match evt {
            AppEvent::Key(key) => {
                // Ignore release events
                if key.kind == KeyEventKind::Release {
                    continue;
                }

                // Global shortcuts
                match (key.code, key.modifiers) {
                    // Ctrl+D on empty input: quit
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                        let text: String = textarea.lines().join("");
                        if text.is_empty() {
                            app.should_quit = true;
                            continue;
                        }
                    }
                    // PageUp / PageDown: scroll
                    (KeyCode::PageUp, _) => {
                        app.scroll_up(10);
                        continue;
                    }
                    (KeyCode::PageDown, _) => {
                        app.scroll_down(10);
                        continue;
                    }
                    // Escape: clear permission modal or scroll to bottom
                    (KeyCode::Esc, _) => {
                        app.permission_prompt = None;
                        app.scroll_to_bottom();
                        continue;
                    }
                    _ => {}
                }

                // If we're idle or there's a permission prompt, handle input
                if app.phase == AgentPhase::Idle {
                    match input_area::handle_key(&mut textarea, key) {
                        input_area::InputAction::Submit(text) => {
                            // Check for slash commands first
                            if !app.handle_slash_command(&text) {
                                app.submit_user_input(text);
                            }
                        }
                        input_area::InputAction::Quit => {
                            app.should_quit = true;
                        }
                        input_area::InputAction::Consumed => {}
                    }
                } else {
                    // During streaming/tool execution, only allow scroll and Ctrl+C
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            // Cancel current operation
                            app.phase = AgentPhase::Idle;
                            app.streaming_text.clear();
                            app.chat_messages
                                .push(ChatMessage::System("Cancelled.".into()));
                        }
                        _ => {}
                    }
                }
            }

            AppEvent::Mouse(mouse) => {
                use crossterm::event::MouseEventKind;
                match mouse.kind {
                    MouseEventKind::ScrollUp => app.scroll_up(3),
                    MouseEventKind::ScrollDown => app.scroll_down(3),
                    _ => {}
                }
            }

            AppEvent::Tick => {
                app.on_tick();
            }

            AppEvent::StreamChunk(text) => {
                app.on_stream_chunk(text);
            }

            AppEvent::StreamDone {
                content,
                prompt_tokens,
                completion_tokens,
            } => {
                app.on_stream_done(content, prompt_tokens, completion_tokens);
            }

            AppEvent::StreamToolCalls {
                calls,
                text,
                prompt_tokens,
                completion_tokens,
            } => {
                app.on_stream_tool_calls(calls, text, prompt_tokens, completion_tokens);
            }

            AppEvent::StreamError(err) => {
                app.on_stream_error(err);
            }

            AppEvent::ToolResult { name, output } => {
                app.on_tool_result(name, output);
            }

            AppEvent::Resize(_, _) => {
                // ratatui handles resize automatically on next draw()
            }
        }
    }

    // Restore terminal
    terminal::disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    // Auto-save session
    let has_user_msgs = app.history.iter().any(|m| m.role == "user");
    if has_user_msgs {
        if let Ok(path) = crate::session::save(
            &app.history,
            &app.client.model,
            &app.workspace_root,
            app.session_id.as_deref(),
        ) {
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
            println!("Session auto-saved (id: {id})");
        }
    }

    Ok(())
}
