use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Decision returned after asking the user whether to allow an action.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Run this once; ask again next time.
    AllowOnce,
    /// Run and remember for the rest of the session.
    AllowSession,
    /// Allow the entire family of this command for the session.
    AllowFamily,
    /// Allow and persist to workspace `.allux/permissions.json`.
    AllowWorkspace,
    /// Allow and persist to global `~/.config/allux/permissions.json`.
    AllowGlobal,
    /// Reject; send an error back to the LLM.
    Deny,
}

/// Grants persisted to disk (workspace or global).
#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedGrants {
    /// Exact command strings that are always allowed.
    #[serde(default)]
    exact: Vec<String>,
    /// Command prefixes (families) that are always allowed.
    #[serde(default)]
    prefixes: Vec<String>,
}

impl PersistedGrants {
    fn is_granted(&self, command: &str) -> bool {
        if self.exact.iter().any(|c| c == command) {
            return true;
        }
        for p in &self.prefixes {
            if command == p || command.starts_with(&format!("{p} ")) {
                return true;
            }
        }
        false
    }

    fn add_exact(&mut self, command: &str) {
        let s = command.to_string();
        if !self.exact.contains(&s) {
            self.exact.push(s);
        }
    }

    fn add_prefix(&mut self, command: &str) {
        let family = command.split_whitespace().next().unwrap_or(command).to_string();
        if !self.prefixes.contains(&family) {
            self.prefixes.push(family);
        }
    }

    fn load(path: &Path) -> Self {
        if !path.is_file() {
            return Self::default();
        }
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// 4-scope permission store: in-memory (once + session) and disk (workspace + global).
pub struct PermissionStore {
    session_exact: HashSet<String>,
    session_prefixes: HashSet<String>,
    workspace_grants: PersistedGrants,
    global_grants: PersistedGrants,
    workspace_path: PathBuf,
    global_path: PathBuf,
}

impl PermissionStore {
    /// Create a new store, loading any persisted grants from disk.
    pub fn new(workspace_root: &Path) -> Self {
        let workspace_path = workspace_root.join(".allux").join("permissions.json");
        let global_path = crate::config::Config::config_dir().join("permissions.json");

        Self {
            session_exact: HashSet::new(),
            session_prefixes: HashSet::new(),
            workspace_grants: PersistedGrants::load(&workspace_path),
            global_grants: PersistedGrants::load(&global_path),
            workspace_path,
            global_path,
        }
    }

    /// True if the command is allowed by any scope (session, workspace, or global).
    pub fn is_granted(&self, command: &str) -> bool {
        // Session exact
        if self.session_exact.contains(command) {
            return true;
        }
        // Session prefixes
        for p in &self.session_prefixes {
            if command == p || command.starts_with(&format!("{p} ")) {
                return true;
            }
        }
        // Workspace
        if self.workspace_grants.is_granted(command) {
            return true;
        }
        // Global
        if self.global_grants.is_granted(command) {
            return true;
        }
        false
    }

    /// Grant exactly this command for the remainder of the session.
    pub fn grant_session(&mut self, command: &str) {
        self.session_exact.insert(command.to_string());
    }

    /// Grant all commands starting with the base name of this command (session only).
    pub fn grant_family(&mut self, command: &str) {
        let family = command.split_whitespace().next().unwrap_or(command);
        self.session_prefixes.insert(family.to_string());
    }

    /// Grant this command family for this workspace (persisted to disk).
    pub fn grant_workspace(&mut self, command: &str) {
        self.workspace_grants.add_prefix(command);
        if let Err(e) = self.workspace_grants.save(&self.workspace_path) {
            eprintln!("Warning: could not save workspace permissions: {e}");
        }
    }

    /// Grant this command family globally (persisted to disk).
    pub fn grant_global(&mut self, command: &str) {
        self.global_grants.add_prefix(command);
        if let Err(e) = self.global_grants.save(&self.global_path) {
            eprintln!("Warning: could not save global permissions: {e}");
        }
    }

    /// Parse user input into a `Decision`.
    /// - `y`, `yes`, `1`       -> AllowOnce
    /// - `s`, `session`        -> AllowSession
    /// - `a`, `family`, `f`    -> AllowFamily
    /// - `w`, `workspace`      -> AllowWorkspace
    /// - `g`, `global`         -> AllowGlobal
    /// - anything else         -> Deny
    pub fn parse_input(s: &str) -> Decision {
        match s.trim().to_lowercase().as_str() {
            "y" | "yes" | "1" => Decision::AllowOnce,
            "s" | "session" => Decision::AllowSession,
            "f" | "family" | "a" => Decision::AllowFamily,
            "w" | "workspace" => Decision::AllowWorkspace,
            "g" | "global" => Decision::AllowGlobal,
            _ => Decision::Deny,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> PermissionStore {
        let dir = std::env::temp_dir().join("allux_perm_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        PermissionStore::new(&dir)
    }

    #[test]
    fn test_exact_grant() {
        let mut store = temp_store();
        store.grant_session("cargo test");
        assert!(store.is_granted("cargo test"));
        assert!(!store.is_granted("cargo build"));
    }

    #[test]
    fn test_family_grant() {
        let mut store = temp_store();
        store.grant_family("npm install");
        assert!(store.is_granted("npm install"));
        assert!(store.is_granted("npm start"));
        assert!(store.is_granted("npm"));
        assert!(!store.is_granted("npm-check"));
    }

    #[test]
    fn test_workspace_grant_persists() {
        let dir = std::env::temp_dir().join("allux_perm_persist_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Grant and save
        {
            let mut store = PermissionStore::new(&dir);
            store.grant_workspace("cargo test --lib");
            assert!(store.is_granted("cargo test --lib"));
            assert!(store.is_granted("cargo build")); // "cargo" prefix
        }

        // Reload from disk
        {
            let store = PermissionStore::new(&dir);
            assert!(store.is_granted("cargo test --lib")); // persisted as prefix "cargo"
            assert!(store.is_granted("cargo build"));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_global_grant_persists() {
        let dir = std::env::temp_dir().join("allux_perm_global_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut store = PermissionStore::new(&dir);
        store.grant_global("git status");
        assert!(store.is_granted("git log"));
        assert!(store.is_granted("git diff"));
    }

    #[test]
    fn test_parse_input() {
        assert_eq!(PermissionStore::parse_input("y"), Decision::AllowOnce);
        assert_eq!(PermissionStore::parse_input("f"), Decision::AllowFamily);
        assert_eq!(PermissionStore::parse_input("a"), Decision::AllowFamily);
        assert_eq!(PermissionStore::parse_input("w"), Decision::AllowWorkspace);
        assert_eq!(PermissionStore::parse_input("g"), Decision::AllowGlobal);
        assert_eq!(PermissionStore::parse_input("no"), Decision::Deny);
    }
}
