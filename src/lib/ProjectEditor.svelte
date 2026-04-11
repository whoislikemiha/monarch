<script lang="ts">
  import { invoke } from "$lib/api";
  import type { Project } from "./types";

  let {
    project,
    agents,
    onclose,
    onupdate,
  }: {
    project: Project;
    agents: { id: string; projectId?: string }[];
    onclose: () => void;
    onupdate: (project: Project) => void;
  } = $props();

  let instructionsText = $state("");
  let saving = $state(false);
  let saved = $state(false);
  let dirty = $state(false);
  let reloading = $state(false);
  let editingName = $state(false);
  let nameInput = $state("");
  let nameInputEl: HTMLInputElement | undefined = $state(undefined);

  // Sync from prop on open
  $effect(() => {
    instructionsText = project.instructions || "";
    nameInput = project.name;
    dirty = false;
    saved = false;
    editingName = false;
  });

  function startRename() {
    nameInput = project.name;
    editingName = true;
    // Focus after the DOM updates
    setTimeout(() => nameInputEl?.select(), 0);
  }

  async function commitRename() {
    const trimmed = nameInput.trim();
    if (!trimmed || trimmed === project.name) {
      editingName = false;
      return;
    }
    try {
      await invoke("db_rename_project", { projectId: project.id, name: trimmed });
      onupdate({ ...project, name: trimmed });
    } catch (e) {
      console.error("Failed to rename project:", e);
    }
    editingName = false;
  }

  function handleNameKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      commitRename();
    }
    if (e.key === "Escape") {
      editingName = false;
    }
  }

  async function save() {
    saving = true;
    try {
      const value = instructionsText.trim() || null;
      await invoke("db_update_project_instructions", {
        projectId: project.id,
        instructions: value,
      });

      // Propagate to all running agents in this project — send updated
      // instructions so the sidecar can rebuild the system prompt live.
      const projectAgents = agents.filter((a) => a.projectId === project.id);
      for (const agent of projectAgents) {
        try {
          await invoke("send_command", {
            id: agent.id,
            commandJson: JSON.stringify({
              type: "set_custom_prompt",
              prompt: null,
              projectInstructions: value,
            }),
          });
        } catch { /* agent may not be running */ }
      }

      onupdate({ ...project, instructions: value });
      saved = true;
      dirty = false;
      setTimeout(() => (saved = false), 2000);
    } catch (e) {
      console.error("Failed to save project instructions:", e);
    }
    saving = false;
  }

  async function reloadFromFiles() {
    reloading = true;
    try {
      const content = await invoke<string | null>("read_project_instructions", {
        cwd: project.rootPath,
      });
      if (content !== null) {
        instructionsText = content;
        dirty = true;
      }
    } catch (e) {
      console.error("Failed to reload instructions:", e);
    }
    reloading = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onclose();
      e.stopPropagation();
    }
    if (e.key === "s" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      save();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={onclose} role="presentation">
  <div
    class="editor-panel"
    onclick={(e: MouseEvent) => e.stopPropagation()}
    onkeydown={handleKeydown}
    role="dialog"
    tabindex="-1"
  >
    <div class="editor-header">
      <div class="header-title">
        <span class="project-icon">/</span>
        {#if editingName}
          <input
            class="name-input"
            bind:this={nameInputEl}
            bind:value={nameInput}
            onblur={commitRename}
            onkeydown={handleNameKeydown}
          />
        {:else}
          <button class="name-btn" onclick={startRename} title="Click to rename">
            <h2>{project.name}</h2>
          </button>
        {/if}
        <span class="header-path">{project.rootPath}</span>
      </div>
      <div class="editor-actions">
        <button
          class="btn-reload"
          onclick={reloadFromFiles}
          disabled={reloading}
        >
          {reloading ? "Reloading..." : "Reload from files"}
        </button>
        <button
          class="btn-save"
          onclick={save}
          disabled={saving || !dirty}
        >
          {#if saved}
            Saved
          {:else if saving}
            Saving...
          {:else}
            Save
          {/if}
          <span class="shortcut">Ctrl+S</span>
        </button>
        <button class="btn-close" onclick={onclose}>Close</button>
      </div>
    </div>

    <div class="editor-hint">
      Project instructions injected into every shadow's system prompt.
      Loaded from AGENTS.md / CLAUDE.md on first detection. Edit here to override.
      {#if dirty}
        <span class="unsaved-badge">Unsaved changes</span>
      {/if}
    </div>

    <textarea
      class="prompt-textarea"
      bind:value={instructionsText}
      oninput={() => { dirty = true; saved = false; }}
      spellcheck="false"
      placeholder="No project instructions. Add AGENTS.md or CLAUDE.md to the project root, or type here."
    ></textarea>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .editor-panel {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
    width: 800px;
    max-width: 90vw;
    height: 80vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .editor-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    gap: 12px;
  }

  .header-title {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .name-btn {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    border-radius: 4px;
    transition: background 0.15s;
  }

  .name-btn:hover {
    background: var(--accent-bg-subtle);
  }

  .name-input {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    background: var(--bg-panel-2);
    border: 1px solid var(--accent);
    border-radius: 4px;
    padding: 2px 6px;
    outline: none;
    width: 200px;
  }

  .header-title h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    white-space: nowrap;
  }

  .project-icon {
    color: var(--accent);
    font-weight: 700;
    font-size: 14px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .header-path {
    font-size: 10px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .editor-actions {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-shrink: 0;
  }

  .editor-hint {
    padding: 8px 20px;
    font-size: 11px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .unsaved-badge {
    font-size: 10px;
    padding: 2px 8px;
    border-radius: 4px;
    background: var(--unsaved-badge-bg);
    color: var(--warning);
  }

  .prompt-textarea {
    flex: 1;
    background: var(--bg-panel-2);
    color: var(--text-primary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    line-height: 1.6;
    padding: 16px 20px;
    border: none;
    outline: none;
    resize: none;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .prompt-textarea::placeholder {
    color: var(--text-muted);
  }

  .btn-save {
    padding: 6px 14px;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: var(--text-on-accent);
    font-size: 11px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
    transition: background 0.15s;
  }

  .btn-save:hover {
    background: var(--accent-hover);
  }

  .btn-save:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-reload {
    padding: 6px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-reload:hover {
    background: var(--bg-panel-2);
  }

  .btn-reload:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-close {
    padding: 6px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-close:hover {
    background: var(--bg-panel-2);
  }

  .shortcut {
    font-size: 9px;
    opacity: 0.6;
  }
</style>
