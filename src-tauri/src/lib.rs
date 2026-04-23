mod agent;
mod agent_state;
mod db;
mod error;
mod mention;
mod models;
mod persistence;
mod project;
mod sidecar_protocol;
mod thinking_config;
mod toolbox;
mod util;
mod ws;
mod zoom;

pub use error::MonarchError;

use agent::AgentManager;
use db::Database;
use models::ModelCache;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, RunEvent};
use tauri_specta::{collect_commands, Builder};

/// Upper bound on how long the `ExitRequested` hook waits for the sidecar to
/// exit gracefully after stdin close before hard-killing. Keeps window-close
/// latency bounded while giving `disposeAll()` room to finish.
const SIDECAR_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(1500);

/// Build the tauri-specta command collection for **type export only**.
/// Runtime invocation still goes through `tauri::generate_handler!` in
/// `run()`; this Builder exists purely so `cargo test` can emit
/// `src/lib/bindings.ts` with typed wrappers for Phase 2 to import.
///
/// MON-35: `agent::spawn_agent` used to be omitted because specta's
/// `SpectaFn` trait caps arg count at 10 and the command took 13 (three
/// state extractors + ten value params). The ten value params now collapse
/// into `agent::SpawnAgentRequest`, so the command fits under the cap and
/// is registered below.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        // Force LiveAgentState to be emitted as a named type export so Phase
        // 2 can `import type { LiveAgentState } from '$lib/bindings'` rather
        // than duplicating the inline shape from the getAgentState signature.
        .typ::<agent_state::LiveAgentState>()
        // MON-83: force QuestRow so `db_get_quest` (Option<QuestRow>) emits
        // a named type reference rather than an anonymous inline shape.
        .typ::<db::QuestRow>()
        // MON-82: same reason for ClassificationRow
        // (db_get_classification_for_message).
        .typ::<db::ClassificationRow>()
        .commands(collect_commands![
        // Agent lifecycle (sidecar-based)
        agent::commands::spawn_agent,
        agent::commands::send_command,
        agent::commands::kill_agent,
        agent::commands::get_agent_state,
        agent::commands::rebuild_agent_state_from_session,
        agent::commands::load_session_context,
        agent::commands::new_agent_session,
        agent::commands::switch_agent_session,
        agent::commands::respond_extension_ui,
        // Models
        models::get_models,
        models::get_provider_auth_status,
        // Prompt file management
        persistence::get_agent_prompt,
        persistence::save_agent_prompt,
        persistence::get_prompts_dir,
        persistence::save_avatar_image,
        persistence::read_avatar_data_url,
        persistence::read_attachment_data_url,
        // SQLite persistence
        db::db_upsert_agent,
        db::db_update_agent,
        db::db_get_agents,
        db::db_archive_agent,
        db::db_unarchive_agent,
        db::db_delete_agent,
        db::db_create_session,
        db::db_get_sessions,
        db::db_save_message,
        db::db_get_messages,
        db::db_get_messages_with_ancestry,
        db::db_save_memory,
        db::db_get_memories,
        db::db_log_event,
        // Agent templates
        db::db_list_agent_templates,
        db::db_save_agent_template,
        db::db_delete_agent_template,
        // Projects
        db::db_upsert_project,
        db::db_get_projects,
        db::db_get_project_by_path,
        db::db_rename_project,
        db::db_update_project_instructions,
        db::db_delete_project,
        // Project detection
        project::commands::detect_project,
        project::commands::read_project_instructions,
        // Mention autocomplete (MON-76)
        mention::list_paths,
        // UI state
        db::db_get_ui_state,
        db::db_set_ui_state,
        // Agent stats
        db::db_get_agent_stats,
        // Quests (MON-83)
        db::db_create_quest,
        db::db_update_quest,
        db::db_get_quest,
        db::db_list_quests_for_agent,
        db::db_get_quest_tree_for_root,
        db::db_record_quest_event,
        db::db_list_quest_events,
        // MON-82
        db::db_list_classifications_for_agent,
        db::db_get_classification_for_message,
        // Toolbox
        toolbox::toolbox_list_tools,
        toolbox::placeholder::toolbox_placeholder_ping,
        // Zoom
        zoom::set_zoom,
        // Thinking defaults (MON-78)
        thinking_config::get_thinking_default,
        thinking_config::get_thinking_config_path,
    ])
}

