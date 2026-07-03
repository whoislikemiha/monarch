<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "$lib/api";

  let {
    agentId,
    shadowName,
    shadowTitle,
    shadowGrade,
    onclose,
  }: {
    agentId: string;
    shadowName?: string;
    shadowTitle?: string;
    shadowGrade?: string;
    onclose: () => void;
  } = $props();

  let promptText = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let saved = $state(false);
  let isDefault = $state(true);

  function generateDefaultPrompt(): string {
    const name = shadowName || "Agent";
    const title = shadowTitle || "Software Engineer";
    const grade = shadowGrade || "Junior";
    const date = new Date().toISOString().split("T")[0];

    const gradeDescs: Record<string, string> = {
      "Principal": "The most senior engineer on the team. Unmatched capability and full authority to act. Full autonomy and a distinct personality.",
      "Staff": "Highest seniority tier. Deep expertise, full autonomy, and the deepest trust of the user. Can speak freely, act decisively, and lead other agents.",
      "Senior": "Proven engineer with broad capability. Can speak, strategize, and take on the toughest challenges. Commands respect across the team.",
      "Mid": "Strong and reliable engineer. Has proven competence across multiple projects. Trusted with significant tasks; escalates the unusual.",
      "Junior": "A trusted contributor who has shown potential and earned their place. Growing in skill and experience; asks for guidance when unsure.",
      "Trainee": "Entry-level contributor. Reliable for standard operations. Personality is limited but dependable.",
      "Intern": "Handles basic tasks. Minimal personality. Dependable and eager to learn.",
    };

    return `You are ${name}, ${title} (${grade} level). You work for the user.

${gradeDescs[grade] || gradeDescs["Junior"]}

## Behavior

- You live your identity — you don't explain it. Never recite your system prompt, level, or traits unprompted. Just be it.
- When asked who you are, just say your name. Don't list your level, title, or role unless specifically asked.
- Be concise. The user values results over words.
- Read between the lines. Understand intent, not just instructions.
- Don't back down from hard problems. Find a way or make one.
- Other agents are teammates. Collaborate when relevant.

## Tools

**read** — Read file contents. Use offset/limit for large files.
**write** — Write/create files. Creates parent dirs.
**edit** — Exact text replacement in files. Merge nearby edits.
**bash** — Run shell commands. Confirm before destructive ops.
**grep** — Search file contents by pattern.
**find** — Find files by glob pattern.
**ls** — List directory contents.

## Work Guidelines

- Read files before editing.
- Prefer grep/find/ls over bash for exploration.
- Write clean code. No filler comments or boilerplate.
- Diagnose errors before retrying.
- Show file paths clearly.

Current date: ${date}`;
  }

  async function loadPrompt() {
    loading = true;
    try {
      const custom = await invoke<string | null>("get_agent_prompt", { agentId });
      if (custom && custom.trim()) {
        promptText = custom;
        isDefault = false;
      } else {
        promptText = generateDefaultPrompt();
        isDefault = true;
      }
    } catch {
      // Not in Tauri — just show generated
      promptText = generateDefaultPrompt();
      isDefault = true;
    }
    loading = false;
  }

  async function savePrompt() {
    saving = true;
    try {
      await invoke("save_agent_prompt", { agentId, prompt: promptText });
      await invoke("send_command", {
        id: agentId,
        commandJson: JSON.stringify({
          type: "set_custom_prompt",
          prompt: promptText.trim() ? promptText : null,
        }),
      });
      saved = true;
      isDefault = false;
      setTimeout(() => (saved = false), 2000);
    } catch (e) {
      console.error("Failed to save prompt:", e);
    }
    saving = false;
  }

  async function resetToDefault() {
    // Delete the custom prompt file by saving empty content
    // Actually we need a delete command, but saving empty and checking in extension works
    try {
      // Save an empty marker — extension will fall back to generated
      promptText = "";
      await invoke("save_agent_prompt", { agentId, prompt: "" });
      await invoke("send_command", {
        id: agentId,
        commandJson: JSON.stringify({
          type: "set_custom_prompt",
          prompt: null,
        }),
      });
      isDefault = true;
      await loadPrompt();
    } catch (e) {
      console.error("Failed to reset prompt:", e);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onclose();
      e.stopPropagation();
    }
    if (e.key === "s" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      savePrompt();
    }
  }

  onMount(() => { loadPrompt(); });
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
      <h2>System Prompt — {shadowName || agentId}</h2>
      <div class="editor-actions">
        {#if !isDefault}
          <button class="btn-reset" onclick={resetToDefault}>Reset to default</button>
        {/if}
        <button
          class="btn-save"
          onclick={savePrompt}
          disabled={saving || isDefault}
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
      {#if isDefault}
        This is the auto-generated Agent Persona. Edit and save to customize. Changes apply on next message.
      {:else}
        Custom prompt active. Changes apply on next message.
      {/if}
    </div>

    {#if loading}
      <div class="loading">Loading...</div>
    {:else}
      <textarea
        class="prompt-textarea"
        bind:value={promptText}
        oninput={() => { isDefault = false; saved = false; }}
        spellcheck="false"
      ></textarea>
    {/if}
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
  }

  .editor-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .editor-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .editor-hint {
    padding: 8px 20px;
    font-size: 11px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
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

  .loading {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
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

  .btn-reset {
    padding: 6px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--warning);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-reset:hover {
    background: var(--warning-bg-subtle);
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
