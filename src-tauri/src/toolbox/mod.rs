//! Pluggable toolbox module — backend side of the MON-12 tool registry.
//!
//! Each tool may (optionally) expose typed Tauri commands living in its own
//! submodule here (see `placeholder.rs`). The list of `ToolDescriptor` entries
//! returned by `toolbox_list_tools` lets the frontend cross-check registration.

pub mod placeholder;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor {
    pub id: String,
    pub title: String,
}

fn descriptors() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            id: "context-inspector".to_string(),
            title: "Context".to_string(),
        },
        ToolDescriptor {
            id: "placeholder".to_string(),
            title: "Placeholder".to_string(),
        },
    ]
}

#[tauri::command]
pub fn toolbox_list_tools() -> Vec<ToolDescriptor> {
    descriptors()
}

pub fn ws_toolbox_list_tools() -> Vec<ToolDescriptor> {
    descriptors()
}