/// Export the tauri-specta command collection to `src/lib/bindings.ts`.
///
/// Invoked from `main.rs` when the process is started with `--export-bindings`.
/// We do this via the main binary rather than a `cargo test` target because
/// on Windows the test binaries fail to start with `STATUS_ENTRYPOINT_NOT_FOUND`
/// due to tauri runtime DLL resolution quirks. `monarch.exe` has the correct
/// DLL neighbours (WebView2Loader etc.) so running it with a flag to export
/// and exit before starting the runtime is the most reliable path.
pub fn export_bindings() -> Result<(), MonarchError> {
    use specta_typescript::Typescript;
    let header = "// This file is auto-generated by `cargo run -- --export-bindings`.\n// Do not edit by hand — update the Rust command signatures and regenerate.\n";
    let output_path = "../src/lib/bindings.ts";
    specta_builder()
        .export(Typescript::default().header(header), output_path)
        .map_err(|e| MonarchError::persistence(format!("Failed to export TypeScript bindings: {}", e)))?;

    // Post-process: specta rc.24 emits `serde_json::Value` references as raw
    // Rust type names (`Value`, `Vec<Value>`) instead of translating them to
    // TS. Inject helper type aliases after the header so those references
    // resolve. This is a workaround and should be removed once specta fixes
    // the `serde_json` feature's TS emission.
    let contents = std::fs::read_to_string(output_path)?;
    let patch = "\n// Specta rc.24 serde_json::Value TS-emission workaround.\ntype Value = unknown;\ntype Vec<T> = T[];\n";
    // Insert after the very first blank line (end of file header).
    let patched = if let Some(idx) = contents.find("\n\n") {
        let (head, tail) = contents.split_at(idx);
        format!("{}{}\n{}", head, patch, tail.trim_start_matches('\n'))
    } else {
        format!("{}{}", patch, contents)
    };

    // Route every typed command through `$lib/api` so the WS fallback path
    // (used when the frontend runs outside the Tauri webview) still fires.
    // Specta emits `import { invoke as __TAURI_INVOKE } from "@tauri-apps/api/core";`
    // which calls the Tauri-only invoke directly. Rewrite it to the shim.
    let patched = patched.replace(
        "import { invoke as __TAURI_INVOKE } from \"@tauri-apps/api/core\";",
        "import { invoke as __TAURI_INVOKE } from \"./api\";",
    );

    std::fs::write(output_path, patched)?;

    eprintln!("[monarch] Exported src/lib/bindings.ts");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if std::env::args().any(|a| a == "--export-bindings") {
        if let Err(e) = export_bindings() {
            eprintln!("{}", e);
            std::process::exit(1);
        }
        return;
    }

    let database = Arc::new(
        tauri::async_runtime::block_on(Database::new())
            .expect("Failed to initialize database"),
    );
    let agent_mgr = Arc::new(AgentManager::new(database.clone()));
    let model_cache = Arc::new(ModelCache::new());

    // Clones for the WS server
    let ws_db = database.clone();
    let ws_agent_mgr = agent_mgr.clone();
    let ws_model_cache = model_cache.clone();
    let ws_broadcast = agent_mgr.ws_broadcast.clone();

    // Note: specta_builder() is used purely for type export (cargo test).
    // Runtime invocation goes through tauri::generate_handler! below.
    // MON-35 collapsed spawn_agent's ten value params into SpawnAgentRequest,
    // so specta and the runtime handler now register the same command shape.
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(agent_mgr)
        .manage(model_cache)
        .manage(database)
        .invoke_handler(tauri::generate_handler![
            agent::commands::spawn_agent,
            agent::commands::send_command,
            agent::commands::kill_agent,
            agent::commands::get_agent_state,
            agent::commands::rebuild_agent_state_from_session,
            agent::commands::load_session_context,
            agent::commands::new_agent_session,
            agent::commands::switch_agent_session,
            agent::commands::respond_extension_ui,
            models::get_models,
            models::get_provider_auth_status,
            persistence::get_agent_prompt,
            persistence::save_agent_prompt,
            persistence::get_prompts_dir,
            persistence::save_avatar_image,
        persistence::read_avatar_data_url,
        persistence::read_attachment_data_url,
            db::db_upsert_agent,
            db::db_update_agent,
            db::db_get_agents,
            db::db_archive_agent,
            db::db_unarchive_agent,
            db::db_delete_agent,
            db::db_create_session,
            db::db_get_sessions,
            db::db_save_message,
            db::db_get_messages,
            db::db_get_messages_with_ancestry,
            db::db_save_memory,
            db::db_get_memories,
            db::db_log_event,
            db::db_list_agent_templates,
            db::db_save_agent_template,
            db::db_delete_agent_template,
            db::db_upsert_project,
            db::db_get_projects,
            db::db_get_project_by_path,
            db::db_rename_project,
            db::db_update_project_instructions,
            db::db_delete_project,
            project::commands::detect_project,
            project::commands::read_project_instructions,
            mention::list_paths,
            db::db_get_ui_state,
            db::db_set_ui_state,
            db::db_get_agent_stats,
            db::db_create_quest,
            db::db_update_quest,
            db::db_get_quest,
            db::db_list_quests_for_agent,
            db::db_get_quest_tree_for_root,
            db::db_record_quest_event,
            db::db_list_quest_events,
            db::db_list_classifications_for_agent,
            db::db_get_classification_for_message,
            toolbox::toolbox_list_tools,
            toolbox::placeholder::toolbox_placeholder_ping,
            zoom::set_zoom,
        ])
        .setup(move |app| {
            // Store AppHandle so WS-initiated commands can access the sidecar
            ws_agent_mgr.set_app_handle(app.handle().clone());

            // Start the WebSocket bridge server
            let ws_state = Arc::new(ws::WsState {
                db: ws_db,
                agent_mgr: ws_agent_mgr,
                model_cache: ws_model_cache,
                broadcast_rx: ws_broadcast,
            });
            std::thread::spawn(move || {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create WS tokio runtime")
                    .block_on(ws::start_ws_server(ws_state));
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // MON-36: on window close / app exit, gracefully shut down the
            // Node sidecar before the process tears down. Closing stdin is
            // the sidecar's graceful-shutdown protocol (see
            // `sidecar/src/index.ts` `rl.on("close", shutdown)`); the
            // manager handles the bounded wait + hard-kill fallback.
            if let RunEvent::ExitRequested { .. } = event {
                let mgr = app_handle.state::<Arc<AgentManager>>();
                mgr.shutdown_sidecar(SIDECAR_SHUTDOWN_TIMEOUT);
            }
        });
}
