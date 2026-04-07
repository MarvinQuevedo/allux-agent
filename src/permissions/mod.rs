use std::collections::HashSet;

/// Decision returned after asking the user whether to allow a bash command.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Run this once; ask again next time.
    AllowOnce,
    /// Run and remember for the rest of the session.
    AllowSession,
    /// Allow the entire family of this command (e.g. all 'npm ...' commands).
    AllowFamily,
    /// Reject; send an error back to the LLM.
    Deny,
}

/// In-memory permission store for the current session.
pub struct PermissionStore {
    session_exact: HashSet<String>,
    session_prefixes: HashSet<String>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self {
            session_exact: HashSet::new(),
            session_prefixes: HashSet::new(),
        }
    }

    /// True if the command was previously granted for the session (exact match or family prefix).
    pub fn is_session_granted(&self, command: &str) -> bool {
        if self.session_exact.contains(command) {
            return true;
        }

        // Check prefixes. A prefix "npm" should match exactly "npm" or "npm ..."
        for p in &self.session_prefixes {
            if command == p || command.starts_with(&format!("{p} ")) {
                return true;
            }
        }
        false
    }

    /// Grant exactly this command for the remainder of the session.
    pub fn grant_session(&mut self, command: &str) {
        self.session_exact.insert(command.to_string());
    }

    /// Grant all commands starting with the base name of this command.
    pub fn grant_family(&mut self, command: &str) {
        let family = command.split_whitespace().next().unwrap_or(command);
        self.session_prefixes.insert(family.to_string());
    }

    /// Parse user input into a `Decision`.
    /// - `y`, `yes`, `1`       → AllowOnce
    /// - `s`, `session`        → AllowSession
    /// - `f`, `family`, `a`    → AllowFamily ("a" for all of this type)
    /// - anything else         → Deny
    pub fn parse_input(s: &str) -> Decision {
        match s.trim().to_lowercase().as_str() {
            "y" | "yes" | "1" => Decision::AllowOnce,
            "s" | "session" => Decision::AllowSession,
            "f" | "family" | "a" => Decision::AllowFamily,
            _ => Decision::Deny,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_grant() {
        let mut store = PermissionStore::new();
        store.grant_session("cargo test");
        assert!(store.is_session_granted("cargo test"));
        assert!(!store.is_session_granted("cargo build"));
    }

    #[test]
    fn test_family_grant() {
        let mut store = PermissionStore::new();
        store.grant_family("npm install");
        assert!(store.is_session_granted("npm install"));
        assert!(store.is_session_granted("npm start"));
        assert!(store.is_session_granted("npm"));
        // Should not match partial word command if it's different
        assert!(!store.is_session_granted("npm-check"));
    }

    #[test]
    fn test_parse_input() {
        assert_eq!(PermissionStore::parse_input("y"), Decision::AllowOnce);
        assert_eq!(PermissionStore::parse_input("f"), Decision::AllowFamily);
        assert_eq!(PermissionStore::parse_input("a"), Decision::AllowFamily);
        assert_eq!(PermissionStore::parse_input("no"), Decision::Deny);
    }
}
