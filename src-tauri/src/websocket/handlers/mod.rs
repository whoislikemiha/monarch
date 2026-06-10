pub mod agents;
pub mod misc;
pub mod plans;
pub mod projects;
pub mod quests;
pub mod sessions;
pub mod memories;

use serde_json::Value;

use crate::error::MonarchError;

// ---- Helpers ----

pub(crate) fn str_field(args: &Value, key: &str) -> Result<String, MonarchError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| MonarchError::invalid_input(format!("Missing required field: {}", key)))
}

pub(crate) fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
