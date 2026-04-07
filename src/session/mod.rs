//! Persist, resume, and list conversations across restarts.
//!
//! Sessions are stored as JSON in `~/.config/allux/sessions/`.
//! Each session file contains the conversation history (minus system prompt)
//! and metadata (model, workspace, timestamp).

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::ollama::types::Message;

/// Metadata + conversation stored on disk.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionFile {
    /// Human-readable name (auto-generated or user-set).
    pub name: String,
    /// Model used in this session.
    pub model: String,
    /// Workspace root path.
    pub workspace: String,
    /// Unix timestamp when session was created.
    pub created_at: u64,
    /// Unix timestamp of last save.
    pub updated_at: u64,
    /// Conversation messages (excludes system prompt).
    pub messages: Vec<Message>,
}

/// Return the sessions directory, creating it if needed.
fn sessions_dir() -> Result<PathBuf> {
    let dir = crate::config::Config::config_dir().join("sessions");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Generate a short session name from the first user message.
fn auto_name(messages: &[Message]) -> String {
    let first_user = messages.iter().find(|m| m.role == "user");
    match first_user {
        Some(m) => {
            let words: Vec<&str> = m.content.split_whitespace().take(6).collect();
            let preview = words.join(" ");
            if preview.len() > 50 {
                format!("{}\u{2026}", &preview[..49])
            } else {
                preview
            }
        }
        None => "empty session".into(),
    }
}

/// Save conversation history to a session file. Returns the file path.
pub fn save(
    messages: &[Message],
    model: &str,
    workspace: &Path,
    existing_id: Option<&str>,
) -> Result<PathBuf> {
    let dir = sessions_dir()?;

    // Filter out system messages (they're rebuilt on load)
    let user_messages: Vec<Message> = messages
        .iter()
        .filter(|m| m.role != "system")
        .cloned()
        .collect();

    let now = now_unix();

    let (path, session) = if let Some(id) = existing_id {
        let path = dir.join(format!("{id}.json"));
        // Update existing session
        let mut existing = load_file(&path).unwrap_or_else(|_| SessionFile {
            name: auto_name(&user_messages),
            model: model.to_string(),
            workspace: workspace.display().to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        });
        existing.messages = user_messages;
        existing.updated_at = now;
        existing.model = model.to_string();
        (path, existing)
    } else {
        let id = format!("{}", now);
        let path = dir.join(format!("{id}.json"));
        let session = SessionFile {
            name: auto_name(&user_messages),
            model: model.to_string(),
            workspace: workspace.display().to_string(),
            created_at: now,
            updated_at: now,
            messages: user_messages,
        };
        (path, session)
    };

    let json = serde_json::to_string_pretty(&session)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

fn load_file(path: &Path) -> Result<SessionFile> {
    let content = std::fs::read_to_string(path)?;
    let session: SessionFile = serde_json::from_str(&content)?;
    Ok(session)
}

/// Load a session by ID (filename without .json extension).
pub fn load(id: &str) -> Result<SessionFile> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{id}.json"));
    load_file(&path)
}

/// Summary of a saved session for listing.
#[derive(Debug)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub model: String,
    pub message_count: usize,
    pub updated_at: u64,
}

/// List all saved sessions, sorted by most recent first.
pub fn list() -> Result<Vec<SessionSummary>> {
    let dir = sessions_dir()?;
    let mut sessions = Vec::new();

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        if let Ok(session) = load_file(&path) {
            sessions.push(SessionSummary {
                id,
                name: session.name,
                model: session.model,
                message_count: session.messages.len(),
                updated_at: session.updated_at,
            });
        }
    }

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

/// Delete a session by ID.
pub fn delete(id: &str) -> Result<()> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_and_load_roundtrip() {
        let messages = vec![
            Message::user("Hello, help me with my project"),
            Message::assistant("Sure! What do you need?"),
        ];
        let workspace = std::env::temp_dir();
        let path = save(&messages, "test-model:7b", &workspace, None).unwrap();
        let id = path.file_stem().unwrap().to_str().unwrap();

        let loaded = load(id).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.model, "test-model:7b");
        assert!(loaded.name.contains("Hello"));

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_list_sessions() {
        // Just verify it doesn't crash
        let sessions = list().unwrap();
        // sessions may or may not be empty depending on test environment
        let _ = sessions;
    }

    #[test]
    fn test_auto_name() {
        let msgs = vec![Message::user("Fix the authentication bug in src/auth.rs please")];
        let name = auto_name(&msgs);
        assert!(name.contains("Fix"));
        assert!(name.len() <= 51); // 50 + potential ellipsis
    }
}
