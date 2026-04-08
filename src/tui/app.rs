//! Application state and logic for the TUI.

use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::compression::CompressionMode;
use crate::config::Config;
use crate::monitor::SharedMetrics;
use crate::ollama::client::OllamaClient;
use crate::ollama::types::{ChatOptions, Message, ToolCallItem};
use crate::permissions::{Decision, PermissionStore};
use crate::tools;
use crate::workspace;

use super::event::{self, AppEvent};
use super::widgets::chat_panel::ChatMessage;

// ── Constants ───────────────────────────────────────────────────────────────

const MAX_TOOL_ROUNDS: usize = 10000;

const SYSTEM_PROMPT: &str = "\
You are Allux, a local code assistant powered by Ollama. \
You help with software engineering tasks. \
You have access to tools: read_file, write_file, edit_file, glob, grep, tree, bash. \
Use them to explore and modify the codebase when needed. \
Always prefer reading files before editing them. \
Be concise and precise.";

const SYSTEM_PROMPT_CHAT_ONLY: &str = "\
You are Allux, a local code assistant. This session is in chat-only mode: \
Ollama does not expose tool calling for this model, so you cannot invoke tools yourself. \
The user can load disk context with slash commands. \
For shell steps, put each command in a fenced block with language bash or sh. \
Be concise.";

// ── Session mode ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionMode {
    Chat,
    Agent,
    Plan,
}

impl SessionMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Agent => "agent",
            Self::Plan => "plan",
        }
    }
}

// ── Agent phase (state machine) ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AgentPhase {
    /// Waiting for user input.
    Idle,
    /// Waiting for the LLM to respond.
    WaitingForLlm,
    /// Asking the user for permission (bash, edit, write).
    #[allow(dead_code)]
    WaitingForPermission {
        tool_name: String,
        command: String,
        /// Index in the current tool calls batch.
        call_index: usize,
        /// The full batch of tool calls.
        pending_calls: Vec<ToolCallItem>,
        /// Results accumulated so far.
        results: Vec<Message>,
    },
    /// Executing tool calls.
    ExecutingTools,
}

// ── Permission modal state ──────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PermissionPrompt {
    pub tool_name: String,
    pub command: String,
    pub detail: String,
    pub options: Vec<(&'static str, &'static str)>,
}

// ── App ─────────────────────────────────────────────────────────────────────

pub struct App {
    // ── Core state ──
    pub client: OllamaClient,
    pub history: Vec<Message>,
    pub config: Config,
    pub workspace_root: PathBuf,
    pub mode: SessionMode,
    pub model_supports_tools: bool,
    pub compression_mode: CompressionMode,
    pub permissions: PermissionStore,
    pub metrics: SharedMetrics,
    pub session_id: Option<String>,

    // ── UI state ──
    pub chat_messages: Vec<ChatMessage>,
    pub scroll_offset: usize,
    pub phase: AgentPhase,
    pub spinner_frame: usize,
    pub should_quit: bool,
    pub status_message: Option<String>,
    pub permission_prompt: Option<PermissionPrompt>,

    // ── Streaming ──
    pub streaming_text: String,
    pub current_tool_round: usize,

    // ── Event channel (for sending events from tool execution, etc.) ──
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
}

