//! Unified event system for the TUI application.
//!
//! Merges crossterm input events, periodic ticks (for animations/metrics),
//! and async stream events from Ollama into a single channel.

use std::time::Duration;

use crossterm::event::{self, Event as CtEvent, KeyEvent, MouseEvent};
use tokio::sync::mpsc;

use crate::ollama::client::StreamEvent;
use crate::ollama::types::{LlmResponse, ToolCallItem};

/// All events the TUI event loop can process.
#[derive(Debug)]
pub enum AppEvent {
    /// A keyboard event.
    Key(KeyEvent),
    /// A mouse event (click, scroll).
    Mouse(MouseEvent),
    /// Periodic tick for animations and metric refresh (~200ms).
    Tick,
    /// A text delta from the LLM streaming response.
    StreamChunk(String),
    /// LLM finished responding with text.
    StreamDone {
        content: String,
        prompt_tokens: u32,
        completion_tokens: u32,
    },
    /// LLM wants to call tools.
    StreamToolCalls {
        calls: Vec<ToolCallItem>,
        text: String,
        prompt_tokens: u32,
        completion_tokens: u32,
    },
    /// LLM or network error.
    StreamError(String),
    /// A tool execution finished.
    ToolResult {
        name: String,
        output: String,
    },
    /// Terminal resize.
    Resize(u16, u16),
}

/// Spawns the crossterm event reader + tick generator, returns the receiver.
pub fn spawn_event_reader() -> mpsc::UnboundedReceiver<AppEvent> {
    let (tx, rx) = mpsc::unbounded_channel();

    // Crossterm events + tick in one task
    let event_tx = tx.clone();
    tokio::spawn(async move {
        let tick_rate = Duration::from_millis(200);
        loop {
            // Poll crossterm with tick_rate timeout
            if crossterm::event::poll(tick_rate).unwrap_or(false) {
                if let Ok(evt) = event::read() {
                    let app_evt = match evt {
                        CtEvent::Key(k) => Some(AppEvent::Key(k)),
                        CtEvent::Mouse(m) => Some(AppEvent::Mouse(m)),
                        CtEvent::Resize(w, h) => Some(AppEvent::Resize(w, h)),
                        _ => None,
                    };
                    if let Some(e) = app_evt {
                        if event_tx.send(e).is_err() {
                            break;
                        }
                    }
                }
            } else {
                // Tick on timeout
                if event_tx.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        }
    });

    rx
}

/// Forward StreamEvents from the Ollama channel into AppEvents.
pub fn forward_stream_events(
    mut stream_rx: mpsc::UnboundedReceiver<StreamEvent>,
    app_tx: mpsc::UnboundedSender<AppEvent>,
) {
    tokio::spawn(async move {
        while let Some(evt) = stream_rx.recv().await {
            let app_evt = match evt {
                StreamEvent::TextDelta(s) => AppEvent::StreamChunk(s),
                StreamEvent::Done(LlmResponse::Text { content, stats }) => {
                    AppEvent::StreamDone {
                        content,
                        prompt_tokens: stats.prompt_tokens,
                        completion_tokens: stats.completion_tokens,
                    }
                }
                StreamEvent::Done(LlmResponse::ToolCalls {
                    calls, text, stats,
                }) => AppEvent::StreamToolCalls {
                    calls,
                    text,
                    prompt_tokens: stats.prompt_tokens,
                    completion_tokens: stats.completion_tokens,
                },
                StreamEvent::Error(e) => AppEvent::StreamError(e),
            };
            if app_tx.send(app_evt).is_err() {
                break;
            }
        }
    });
}
