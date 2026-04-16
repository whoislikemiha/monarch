<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "$lib/api";
  import { SHADOW_GRADES, type Agent, type ShadowGrade } from "./types";
  import { agentStore } from "./stores/agentStore.svelte";
  import AvatarPicker from "./avatar/AvatarPicker.svelte";

  let {
    agent,
    onclose,
  }: {
    agent: Agent;
    onclose: () => void;
  } = $props();

  // Shadow identity
  let shadowName = $state(agent.shadow?.shadowName ?? "");
  let shadowTitle = $state(agent.shadow?.shadowTitle ?? "");
  let shadowGrade: ShadowGrade = $state((agent.shadow?.shadowGrade as ShadowGrade) ?? "Knight");

  // Connection / model
  const providers = [
    { label: "Anthropic", value: "anthropic" },
    { label: "OpenAI Codex", value: "openai-codex" },
    { label: "OpenRouter", value: "openrouter" },
    { label: "LM Studio", value: "lmstudio" },
  ];
  const REFRESHABLE_PROVIDERS = new Set(["openrouter", "lmstudio"]);
  const thinkingLevels = ["off", "minimal", "low", "medium", "high", "xhigh"];

  let selectedProvider = $state(agent.provider ?? "openrouter");
  let modelInput = $state(agent.model ?? "");
  let thinkingLevel = $state(agent.thinkingLevel ?? "off");
  let cwd = $state(agent.cwd ?? "");

  // Avatar
  let avatarType = $state<"rive" | "image" | undefined>(agent.avatarType);
  let avatarPath = $state<string | undefined>(agent.avatarPath);

  // Model dropdown state
  interface ModelInfo { id: string; name: string; provider: string; contextWindow?: number; }
  let allModels: ModelInfo[] = $state([]);
  let modelsLoading = $state(false);
  let modelsError: string | null = $state(null);
  let modelFetchToken = 0;
  let showDropdown = $state(false);
  let highlightedIndex = $state(-1);
  let modelInputEl: HTMLInputElement | undefined = $state(undefined);
  const fixedModelId = $derived(selectedProvider === "openai-codex" ? "gpt-5.4" : "");

  let saving = $state(false);
  let saveError: string | null = $state(null);

  let filteredModels = $derived.by(() => {
    const query = modelInput.toLowerCase().trim();
    if (!query) return allModels.slice(0, 50);
    const terms = query.split(/\s+/).filter(Boolean);
    return allModels
      .filter((m) => {
        const haystack = (m.id + " " + m.name).toLowerCase();
        return terms.every((t) => haystack.includes(t));
      })
      .slice(0, 50);
  });

  async function fetchModels(provider: string) {
    const token = ++modelFetchToken;
    modelsLoading = true;
    modelsError = null;
    try {
      const fetched = await invoke<ModelInfo[]>("get_models", { provider });
      if (token !== modelFetchToken) return;
      allModels = fetched;
    } catch (err) {
      if (token !== modelFetchToken) return;
      modelsError = err instanceof Error ? err.message : String(err);
      allModels = [];
    } finally {
      if (token === modelFetchToken) modelsLoading = false;
    }
  }

  $effect(() => {
    const provider = selectedProvider;
    allModels = [];
    modelsError = null;
    modelInput = fixedModelId || (provider === selectedProvider && agent.provider === provider ? agent.model ?? "" : "");
    showDropdown = false;
    highlightedIndex = -1;
    fetchModels(provider);
  });

  function selectModel(model: ModelInfo) {
    modelInput = model.id;
    showDropdown = false;
    highlightedIndex = -1;
  }

  function handleModelKeydown(e: KeyboardEvent) {
    if (fixedModelId) return;
    const models = filteredModels;
    if (!showDropdown || models.length === 0) {
      if (e.key === "ArrowDown" && allModels.length > 0) {
        showDropdown = true;
        highlightedIndex = 0;
        e.preventDefault();
      }
      return;
    }
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        highlightedIndex = Math.min(highlightedIndex + 1, models.length - 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        highlightedIndex = Math.max(highlightedIndex - 1, 0);
        break;
      case "Enter":
        if (highlightedIndex >= 0 && highlightedIndex < models.length) {
          e.preventDefault();
          e.stopPropagation();
          selectModel(models[highlightedIndex]);
        }
        break;
      case "Escape":
        showDropdown = false;
        highlightedIndex = -1;
        break;
    }
  }

  async function browseFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: cwd || undefined,
      title: "Select Working Directory",
    });
    if (selected && typeof selected === "string") cwd = selected;
  }

  async function handleSave() {
    saving = true;
    saveError = null;
    try {
      const sName = shadowName.trim();
      await agentStore.saveAgentEdits({
        id: agent.id,
        name: sName || agent.id,
        shadowName: sName || undefined,
        shadowTitle: shadowTitle.trim() || sName || undefined,
        shadowGrade: sName ? shadowGrade : undefined,
        provider: selectedProvider || undefined,
        model: (fixedModelId || modelInput.trim()) || undefined,
        thinkingLevel: thinkingLevel !== "off" ? thinkingLevel : undefined,
        cwd: cwd.trim() || undefined,
        avatarType,
        avatarPath,
      });
      onclose();
    } catch (e) {
      saveError = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !showDropdown) onclose();
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") handleSave();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={onclose} role="presentation">
  <div
    class="dialog"
    onclick={(e: MouseEvent) => e.stopPropagation()}
    role="dialog"
    tabindex="-1"
  >
    <h2>Edit Shadow</h2>

    <!-- Shadow Identity -->
    <div class="section">
      <span class="label">Shadow Identity</span>
      <div class="row">
        <div class="field">
          <label class="label" for="edit-shadow-name">Name</label>
          <input
            id="edit-shadow-name"
            type="text"
            bind:value={shadowName}
            placeholder="e.g. Igris, Beru, Tusk"
          />
        </div>
        <div class="field">
          <label class="label" for="edit-shadow-grade">Grade</label>
          <select id="edit-shadow-grade" bind:value={shadowGrade}>
            {#each SHADOW_GRADES as grade}
              <option value={grade}>{grade}</option>
            {/each}
          </select>
        </div>
      </div>
      <div class="field">
        <label class="label" for="edit-shadow-title">Title</label>
        <input
          id="edit-shadow-title"
          type="text"
          bind:value={shadowTitle}
          placeholder="e.g. Shadow Commander, The First Shadow"
        />
      </div>
    </div>

    <!-- Provider -->
    <div class="section">
      <span class="label">Provider</span>
      <div class="preset-grid">
        {#each providers as p}
          <button
            class="preset-btn"
            class:active={selectedProvider === p.value}
            onclick={() => (selectedProvider = p.value)}
            type="button"
          >
            {p.label}
          </button>
        {/each}
      </div>
    </div>

    <!-- Model -->
    <div class="field model-field">
      <label class="label" for="edit-model-input">Model</label>
      <div class="model-input-wrap">
        <input
          id="edit-model-input"
          type="text"
          bind:this={modelInputEl}
          bind:value={modelInput}
          placeholder={fixedModelId
            ? "Uses your Pi Codex login"
            : modelsLoading
              ? "Loading models..."
              : modelsError
                ? "Provider unreachable"
                : allModels.length === 0
                  ? "No models available"
                  : "Search models..."}
          readonly={!!fixedModelId}
          onfocus={() => { if (!fixedModelId) showDropdown = true; }}
          onblur={() => setTimeout(() => (showDropdown = false), 200)}
          onkeydown={handleModelKeydown}
          oninput={() => { if (!fixedModelId) { showDropdown = true; highlightedIndex = 0; } }}
          autocomplete="off"
        />
        {#if modelsLoading}
          <span class="loading-indicator"></span>
        {:else if !fixedModelId && REFRESHABLE_PROVIDERS.has(selectedProvider)}
          <button
            class="refresh-btn"
            onmousedown={(e: MouseEvent) => { e.preventDefault(); fetchModels(selectedProvider); }}
            title="Refresh model list"
            type="button"
          >↻</button>
        {/if}
        {#if !fixedModelId && showDropdown && filteredModels.length > 0}
          <div class="model-dropdown">
            {#each filteredModels as model, i (model.id)}
              <button
                class="model-option"
                class:highlighted={i === highlightedIndex}
                class:selected={modelInput === model.id}
                onmousedown={(e: MouseEvent) => { e.preventDefault(); selectModel(model); }}
                onmouseenter={() => (highlightedIndex = i)}
                type="button"
              >
                <span class="model-id">{model.id}</span>
                <span class="model-name">{model.name}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
      {#if modelsError}
        <div class="field-hint error">{modelsError}</div>
      {/if}
    </div>

    <!-- Thinking + CWD -->
    <div class="row">
      <div class="field">
        <label class="label" for="edit-thinking">Thinking</label>
        <select id="edit-thinking" bind:value={thinkingLevel}>
          {#each thinkingLevels as level}
            <option value={level}>{level}</option>
          {/each}
        </select>
      </div>
      <div class="field flex-grow">
        <label class="label" for="edit-cwd">Working Directory</label>
        <div class="cwd-row">
          <input
            id="edit-cwd"
            type="text"
            bind:value={cwd}
            placeholder="/home/miha/project"
          />
          <button class="browse-btn" onclick={browseFolder} type="button" title="Browse">
            ...
          </button>
        </div>
      </div>
    </div>

    <!-- Avatar -->
    <div class="section">
      <span class="label">Avatar</span>
      <AvatarPicker agentId={agent.id} bind:avatarType bind:avatarPath />
    </div>

    {#if saveError}
      <div class="save-error">{saveError}</div>
    {/if}

    <div class="actions">
      <button class="btn-cancel" onclick={onclose} type="button">Cancel</button>
      <button class="btn-save" onclick={handleSave} disabled={saving} type="button">
        {saving ? "Saving…" : "Save"}
        <span class="shortcut">Ctrl+Enter</span>
      </button>
    </div>
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
    padding: 24px;
    overflow-y: auto;
    z-index: 100;
  }

  .dialog {
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
    padding: 24px;
    width: min(560px, 100%);
    max-width: min(560px, calc(100vw - 48px));
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow-y: auto;
    box-shadow: 0 28px 80px var(--shadow-dark);
  }

  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .label {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .row {
    display: flex;
    gap: 12px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field.flex-grow {
    flex: 1;
  }

  input[type="text"],
  select {
    width: 100%;
    padding: 8px 10px;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    box-sizing: border-box;
    outline: none;
  }

  input[type="text"]::placeholder {
    color: var(--text-muted);
    opacity: 0.6;
  }

  input[type="text"]:focus,
  select:focus {
    border-color: var(--accent);
  }

  select {
    cursor: pointer;
    appearance: none;
  }

  .preset-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }

  .preset-btn {
    padding: 6px 8px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .preset-btn:hover {
    background: var(--bg-panel-3);
  }

  .preset-btn.active {
    border-color: var(--accent);
    color: var(--accent);
  }

  .model-field {
    position: relative;
  }

  .model-input-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }

  .model-input-wrap input {
    flex: 1;
    padding-right: 32px;
    width: auto;
  }

  .loading-indicator {
    position: absolute;
    right: 10px;
    width: 12px;
    height: 12px;
    border: 2px solid var(--border-subtle);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .refresh-btn {
    position: absolute;
    right: 8px;
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 14px;
    padding: 2px;
    line-height: 1;
    transition: color 0.15s;
  }

  .refresh-btn:hover {
    color: var(--text-primary);
  }

  .model-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    box-shadow: 0 8px 32px var(--shadow-dark);
    z-index: 200;
    max-height: 240px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .model-option {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 8px 12px;
    background: none;
    border: none;
    border-bottom: 1px solid var(--border-subtle);
    text-align: left;
    cursor: pointer;
    width: 100%;
    transition: background 0.1s;
  }

  .model-option:last-child {
    border-bottom: none;
  }

  .model-option:hover,
  .model-option.highlighted {
    background: var(--bg-hover);
  }

  .model-option.selected {
    background: var(--accent-bg-hover);
  }

  .model-id {
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }

  .model-name {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .cwd-row {
    display: flex;
    gap: 6px;
  }

  .cwd-row input {
    flex: 1;
  }

  .browse-btn {
    padding: 7px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-input);
    color: var(--text-muted);
    font-size: 12px;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
    white-space: nowrap;
  }

  .browse-btn:hover {
    color: var(--text-primary);
    border-color: var(--accent);
  }

  .field-hint {
    margin-top: 4px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .field-hint.error {
    color: var(--color-error);
  }

  .save-error {
    font-size: 12px;
    color: var(--color-error);
    padding: 8px 12px;
    border: 1px solid var(--color-error);
    border-radius: 6px;
    background: color-mix(in srgb, var(--color-error) 8%, transparent);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 4px;
  }

  .btn-cancel,
  .btn-save {
    padding: 8px 16px;
    border-radius: 6px;
    font-size: 13px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .btn-cancel {
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
  }

  .btn-cancel:hover {
    background: var(--bg-panel-3);
  }

  .btn-save {
    background: var(--accent);
    border: 1px solid var(--accent);
    color: white;
  }

  .btn-save:hover:not(:disabled) {
    opacity: 0.9;
  }

  .btn-save:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .shortcut {
    font-size: 10px;
    opacity: 0.6;
  }
</style>