impl App {
    pub fn new(
        config: Config,
        workspace_root: PathBuf,
        metrics: SharedMetrics,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Self {
        let client = OllamaClient::new(&config.ollama_url, &config.model);
        let compression_mode = CompressionMode::from_str_loose(&config.compression_mode)
            .unwrap_or(CompressionMode::Auto);

        let system_prompt = Self::compose_system_prompt(&workspace_root, &SessionMode::Agent, true);
        let history = vec![Message::system(system_prompt)];

        Self {
            client,
            history,
            config,
            workspace_root: workspace_root.clone(),
            mode: SessionMode::Agent,
            model_supports_tools: true,
            compression_mode,
            permissions: PermissionStore::new(&workspace_root),
            metrics,
            session_id: None,

            chat_messages: Vec::new(),
            scroll_offset: 0,
            phase: AgentPhase::Idle,
            spinner_frame: 0,
            should_quit: false,
            status_message: None,
            permission_prompt: None,

            streaming_text: String::new(),
            current_tool_round: 0,

            event_tx,
        }
    }

    // ── System prompt ───────────────────────────────────────────────────────

    fn compose_system_prompt(
        root: &std::path::Path,
        mode: &SessionMode,
        model_supports_tools: bool,
    ) -> String {
        let intro = match mode {
            SessionMode::Chat => SYSTEM_PROMPT_CHAT_ONLY,
            SessionMode::Agent | SessionMode::Plan => {
                if model_supports_tools {
                    SYSTEM_PROMPT
                } else {
                    SYSTEM_PROMPT_CHAT_ONLY
                }
            }
        };
        format!("{intro}\n\n{}", workspace::snapshot(root))
    }

    pub fn rebuild_system_prompt(&mut self) {
        let content = Self::compose_system_prompt(
            &self.workspace_root,
            &self.mode,
            self.model_supports_tools,
        );
        if let Some(first) = self.history.first_mut() {
            if first.role == "system" {
                first.content = content;
                return;
            }
        }
        self.history.insert(0, Message::system(content));
    }

    // ── Context tracking ────────────────────────────────────────────────────

    pub fn history_char_count(&self) -> usize {
        self.history.iter().map(|m| m.content.len()).sum()
    }

    pub fn context_pct(&self) -> f64 {
        let budget = (self.config.context_size as usize) * 3;
        if budget == 0 {
            return 0.0;
        }
        ((self.history_char_count() as f64 / budget as f64) * 100.0).min(100.0)
    }

    // ── Submit user input ───────────────────────────────────────────────────

    pub fn submit_user_input(&mut self, input: String) {
        if input.is_empty() {
            return;
        }

        // Add to chat display
        self.chat_messages.push(ChatMessage::User(input.clone()));

        // Add to LLM history
        self.history.push(Message::user(&input));

        // Start the LLM call
        self.start_llm_call();
    }

    pub fn start_llm_call(&mut self) {
        self.phase = AgentPhase::WaitingForLlm;
        self.streaming_text.clear();
        self.spinner_frame = 0;

        let tools_defs = tools::all_definitions();
        let use_tools = matches!(self.mode, SessionMode::Agent | SessionMode::Plan)
            && self.model_supports_tools;

        let client = self.client.clone();
        let history = self.history.clone();
        let options = ChatOptions {
            temperature: None,
            num_ctx: Some(self.config.context_size),
        };
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            let (stream_tx, stream_rx) = mpsc::unbounded_channel();

            // Forward stream events to the app event channel
            event::forward_stream_events(stream_rx, event_tx);

            if use_tools {
                client
                    .chat_streaming(&history, Some(&tools_defs), Some(options), stream_tx)
                    .await;
            } else {
                client
                    .chat_streaming(&history, None, Some(options), stream_tx)
                    .await;
            }
        });
    }

    // ── Handle stream events ────────────────────────────────────────────────

    pub fn on_stream_chunk(&mut self, text: String) {
        self.streaming_text.push_str(&text);
        // Auto-scroll to bottom
        self.scroll_to_bottom();
    }

    pub fn on_stream_done(&mut self, content: String, prompt_tokens: u32, completion_tokens: u32) {
        self.phase = AgentPhase::Idle;
        self.history.push(Message::assistant(&content));
        self.chat_messages.push(ChatMessage::Assistant(content));
        self.streaming_text.clear();
        self.status_message = Some(format!(
            "tokens: {} in · {} out",
            prompt_tokens, completion_tokens
        ));
        self.scroll_to_bottom();
    }

    pub fn on_stream_tool_calls(
        &mut self,
        calls: Vec<ToolCallItem>,
        text: String,
        _prompt_tokens: u32,
        _completion_tokens: u32,
    ) {
        self.streaming_text.clear();

        // Show tool calls in chat
        let names: Vec<String> = calls
            .iter()
            .map(|c| {
                let detail = tool_call_detail(&c.function.name, &c.function.arguments);
                if detail.is_empty() {
                    format!("  {} {}", "\u{26A1}", c.function.name)
                } else {
                    format!("  {} {} {}", "\u{26A1}", c.function.name, detail)
                }
            })
            .collect();
        self.chat_messages
            .push(ChatMessage::ToolHeader(names.join("\n")));

        // Store in history
        self.history
            .push(Message::assistant_tool_calls(calls.clone(), &text));

        // Execute tool calls
        self.execute_tool_calls(calls);
    }

    pub fn on_stream_error(&mut self, error: String) {
        self.phase = AgentPhase::Idle;
        self.streaming_text.clear();

        // Check if model doesn't support tools
        if self.model_supports_tools && error.contains("does not support tools") {
            self.model_supports_tools = false;
            self.rebuild_system_prompt();
            self.chat_messages.push(ChatMessage::System(format!(
                "Model '{}' does not support tools. Falling back to chat mode.",
                self.client.model
            )));
            // Retry without tools
            self.start_llm_call();
            return;
        }

        self.chat_messages
            .push(ChatMessage::Error(format!("Error: {}", error)));
        // Remove the failed user message from history
        if self.history.last().map(|m| m.role.as_str()) == Some("user") {
            self.history.pop();
        }
    }

    // ── Tool execution ──────────────────────────────────────────────────────

    fn execute_tool_calls(&mut self, calls: Vec<ToolCallItem>) {
        self.phase = AgentPhase::ExecutingTools;
        self.current_tool_round += 1;

        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            for call in &calls {
                let name = &call.function.name;
                let args = &call.function.arguments;

                // Execute tool (permissions are checked in the TUI layer for bash/edit)
                let output = match tools::dispatch(name, args).await {
                    Ok(out) => out,
                    Err(e) => format!("Error executing {name}: {e}"),
                };

                let _ = event_tx.send(AppEvent::ToolResult {
                    name: name.clone(),
                    output: output.clone(),
                });
            }
        });
    }

    pub fn on_tool_result(&mut self, name: String, output: String) {
        // Add tool result to history
        self.history
            .push(Message::tool_result(name.clone(), output.clone()));

        // Show compact result in chat
        let preview = if output.len() > 200 {
            format!("{}...", &output[..200])
        } else {
            output
        };
        self.chat_messages
            .push(ChatMessage::ToolResult(name, preview));

        // Check if this was the last tool result for this batch
        // For simplicity, we'll start a new LLM call after each tool result
        // In production, we'd batch all results first
        // Start new LLM round
        if self.current_tool_round < MAX_TOOL_ROUNDS {
            self.start_llm_call();
        }
        self.scroll_to_bottom();
    }

    // ── Permission handling ─────────────────────────────────────────────────

    pub fn handle_permission_response(&mut self, decision: Decision) {
        if let Some(prompt) = self.permission_prompt.take() {
            match decision {
                Decision::AllowOnce => {}
                Decision::AllowSession => {
                    self.permissions.grant_session(&prompt.command);
                }
                Decision::AllowFamily => {
                    self.permissions.grant_family(&prompt.command);
                }
                Decision::AllowWorkspace => {
                    self.permissions.grant_workspace(&prompt.command);
                }
                Decision::AllowGlobal => {
                    self.permissions.grant_global(&prompt.command);
                }
                Decision::Deny => {
                    self.chat_messages
                        .push(ChatMessage::System("Permission denied.".into()));
                    return;
                }
            }
        }
    }

    // ── Scroll ──────────────────────────────────────────────────────────────

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    // ── Slash commands ──────────────────────────────────────────────────────

    pub fn handle_slash_command(&mut self, input: &str) -> bool {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return false;
        }

        let (cmd, rest) = match trimmed.find(char::is_whitespace) {
            Some(pos) => (&trimmed[..pos], trimmed[pos..].trim()),
            None => (trimmed, ""),
        };

        match cmd {
            "/quit" | "/exit" | "/q" => {
                self.should_quit = true;
            }
            "/clear" => {
                self.history = vec![Message::system(Self::compose_system_prompt(
                    &self.workspace_root,
                    &self.mode,
                    self.model_supports_tools,
                ))];
                self.chat_messages.clear();
                self.chat_messages.push(ChatMessage::System(
                    "Conversation cleared.".into(),
                ));
            }
            "/model" => {
                if rest.is_empty() {
                    self.chat_messages.push(ChatMessage::System(format!(
                        "Current model: {}",
                        self.client.model
                    )));
                } else if rest == "list" {
                    self.chat_messages
                        .push(ChatMessage::System("Use /model <name> to switch models.".into()));
                } else {
                    self.config.model = rest.to_string();
                    self.client.model = rest.to_string();
                    self.model_supports_tools = true;
                    self.rebuild_system_prompt();
                    let _ = self.config.save();
                    self.chat_messages.push(ChatMessage::System(format!(
                        "Model set to: {}",
                        rest
                    )));
                }
            }
            "/mode" => {
                if rest.is_empty() {
                    self.chat_messages.push(ChatMessage::System(format!(
                        "Current mode: {}",
                        self.mode.label()
                    )));
                } else {
                    let new_mode = match rest {
                        "chat" => Some(SessionMode::Chat),
                        "agent" => Some(SessionMode::Agent),
                        "plan" => Some(SessionMode::Plan),
                        _ => None,
                    };
                    if let Some(m) = new_mode {
                        self.mode = m;
                        self.rebuild_system_prompt();
                        self.chat_messages.push(ChatMessage::System(format!(
                            "Mode set to: {}",
                            self.mode.label()
                        )));
                    }
                }
            }
            "/help" => {
                self.chat_messages.push(ChatMessage::System(
                    "/clear      Clear conversation\n\
                     /model      Show current model\n\
                     /model list List available models\n\
                     /model NAME Switch model\n\
                     /mode       Show current mode\n\
                     /mode chat|agent|plan  Switch mode\n\
                     /quit       Exit Allux\n\
                     \n\
                     Scroll: PageUp/PageDown, mouse wheel\n\
                     Submit: Enter | Ctrl+D exit"
                        .into(),
                ));
            }
            _ => {
                self.chat_messages.push(ChatMessage::System(format!(
                    "Unknown command: {cmd}. Type /help for available commands."
                )));
            }
        }

        self.scroll_to_bottom();
        true
    }

    // ── Tick ────────────────────────────────────────────────────────────────

    pub fn on_tick(&mut self) {
        if self.phase == AgentPhase::WaitingForLlm || self.phase == AgentPhase::ExecutingTools {
            self.spinner_frame = self.spinner_frame.wrapping_add(1);
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn tool_call_detail(name: &str, args: &serde_json::Value) -> String {
    let s = |key: &str| args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string());
    match name {
        "read_file" | "write_file" | "edit_file" => s("path").unwrap_or_default(),
        "bash" => {
            let cmd = s("command").unwrap_or_default();
            if cmd.len() > 60 {
                format!("{}...", &cmd[..59])
            } else {
                cmd
            }
        }
        "grep" => {
            let pattern = s("pattern").unwrap_or_default();
            let dir = s("path").unwrap_or_default();
            if dir.is_empty() {
                pattern
            } else {
                format!("{pattern} in {dir}")
            }
        }
        "glob" => s("pattern").unwrap_or_default(),
        "tree" => s("path").unwrap_or_else(|| ".".to_string()),
        _ => String::new(),
    }
}
