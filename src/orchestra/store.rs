use anyhow::Result;

/// Summary of a saved Orchestra run for listing.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub run_id: String,
    pub goal_preview: String,
    pub phase: String,
    pub created_at: u64,
    pub updated_at: u64,
}

/// List all saved Orchestra runs, most recent first.
pub fn list_runs() -> Result<Vec<RunSummary>> {
    // Phase 3 implementation
    Ok(Vec::new())
}
